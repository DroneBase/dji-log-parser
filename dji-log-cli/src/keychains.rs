use dji_log_parser::keychain::KeychainFeaturePoint;
use dji_log_parser::layout::auxiliary::Department;
use dji_log_parser::{DJILog, Error, Result};

pub(crate) fn fetch_keychains(
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
