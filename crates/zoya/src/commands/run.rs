use std::path::Path;

use crate::eval::{self, EvalError, VirtualModules};
use zoya_check::check;
use zoya_codegen::codegen;
use zoya_ir::CheckedItem;

/// Run a Zoya source file and print the result
pub fn execute(path: &Path) -> Result<(), EvalError> {
    // Load and parse modules
    let tree = zoya_loader::load_modules(path)
        .map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;

    // Type check entire module tree
    let checked_tree = check(&tree).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Find main function in root module
    let root_module = checked_tree
        .root()
        .ok_or_else(|| EvalError::RuntimeError("root module not found".to_string()))?;

    let main_func = root_module
        .items
        .iter()
        .find_map(|item| match item {
            CheckedItem::Function(f) if f.name == "main" => Some(f.as_ref()),
            _ => None,
        })
        .ok_or_else(|| EvalError::RuntimeError("no main() function found".to_string()))?;

    if !main_func.params.is_empty() {
        return Err(EvalError::RuntimeError(
            "main() must not take any parameters".to_string(),
        ));
    }

    // Generate JS module code (ESM with exports)
    let output = codegen(&checked_tree);

    // Create virtual modules and register the generated code
    let virtual_modules = VirtualModules::new();
    let module_name = format!("root_{}", output.hash);
    virtual_modules.register(&module_name, output.code);

    // Create runtime with module loader
    let (_runtime, context) = eval::create_module_runtime(virtual_modules)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Evaluate the module and call main
    let value = context.with(|ctx| {
        eval::eval_module(
            &ctx,
            &module_name,
            "$root$main",
            main_func.return_type.clone(),
        )
    })?;
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
