use std::path::Path;

use zoya_check::check;
use zoya_loader::Mode;

/// Type-check a file without executing it
pub fn execute(path: &Path, mode: Mode) -> Result<(), String> {
    // Load and parse package
    let pkg = zoya_loader::load_package(path, mode).map_err(|e| format!("error: {}", e))?;

    // Type check entire package with std
    let std = zoya_std::std();
    check(&pkg, &[std]).map_err(|e| format!("error: {}", e))?;

    // Success
    eprintln!("✓ Type checking passed: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_success() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&file, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_type_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { true }").unwrap();

        let result = execute(&file, Mode::Dev);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_file_not_found() {
        let result = execute(Path::new("nonexistent.zy"), Mode::Dev);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to read"));
    }

    #[test]
    fn test_multi_module_check() {
        let dir = tempfile::tempdir().unwrap();

        // Create main module with mod declaration
        let main_file = dir.path().join("main.zy");
        std::fs::write(
            &main_file,
            r#"
            mod utils

            pub fn main() -> Int { 42 }
            "#,
        )
        .unwrap();

        // Create child module
        let utils_file = dir.path().join("utils.zy");
        std::fs::write(
            &utils_file,
            r#"
            fn helper() -> Int { 10 }
            "#,
        )
        .unwrap();

        let result = execute(&main_file, Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multi_module_nested() {
        let dir = tempfile::tempdir().unwrap();

        // Create main module with nested mod declaration
        let main_file = dir.path().join("main.zy");
        std::fs::write(
            &main_file,
            r#"
            mod utils

            pub fn main() -> Int { 42 }
            "#,
        )
        .unwrap();

        // Create utils directory
        std::fs::create_dir(dir.path().join("utils")).unwrap();

        // Create utils module with its own child
        let utils_file = dir.path().join("utils.zy");
        std::fs::write(
            &utils_file,
            r#"
            mod helpers

            fn utility() -> Int { 20 }
            "#,
        )
        .unwrap();

        // Create helpers module
        let helpers_file = dir.path().join("utils").join("helpers.zy");
        std::fs::write(
            &helpers_file,
            r#"
            fn deep_helper() -> Int { 30 }
            "#,
        )
        .unwrap();

        let result = execute(&main_file, Mode::Dev);
        assert!(result.is_ok());
    }
}
