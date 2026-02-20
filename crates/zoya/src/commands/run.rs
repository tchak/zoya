use std::path::Path;

use anyhow::{Result, anyhow, bail};
use zoya_check::check;
use zoya_ir::{DefinitionLookup, TypedPattern};
use zoya_loader::Mode;
use zoya_package::QualifiedPath;
use zoya_run::{Runner, Value};

/// Run a Zoya package or file and print the result
pub fn execute(
    path: &Path,
    mode: Mode,
    name: Option<&str>,
    args: &[String],
    json: bool,
) -> Result<()> {
    match name {
        None => {
            // Default behavior: run main()
            let value = Runner::new().path(path).mode(mode).run()?;
            if json {
                println!("{}", value.to_json_pretty());
            } else {
                println!("{}", value);
            }
            Ok(())
        }
        Some(fn_name) => {
            // Run a named function with type-guided arg parsing
            let pkg = zoya_loader::load_package(path, mode)?;
            let std = zoya_std::std();
            let checked = check(&pkg, &[std])?;

            // Build qualified path from function name (e.g. "add" or "utils::helper")
            let mut fn_path = QualifiedPath::root();
            for segment in fn_name.split("::") {
                fn_path = fn_path.child(segment);
            }

            // Verify this is a known public function
            let fns = checked.fns();
            if !fns.contains(&fn_path) {
                let available: Vec<String> = fns
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
                    "no public functions found in this package".to_string()
                } else {
                    format!("available functions: {}", available.join(", "))
                };
                bail!("function '{fn_name}' not found ({hint})");
            }

            // Get the function's typed signature
            let func = checked
                .items
                .get(&fn_path)
                .ok_or_else(|| anyhow!("function '{fn_name}' not found in items"))?;

            // Validate argument count
            if args.len() != func.params.len() {
                bail!(
                    "function '{fn_name}' expects {} argument(s), got {}",
                    func.params.len(),
                    args.len()
                );
            }

            // Build type lookup for struct/enum resolution
            let type_lookup = DefinitionLookup::from_packages(&checked, &[std]);

            // Parse each argument guided by the parameter type
            let mut parsed_args = Vec::with_capacity(args.len());
            for (i, (arg_str, (pattern, param_type))) in
                args.iter().zip(func.params.iter()).enumerate()
            {
                let param_name = match pattern {
                    TypedPattern::Var { name, .. } => name.as_str(),
                    _ => "?",
                };
                let value = Value::parse(arg_str, param_type, &type_lookup)
                    .map_err(|e| anyhow!("argument {} ({param_name}): {e}", i + 1))?;
                parsed_args.push(value);
            }

            // Run the function
            let result = Runner::new()
                .package(&checked, [std])
                .entry(fn_path, parsed_args)
                .run()?;

            // Print result
            if json {
                println!("{}", result.to_json_pretty());
            } else {
                println!("{}", result);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_success() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&file, Mode::Dev, None, &[], false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_file_not_found() {
        let result = execute(Path::new("nonexistent.zy"), Mode::Dev, None, &[], false);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_type_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { true }").unwrap();

        let result = execute(&file, Mode::Dev, None, &[], false);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_missing_main() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "fn helper() -> Int { 1 }").unwrap();

        let result = execute(&file, Mode::Dev, None, &[], false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("main"));
    }

    #[test]
    fn test_execute_main_with_parameters() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main(x: Int) -> Int { x }").unwrap();

        let result = execute(&file, Mode::Dev, None, &[], false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("argument(s), got 0"));
    }

    #[test]
    fn test_execute_returns_bool() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Bool { true }").unwrap();

        let result = execute(&file, Mode::Dev, None, &[], false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_returns_string() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, r#"pub fn main() -> String { "hello" }"#).unwrap();

        let result = execute(&file, Mode::Dev, None, &[], false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_multi_module() {
        let dir = tempfile::tempdir().unwrap();

        // Create main module with mod declaration
        let main_file = dir.path().join("main.zy");
        std::fs::write(
            &main_file,
            r#"
            mod utils

            pub fn main() -> Int { utils::helper() }
            "#,
        )
        .unwrap();

        // Create child module with public function
        let utils_file = dir.path().join("utils.zy");
        std::fs::write(
            &utils_file,
            r#"
            pub fn helper() -> Int { 42 }
            "#,
        )
        .unwrap();

        let result = execute(&main_file, Mode::Dev, None, &[], false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_named_fn_no_args() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, r#"pub fn greet() -> String { "hello" }"#).unwrap();

        let result = execute(&file, Mode::Dev, Some("greet"), &[], false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_named_fn_with_args() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn add(x: Int, y: Int) -> Int { x + y }").unwrap();

        let result = execute(
            &file,
            Mode::Dev,
            Some("add"),
            &["1".into(), "2".into()],
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_named_fn_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn greet() -> String { \"hello\" }").unwrap();

        let result = execute(&file, Mode::Dev, Some("missing"), &[], false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
        assert!(err.contains("greet"));
    }

    #[test]
    fn test_execute_named_fn_wrong_arg_count() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn add(x: Int, y: Int) -> Int { x + y }").unwrap();

        let result = execute(&file, Mode::Dev, Some("add"), &["1".into()], false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("expects 2 argument(s), got 1")
        );
    }

    #[test]
    fn test_execute_named_fn_invalid_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn double(n: Int) -> Int { n * 2 }").unwrap();

        let result = execute(&file, Mode::Dev, Some("double"), &["abc".into()], false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("argument 1"));
    }
}
