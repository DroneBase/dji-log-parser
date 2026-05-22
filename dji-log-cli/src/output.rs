use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct OutputPlan {
    pub entries: Vec<OutputPlanEntry>,
    pub existing_outputs: Vec<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct OutputPlanEntry {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub skip: bool,
}

pub(crate) fn build_output_plan(
    input_paths: &[String],
    output_paths: &[String],
    overwrite: bool,
) -> Result<OutputPlan, String> {
    if !output_paths.is_empty() && output_paths.len() != input_paths.len() {
        return Err(format!(
            "--output expects {} file(s), but {} were provided",
            input_paths.len(),
            output_paths.len()
        ));
    }

    let mut entries = Vec::new();
    let mut existing_outputs = Vec::new();
    let mut seen_existing_outputs = HashSet::new();

    for (index, input_path) in input_paths.iter().enumerate() {
        let input_path = PathBuf::from(input_path);
        if output_paths.is_empty() && is_json_path(&input_path) {
            continue;
        }

        let output_path = output_paths
            .get(index)
            .map(PathBuf::from)
            .unwrap_or_else(|| default_json_path(&input_path));
        let exists = output_path.exists();

        if exists && seen_existing_outputs.insert(output_path.clone()) {
            existing_outputs.push(output_path.clone());
        }

        entries.push(OutputPlanEntry {
            input_path,
            output_path,
            skip: exists && !overwrite,
        });
    }

    Ok(OutputPlan {
        entries,
        existing_outputs,
    })
}

fn default_json_path(input_path: &Path) -> PathBuf {
    input_path.with_extension("json")
}

fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_input_extension_with_json() {
        assert_eq!(
            default_json_path(Path::new("examples/file1.txt")),
            PathBuf::from("examples/file1.json")
        );
    }

    #[test]
    fn rejects_mismatched_manual_outputs() {
        let error = build_output_plan(
            &["file1.txt".to_owned(), "file2.txt".to_owned()],
            &["file1.json".to_owned()],
            false,
        )
        .unwrap_err();

        assert_eq!(error, "--output expects 2 file(s), but 1 were provided");
    }

    #[test]
    fn accepts_matching_manual_outputs() {
        let plan = build_output_plan(
            &["file1.txt".to_owned(), "file2.txt".to_owned()],
            &["out1.json".to_owned(), "out2.json".to_owned()],
            false,
        )
        .unwrap();

        assert_eq!(plan.entries.len(), 2);
        assert_eq!(plan.entries[0].output_path, PathBuf::from("out1.json"));
        assert_eq!(plan.entries[1].output_path, PathBuf::from("out2.json"));
    }

    #[test]
    fn ignores_json_inputs_in_automatic_output_mode() {
        let plan = build_output_plan(
            &[
                "examples/file1.json".to_owned(),
                "examples/file1.txt".to_owned(),
            ],
            &[],
            false,
        )
        .unwrap();

        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            plan.entries[0].input_path,
            PathBuf::from("examples/file1.txt")
        );
        assert_eq!(
            plan.entries[0].output_path,
            PathBuf::from("examples/file1.json")
        );
    }

    #[test]
    fn keeps_json_inputs_when_outputs_are_explicit() {
        let plan = build_output_plan(
            &["examples/file1.json".to_owned()],
            &["examples/file1.out.json".to_owned()],
            false,
        )
        .unwrap();

        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            plan.entries[0].input_path,
            PathBuf::from("examples/file1.json")
        );
    }
}
