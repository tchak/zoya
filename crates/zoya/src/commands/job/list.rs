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
        let display = job_path.without_root().to_string();
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
}
