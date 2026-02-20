use std::path::Path;

use anyhow::Result;
use console::{Term, style};
use zoya_check::check;
use zoya_ir::pretty_type;
use zoya_loader::Mode;

/// List all `#[task]` functions in a Zoya package or file
pub fn execute(path: &Path, mode: Mode) -> Result<()> {
    let term = Term::stderr();

    // Load and parse package
    let pkg = zoya_loader::load_package(path, mode)?;

    // Type check entire package with std
    let std = zoya_std::std();
    let checked = check(&pkg, &[std])?;

    let tasks = checked.tasks();

    if tasks.is_empty() {
        term.write_line(&format!("{}", style("no tasks found").yellow()))?;
        return Ok(());
    }

    for task_path in &tasks {
        let display = format_task_path(task_path.segments());
        let sig = checked
            .items
            .get(task_path)
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
fn format_task_signature(func: &zoya_ir::TypedFunction) -> String {
    let params: Vec<String> = func.params.iter().map(|(_, ty)| pretty_type(ty)).collect();
    let ret = pretty_type(&func.return_type);
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
