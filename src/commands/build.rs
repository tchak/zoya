use std::path::Path;

use crate::check::check_file;
use crate::codegen::{codegen_function, prelude};
use crate::ir::{CheckedItem, TypedFunction};
use crate::lexer;
use crate::parser;

/// Compile a file to JavaScript without executing
pub fn execute(path: &Path, output: Option<&Path>) -> Result<(), String> {
    // Read file
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("error: failed to read file '{}': {}", path.display(), e))?;

    // Lex
    let tokens = lexer::lex(&source).map_err(|e| format!("error: {}", e.message))?;

    // Parse
    let items = parser::parse_file(tokens).map_err(|e| format!("error: {}", e.message))?;

    // Type check
    let checked_items = check_file(&items).map_err(|e| format!("error: {}", e))?;

    // Extract functions from checked items
    let typed_functions: Vec<&TypedFunction> = checked_items
        .iter()
        .filter_map(|item| match item {
            CheckedItem::Function(f) => Some(f.as_ref()),
            CheckedItem::Struct(_) => None,
            CheckedItem::Enum(_) => None,
        })
        .collect();

    // Generate JS code
    let mut js_code = String::new();
    js_code.push_str(prelude());
    js_code.push('\n');
    for typed_func in &typed_functions {
        js_code.push_str(&codegen_function(typed_func));
        js_code.push('\n');
    }

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
        assert!(js.contains("function main()"));
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
        assert!(result.unwrap_err().contains("failed to read file"));
    }
}
