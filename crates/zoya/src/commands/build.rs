use std::path::Path;

use crate::check::{check_items, TypeEnv, UnifyCtx};
use zoya_codegen::{codegen_items, prelude};

/// Compile a file to JavaScript without executing
pub fn execute(path: &Path, output: Option<&Path>) -> Result<(), String> {
    // Load and parse modules
    let tree = zoya_loader::load_modules(path).map_err(|e| format!("error: {}", e))?;
    let items = tree.root().expect("root module must exist").items.clone();

    // Type check
    let mut env = TypeEnv::with_builtins();
    let mut ctx = UnifyCtx::new();
    let checked_items = check_items(&items, &mut env, &mut ctx).map_err(|e| format!("error: {}", e))?;

    // Generate JS code
    let mut js_code = String::new();
    js_code.push_str(prelude());
    js_code.push('\n');
    js_code.push_str(&codegen_items(&checked_items));

    // Write output
    match output {
        Some(out_path) => {
            std::fs::write(out_path, &js_code)
                .map_err(|e| format!("error: failed to write file '{}': {}", out_path.display(), e))?;
        }
        None => {
            print!("{}", js_code);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_to_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main() -> Int { 42 }").unwrap();

        let result = execute(&file, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.zoya");
        let output = dir.path().join("test.js");
        std::fs::write(&input, "fn main() -> Int { 42 }").unwrap();

        let result = execute(&input, Some(&output));
        assert!(result.is_ok());
        assert!(output.exists());
        let js = std::fs::read_to_string(&output).unwrap();
        assert!(js.contains("function $main()"));
    }

    #[test]
    fn test_execute_type_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "fn main() -> Int { true }").unwrap();

        let result = execute(&file, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_file_not_found() {
        let result = execute(Path::new("nonexistent.zoya"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to read"));
    }
}
