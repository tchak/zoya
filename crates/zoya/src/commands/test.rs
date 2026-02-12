use std::path::Path;

use console::{Term, style};
use zoya_run::{EvalError, Runner};

/// Run all `#[test]` functions in a Zoya package or file
pub fn execute(path: &Path) -> Result<(), EvalError> {
    let term = Term::stderr();
    let runner = Runner::new().test(path)?;

    if runner.tests.is_empty() {
        term.write_line(&format!("{}", style("no tests found").yellow()))
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        return Ok(());
    }

    let is_term = term.is_term();
    let test_count = runner.tests.len();

    // Show all tests as pending upfront (only when interactive)
    if is_term {
        for p in &runner.tests {
            term.write_line(&format!(
                " {}  {}",
                style("····").dim(),
                style(format_test_path(p.segments())).dim()
            ))
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        }
        term.move_cursor_up(test_count)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    }

    // Execute, overwriting each line as results arrive
    let report = runner.execute(|result| {
        let display = format_test_path(result.path.segments());
        if is_term {
            let _ = term.clear_line();
        }
        match &result.outcome {
            Ok(()) => {
                let _ = term.write_line(&format!(
                    " {}  {}",
                    style("PASS").green().bold(),
                    style(&display).bold()
                ));
            }
            Err(_) => {
                let _ = term.write_line(&format!(
                    " {}  {}",
                    style("FAIL").red().bold(),
                    style(&display).bold()
                ));
            }
        }
    })?;

    // Failure details (after all tests)
    let failures: Vec<_> = report
        .results
        .iter()
        .filter_map(|r| r.outcome.as_ref().err().map(|msg| (r, msg)))
        .collect();
    if !failures.is_empty() {
        term.write_line("")
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        term.write_line(&format!("{}", style("failures:").red().bold()))
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        for (r, msg) in &failures {
            term.write_line(&format!(
                "  {}: {}",
                style(format_test_path(r.path.segments())).bold(),
                style(msg).red().dim()
            ))
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        }
    }

    // Summary
    term.write_line("")
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    if report.is_success() {
        term.write_line(&format!(
            "  {}",
            style(format!("{} passed", report.passed())).green().bold()
        ))
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        Ok(())
    } else {
        term.write_line(&format!(
            "  {}, {}",
            style(format!("{} passed", report.passed())).green(),
            style(format!("{} failed", report.failed())).red().bold()
        ))
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        Err(EvalError::RuntimeError(format!(
            "{} test(s) failed",
            report.failed()
        )))
    }
}

/// Format a test path for display, stripping the "root" prefix.
fn format_test_path(segments: &[String]) -> String {
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
    fn test_execute_passing_tests() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[test]
            fn test_add() -> () { () }

            #[test]
            fn test_sub() -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_failing_test() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[test]
            fn test_ok() -> () { () }

            #[test]
            fn test_bad() -> () { panic("oops") }
            "#,
        )
        .unwrap();

        let result = execute(&file);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_no_tests() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_nonexistent_file() {
        let result = execute(Path::new("nonexistent.zy"));
        assert!(result.is_err());
    }

    #[test]
    fn test_format_test_path_strips_root() {
        let segments = vec![
            "root".to_string(),
            "my_module".to_string(),
            "test_foo".to_string(),
        ];
        assert_eq!(format_test_path(&segments), "my_module::test_foo");
    }

    #[test]
    fn test_format_test_path_simple() {
        let segments = vec!["root".to_string(), "test_bar".to_string()];
        assert_eq!(format_test_path(&segments), "test_bar");
    }
}
