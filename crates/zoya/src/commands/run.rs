use std::path::Path;

use crate::check::check_module_tree;
use crate::eval::{self, EvalError};
use zoya_codegen::{codegen_module_tree, prelude};
use zoya_ir::CheckedItem;

/// Run a Zoya source file and print the result
pub fn execute(path: &Path) -> Result<(), EvalError> {
    // Load and parse modules
    let tree = zoya_loader::load_modules(path)
        .map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;

    // Type check entire module tree
    let checked_tree =
        check_module_tree(&tree).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

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

    // Generate JS code
    let mut js_code = String::new();
    js_code.push_str(prelude());
    js_code.push('\n');
    js_code.push_str(&codegen_module_tree(&checked_tree));
    js_code.push_str("$main()");

    // Execute
    let (_runtime, context) =
        eval::create_context().map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    let value = context.with(|ctx| eval::eval(&ctx, js_code, main_func.return_type.clone()))?;
    println!("{}", value);
    Ok(())
}
