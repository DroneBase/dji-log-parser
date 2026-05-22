use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DJI_API_KEY_ENV: &str = "DJI_API_KEY";

pub(crate) fn resolve_api_key(cli_api_key: Option<&str>) -> Option<String> {
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
