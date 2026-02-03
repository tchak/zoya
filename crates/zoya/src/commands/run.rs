use std::path::Path;

use zoya_check::check;
use crate::eval::{self, EvalError, VirtualModules};
use zoya_codegen::codegen;
use zoya_ir::CheckedItem;

/// Run a Zoya source file and print the result
pub fn execute(path: &Path) -> Result<(), EvalError> {
    // Load and parse modules
    let tree = zoya_loader::load_modules(path)
        .map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;

    // Type check entire module tree
    let checked_tree =
        check(&tree).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

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
    let js_code = codegen(&checked_tree);

    // Create virtual modules and register the generated code
    let virtual_modules = VirtualModules::new();
    virtual_modules.register("root", js_code);

    // Create runtime with module loader
    let (_runtime, context) = eval::create_module_runtime(virtual_modules)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Evaluate the module and call main
    let value = context.with(|ctx| {
        eval::eval_module(&ctx, "root", "$root$main", main_func.return_type.clone())
    })?;
    println!("{}", value);
    Ok(())
}
