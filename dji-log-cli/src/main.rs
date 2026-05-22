use clap::Parser;
use dji_log_parser::frame::Frame;
use dji_log_parser::record::Record;
use dji_log_parser::DJILog;
use exporters::{CSVExporter, GeoJsonExporter, ImageExporter, JsonExporter, KmlExporter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

mod api_key;
mod exporters;
mod keychains;
mod output;
mod utils;

use api_key::resolve_api_key;
use keychains::fetch_keychains;
use output::{build_output_plan, OutputPlan};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// Input log file(s)
    #[arg(value_name = "FILE", required = true, num_args = 1..)]
    filepaths: Vec<String>,

    /// Write JSON output to FILE(s) instead of using input-derived names
    #[arg(short, long, value_name = "FILE", num_args = 1..)]
    output: Vec<String>,

    /// Overwrite existing JSON output files
    #[arg(long)]
    overwrite: bool,

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

pub(crate) struct ExportOptions {
    output: Option<String>,
    images: Option<String>,
    thumbnails: Option<String>,
    geojson: Option<String>,
    kml: Option<String>,
    csv: Option<String>,
    raw: bool,
}

pub(crate) trait Exporter {
    fn export(
        &self,
        parser: &DJILog,
        records: &Vec<Record>,
        frames: &Vec<Frame>,
        options: &ExportOptions,
    );
}

struct ParseFailure {
    input_path: PathBuf,
    error: String,
}

fn main() {
    let args = Cli::parse();
    let output_plan = build_output_plan(&args.filepaths, &args.output, args.overwrite)
        .unwrap_or_else(|error| exit_with_error(&error));

    print_existing_outputs(&output_plan, args.overwrite);

    let mut decoded_count = 0;
    let mut failures = Vec::new();
    for entry in output_plan.entries.iter().filter(|entry| !entry.skip) {
        eprintln!(
            "Decoding file {} as {}",
            entry.input_path.display(),
            entry.output_path.display()
        );

        match parse_file(&args, &entry.input_path, &entry.output_path) {
            Ok(()) => decoded_count += 1,
            Err(error) => {
                eprintln!("Skipping file {}: {}", entry.input_path.display(), error);
                failures.push(ParseFailure {
                    input_path: entry.input_path.clone(),
                    error,
                });
            }
        }
    }

    if args.overwrite {
        print_overwritten_outputs(&output_plan);
    }

    print_parse_failures(&failures);
    eprintln!("Decoded {} file(s)", decoded_count);
}

fn parse_file(args: &Cli, input_path: &Path, output_path: &Path) -> Result<(), String> {
    let bytes = fs::read(input_path).map_err(|error| format!("unable to read file: {error}"))?;
    let parser =
        DJILog::from_bytes(bytes).map_err(|_| "file is not a parsable DJI log".to_owned())?;

    let keychains = if parser.version >= 13 {
        let api_key = resolve_api_key(args.api_key.as_deref())
            .ok_or_else(|| "API key is required for version 13 and above".to_owned())?;

        let keychains = fetch_keychains(
            &parser,
            &api_key,
            args.api_custom_department,
            args.api_custom_version,
        )
        .map_err(|error| format!("unable to fetch keychain: {error}"))?;

        Some(keychains)
    } else {
        None
    };

    let records = parser
        .records(keychains.clone())
        .map_err(|_| "unable to parse records".to_owned())?;

    let frames = parser
        .frames(keychains)
        .map_err(|_| "unable to parse frames".to_owned())?;
    let output_path = output_path.to_string_lossy().to_string();
    let export_options = ExportOptions {
        output: Some(output_path),
        images: args.images.clone(),
        thumbnails: args.thumbnails.clone(),
        geojson: args.geojson.clone(),
        kml: args.kml.clone(),
        csv: args.csv.clone(),
        raw: args.raw,
    };

    let exporters: Vec<Box<dyn Exporter>> = vec![
        Box::new(JsonExporter),
        Box::new(ImageExporter),
        Box::new(GeoJsonExporter),
        Box::new(KmlExporter),
        Box::new(CSVExporter),
    ];

    for exporter in exporters {
        exporter.export(&parser, &records, &frames, &export_options);
    }

    Ok(())
}

fn print_existing_outputs(output_plan: &OutputPlan, overwrite: bool) {
    if output_plan.existing_outputs.is_empty() {
        return;
    }

    if overwrite {
        return;
    }

    eprintln!("Output file(s) already exist; skipping them. Use --overwrite to replace:");
    for output_path in &output_plan.existing_outputs {
        eprintln!("  {}", output_path.display());
    }
}

fn print_overwritten_outputs(output_plan: &OutputPlan) {
    if output_plan.existing_outputs.is_empty() {
        return;
    }

    eprintln!("Overwrote existing output file(s):");
    for output_path in &output_plan.existing_outputs {
        eprintln!("  {}", output_path.display());
    }
}

fn print_parse_failures(failures: &[ParseFailure]) {
    if failures.is_empty() {
        return;
    }

    eprintln!("File(s) skipped because they could not be decoded:");
    for failure in failures {
        eprintln!("  {}: {}", failure.input_path.display(), failure.error);
    }
}

fn exit_with_error(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(2);
}
