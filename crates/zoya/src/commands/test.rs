use std::path::Path;

use console::{Term, style};
use zoya_run::EvalError;
use zoya_test::TestRunner;

/// Run all `#[test]` functions in a Zoya package or file
pub fn execute(path: &Path) -> Result<(), EvalError> {
    let term = Term::stderr();
    let output =
        zoya_build::build_from_path(path, zoya_build::Mode::Test).map_err(|e| match e {
            zoya_build::BuildError::Load(e) => EvalError::LoadError(e.map_path(|p| p.to_string())),
            zoya_build::BuildError::Check(e) => EvalError::TypeError(e),
        })?;
    let runner = TestRunner::new(&output);

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
                style(p.without_root().to_string()).dim()
            ))
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        }
        term.move_cursor_up(test_count)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    }

    // Execute, overwriting each line as results arrive
    let report = runner.execute(|result| {
        let display = result.path.without_root().to_string();
        if is_term {
            let _ = term.clear_line();
        }
        match &result.outcome {
            Ok(()) => {
                let jobs_suffix = if result.jobs.is_empty() {
                    String::new()
                } else {
                    format!(
                        " {}",
                        style(format!("({} jobs enqueued)", result.jobs.len())).dim()
                    )
                };
                let _ = term.write_line(&format!(
                    " {}  {}{}",
                    style("PASS").green().bold(),
                    style(&display).bold(),
                    jobs_suffix
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
                style(r.path.without_root().to_string()).bold(),
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
}
