use std::path::Path;

use anyhow::{Result, anyhow, bail};
use zoya_check::check;
use zoya_ir::{DefinitionLookup, TypedPattern};
use zoya_loader::Mode;
use zoya_package::QualifiedPath;
use zoya_run::{Runner, Value};

/// Run a `#[task]` function with CLI arguments parsed by type
pub fn execute(
    path: &Path,
    task_name: &str,
    args: &[String],
    json: bool,
    mode: Mode,
) -> Result<()> {
    // Load and type-check
    let pkg = zoya_loader::load_package(path, mode)?;
    let std = zoya_std::std();
    let checked = check(&pkg, &[std])?;

    // Build qualified path from task name (e.g. "deploy" or "utils::migrate")
    let mut task_path = QualifiedPath::root();
    for segment in task_name.split("::") {
        task_path = task_path.child(segment);
    }

    // Verify this is a known task
    let tasks = checked.tasks();
    if !tasks.contains(&task_path) {
        let available: Vec<String> = tasks
            .iter()
            .map(|p| {
                p.segments()
                    .iter()
                    .skip_while(|s| s.as_str() == "root")
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect();
        let hint = if available.is_empty() {
            "no tasks found in this package".to_string()
        } else {
            format!("available tasks: {}", available.join(", "))
        };
        bail!("task '{task_name}' not found ({hint})");
    }

    // Get the function's typed signature
    let func = checked
        .items
        .get(&task_path)
        .ok_or_else(|| anyhow!("task '{task_name}' not found in items"))?;

    // Validate argument count
    if args.len() != func.params.len() {
        bail!(
            "task '{task_name}' expects {} argument(s), got {}",
            func.params.len(),
            args.len()
        );
    }

    // Build type lookup for struct/enum resolution
    let type_lookup = DefinitionLookup::from_packages(&checked, &[std]);

    // Parse each argument guided by the parameter type
    let mut parsed_args = Vec::with_capacity(args.len());
    for (i, (arg_str, (pattern, param_type))) in args.iter().zip(func.params.iter()).enumerate() {
        let param_name = match pattern {
            TypedPattern::Var { name, .. } => name.as_str(),
            _ => "?",
        };
        let value = Value::parse(arg_str, param_type, &type_lookup)
            .map_err(|e| anyhow!("argument {} ({param_name}): {e}", i + 1))?;
        parsed_args.push(value);
    }

    // Run the task function
    let result = Runner::new()
        .package(&checked, [std])
        .entry(task_path, parsed_args)
        .run()?;

    // Print result
    if json {
        println!("{}", result.to_json_pretty());
    } else {
        println!("{}", result);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_task_simple() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
        )
        .unwrap();

        let result = execute(&file, "add", &["1".into(), "2".into()], false, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_task_string_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn greet(name: String) -> String { name }
            "#,
        )
        .unwrap();

        let result = execute(&file, "greet", &["world".into()], false, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_task_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn deploy() -> String { "done" }
            "#,
        )
        .unwrap();

        let result = execute(&file, "missing", &[], false, Mode::Dev);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_run_task_wrong_arg_count() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
        )
        .unwrap();

        let result = execute(&file, "add", &["1".into()], false, Mode::Dev);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("expects 2 argument(s), got 1")
        );
    }

    #[test]
    fn test_run_task_invalid_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn double(n: Int) -> Int { n * 2 }
            "#,
        )
        .unwrap();

        let result = execute(&file, "double", &["abc".into()], false, Mode::Dev);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("argument 1"));
    }

    #[test]
    fn test_run_task_no_args() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn deploy() -> String { "deployed" }
            "#,
        )
        .unwrap();

        let result = execute(&file, "deploy", &[], false, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_task_bool_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[task]
            pub fn identity(flag: Bool) -> Bool { flag }
            "#,
        )
        .unwrap();

        let result = execute(&file, "identity", &["true".into()], false, Mode::Dev);
        assert!(result.is_ok());
    }
}
