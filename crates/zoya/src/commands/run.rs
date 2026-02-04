use std::path::Path;

use zoya_run::{run_file, EvalError};

/// Run a Zoya source file and print the result
pub fn execute(path: &Path) -> Result<(), EvalError> {
    let value = run_file(path)?;
    println!("{}", value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_success() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main() -> Int { 42 }").unwrap();

        let result = execute(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_file_not_found() {
        let result = execute(Path::new("nonexistent.zoya"));
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_type_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main() -> Int { true }").unwrap();

        let result = execute(&file);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_missing_main() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn helper() -> Int { 1 }").unwrap();

        let result = execute(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("main"));
    }

    #[test]
    fn test_execute_main_with_parameters() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main(x: Int) -> Int { x }").unwrap();

        let result = execute(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("parameter"));
    }

    #[test]
    fn test_execute_returns_bool() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main() -> Bool { true }").unwrap();

        let result = execute(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_returns_string() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, r#"fn main() -> String { "hello" }"#).unwrap();

        let result = execute(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_multi_module() {
        let dir = tempfile::tempdir().unwrap();

        // Create main module with mod declaration
        let main_file = dir.path().join("main.zoya");
        std::fs::write(
            &main_file,
            r#"
            mod utils

            fn main() -> Int { utils::helper() }
            "#,
        )
        .unwrap();

        // Create child module with public function
        let utils_file = dir.path().join("utils.zoya");
        std::fs::write(
            &utils_file,
            r#"
            pub fn helper() -> Int { 42 }
            "#,
        )
        .unwrap();

        let result = execute(&main_file);
        assert!(result.is_ok());
    }
}
