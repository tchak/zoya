use std::path::Path;

use anyhow::Result;
use console::{Term, style};
use zoya_build::{Mode, build_from_path};
use zoya_ir::FunctionType;

/// List all `#[task]` functions in a Zoya package or file
pub fn execute(path: &Path, mode: Mode) -> Result<()> {
    let term = Term::stderr();

    let output = build_from_path(path, mode)?;

    if output.tasks.is_empty() {
        term.write_line(&format!("{}", style("no tasks found").yellow()))?;
        return Ok(());
    }

    for task_path in &output.tasks {
        let display = format_task_path(task_path.segments());
        let sig = output
            .definitions
            .get_function(task_path)
            .map(format_task_signature)
            .unwrap_or_default();
        term.write_line(&format!(
            "  {}  {}",
            style(&display).bold(),
            style(&sig).dim()
        ))?;
    }

    Ok(())
}

/// Format a task path for display, stripping the "root" prefix.
fn format_task_path(segments: &[String]) -> String {
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

/// Format a task function's type signature for display.
fn format_task_signature(func: &FunctionType) -> String {
    let params: Vec<String> = func.params.iter().map(|ty| ty.pretty()).collect();
    let ret = func.return_type.pretty();
    format!("({}) -> {}", params.join(", "), ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_with_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn deploy() -> String { "done" }

            #[task]
            pub fn migrate(n: Int) -> Int { n }
            "#,
        )
        .unwrap();

        let result = execute(&file, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_no_tasks() {
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
    fn test_format_task_path_strips_root() {
        let segments = vec![
            "root".to_string(),
            "utils".to_string(),
            "deploy".to_string(),
        ];
        assert_eq!(format_task_path(&segments), "utils::deploy");
    }

    #[test]
    fn test_format_task_path_simple() {
        let segments = vec!["root".to_string(), "my_task".to_string()];
        assert_eq!(format_task_path(&segments), "my_task");
    }
}
