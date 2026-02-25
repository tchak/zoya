use std::path::Path;

use anyhow::Result;
use console::{Term, style};
use zoya_build::{Mode, build_from_path};

/// List all `#[job]` functions in a Zoya package or file
pub fn execute(path: &Path, mode: Mode) -> Result<()> {
    let term = Term::stderr();

    let output = build_from_path(path, mode)?;

    if output.jobs.is_empty() {
        term.write_line(&format!("{}", style("no jobs found").yellow()))?;
        return Ok(());
    }

    for (job_path, _) in &output.jobs {
        let display = format_job_path(job_path.segments());
        let sig = output
            .definitions
            .get_function(job_path)
            .map(|f| f.pretty())
            .unwrap_or_default();
        term.write_line(&format!(
            "  {}  {}",
            style(&display).bold(),
            style(&sig).dim()
        ))?;
    }

    Ok(())
}

/// Format a job path for display, stripping the "root" prefix.
fn format_job_path(segments: &[String]) -> String {
    let display_segments: Vec<&str> = segments
        .iter()
        .skip_while(|s| s.as_str() == "root")
        .map(|s| s.as_str())
        .collect();
    if display_segments.is_empty() {
        segments.join("::")
    } else {
        display_segments.join("::")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_with_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn deploy() -> () { () }

            #[job]
            pub fn migrate(n: Int) -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_no_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&file, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_nonexistent_file() {
        let result = execute(Path::new("nonexistent.zy"), Mode::Dev);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_job_path_strips_root() {
        let segments = vec![
            "root".to_string(),
            "utils".to_string(),
            "deploy".to_string(),
        ];
        assert_eq!(format_job_path(&segments), "utils::deploy");
    }

    #[test]
    fn test_format_job_path_simple() {
        let segments = vec!["root".to_string(), "my_job".to_string()];
        assert_eq!(format_job_path(&segments), "my_job");
    }
}
