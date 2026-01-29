use std::path::Path;

use crate::check::{check_items, TypeEnv, UnifyCtx};

/// Type-check a file without executing it
pub fn execute(path: &Path) -> Result<(), String> {
    // Load and parse modules
    let tree = zoya_loader::load_modules(path).map_err(|e| format!("error: {}", e))?;
    let items = tree.root().expect("root module must exist").items.clone();

    // Type check
    let mut env = TypeEnv::with_builtins();
    let mut ctx = UnifyCtx::new();
    check_items(&items, &mut env, &mut ctx).map_err(|e| format!("error: {}", e))?;

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
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main() -> Int { 42 }").unwrap();

        let result = execute(&file);
        assert!(result.is_ok());
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
    fn test_execute_file_not_found() {
        let result = execute(Path::new("nonexistent.zoya"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to read"));
    }
}
