use clap::Parser;
use dji_log_parser::frame::Frame;
use dji_log_parser::keychain::KeychainFeaturePoint;
use dji_log_parser::layout::auxiliary::Department;
use dji_log_parser::record::Record;
use dji_log_parser::{DJILog, Error, Result};
use exporters::{CSVExporter, GeoJsonExporter, ImageExporter, JsonExporter, KmlExporter};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod exporters;
mod utils;

const DJI_API_KEY_ENV: &str = "DJI_API_KEY";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// Input log file
    #[arg(value_name = "FILE")]
    filepath: String,

    /// Write JSON output to FILE instead of stdout
    #[arg(short, long)]
    output: Option<String>,

    /// Extract images (use %d for sequence, e.g., image%d.jpeg)
    #[arg(short, long)]
    images: Option<String>,

    /// Extract thumbnails (use %d for sequence, e.g., thumb%d.jpeg)
    #[arg(short, long)]
    thumbnails: Option<String>,

    /// Generate GeoJSON file
    #[arg(short, long)]
    geojson: Option<String>,

    /// Generate KML file
    #[arg(short, long)]
    kml: Option<String>,

    /// Generate CSV file
    #[arg(short, long)]
    csv: Option<String>,

    /// DJI keychain Api Key
    #[arg(short, long)]
    api_key: Option<String>,

    /// Extract raw records instead of normalized frames
    #[arg(short, long)]
    raw: bool,

    /// Custom department for keychain request
    #[arg(long)]
    api_custom_department: Option<u8>,

    /// Custom version for keychain request
    #[arg(long)]
    api_custom_version: Option<u16>,
}

pub(crate) trait Exporter {
    fn export(&self, parser: &DJILog, records: &Vec<Record>, frames: &Vec<Frame>, args: &Cli);
}

fn fetch_keychains(
    parser: &DJILog,
    api_key: &str,
    department: Option<u8>,
    version: Option<u16>,
) -> Result<Vec<Vec<KeychainFeaturePoint>>> {
    let req =
        parser.keychains_request_with_custom_params(department.map(Department::from), version)?;
    let inferred_department = req.department;

    match req.fetch(api_key, None) {
        Ok(keychains) => Ok(keychains),
        Err(error) if department.is_none() && is_invalid_ciphertext_error(&error) => {
            fetch_keychains_with_department_fallbacks(
                parser,
                api_key,
                version,
                inferred_department,
                error,
            )
        }
        Err(error) => Err(error),
    }
}

fn fetch_keychains_with_department_fallbacks(
    parser: &DJILog,
    api_key: &str,
    version: Option<u16>,
    failed_department: u8,
    original_error: Error,
) -> Result<Vec<Vec<KeychainFeaturePoint>>> {
    for department in [Department::DJIFly, Department::DJIGO, Department::DJIPilot] {
        let department_id = u8::from(department.clone());

        if department_id == failed_department {
            continue;
        }

        let req = parser.keychains_request_with_custom_params(Some(department), version)?;
        match req.fetch(api_key, None) {
            Ok(keychains) => {
                eprintln!(
                    "Fetched keychains after retrying with department {}",
                    department_id
                );
                return Ok(keychains);
            }
            Err(error) if is_invalid_ciphertext_error(&error) => {}
            Err(error) => return Err(error),
        }
    }

    Err(original_error)
}

fn is_invalid_ciphertext_error(error: &Error) -> bool {
    matches!(error, Error::ApiError(message) if message == "invalid ciphertext")
}

fn resolve_api_key(cli_api_key: Option<&str>) -> Option<String> {
    cli_api_key
        .and_then(non_empty_string)
        .or_else(|| {
            env::var(DJI_API_KEY_ENV)
                .ok()
                .and_then(|value| non_empty_string(&value))
        })
        .or_else(api_key_from_dotenv)
}

fn api_key_from_dotenv() -> Option<String> {
    let dotenv_path = find_dotenv()?;
    let contents = fs::read_to_string(dotenv_path).ok()?;

    parse_dotenv_api_key(&contents)
}

fn find_dotenv() -> Option<PathBuf> {
    let current_dir_match = env::current_dir()
        .ok()
        .and_then(|path| find_file_upwards(&path, ".env"));

    if current_dir_match.is_some() {
        return current_dir_match;
    }

    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .and_then(|path| find_file_upwards(&path, ".env"))
}

fn find_file_upwards(start: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(dir) = current {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }

        current = dir.parent();
    }

    None
}

fn parse_dotenv_api_key(contents: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let (key, value) = line.split_once('=')?;

        if key.trim() != DJI_API_KEY_ENV {
            return None;
        }

        non_empty_string(strip_dotenv_quotes(value.trim()))
    })
}

fn strip_dotenv_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }

    value
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn main() {
    let args = Cli::parse();

    let bytes = fs::read(&args.filepath).expect("Unable to read file");
    let parser = DJILog::from_bytes(bytes).expect("Unable to parse file");

    let keychains = if parser.version >= 13 {
        let api_key = resolve_api_key(args.api_key.as_deref())
            .expect("API Key is required for version 13 and above");

        let keychains = fetch_keychains(
            &parser,
            &api_key,
            args.api_custom_department,
            args.api_custom_version,
        )
        .expect("Unable to fetch keychain");

        Some(keychains)
    } else {
        None
    };

    let records = parser
        .records(keychains.clone())
        .expect("Unable to parse records");

    let frames = parser.frames(keychains).expect("Unable to parse frames");

    let exporters: Vec<Box<dyn Exporter>> = vec![
        Box::new(JsonExporter),
        Box::new(ImageExporter),
        Box::new(GeoJsonExporter),
        Box::new(KmlExporter),
        Box::new(CSVExporter),
    ];

    for exporter in exporters {
        exporter.export(&parser, &records, &frames, &args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_dotenv_api_key() {
        assert_eq!(
            parse_dotenv_api_key("DJI_API_KEY=test-key\n"),
            Some("test-key".to_owned())
        );
    }

    #[test]
    fn parses_exported_and_quoted_dotenv_api_key() {
        assert_eq!(
            parse_dotenv_api_key("export DJI_API_KEY=\"test-key\"\n"),
            Some("test-key".to_owned())
        );
    }

    #[test]
    fn ignores_blank_dotenv_api_key() {
        assert_eq!(parse_dotenv_api_key("DJI_API_KEY=\n"), None);
    }
}
