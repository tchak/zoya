use std::path::Path;

use zoya_check::check;
use zoya_codegen::codegen;
use zoya_ir::{CheckedPackage, Type};
use zoya_loader::{load_memory_package, load_package, MemorySource};
use zoya_package::QualifiedPath;

use crate::eval::{self, EvalError, Value, VirtualModules};

/// Run an already-checked package by executing its main function
///
/// If `module` is `None`, the root module is used.
/// If `return_type` is `Some`, it overrides the return type from the checked package.
/// This is useful when the main function has an inferred return type that may contain
/// unresolved type variables.
pub fn run(
    package: CheckedPackage,
    module: Option<QualifiedPath>,
    return_type: Option<Type>,
) -> Result<Value, EvalError> {
    let module_path = module.unwrap_or_else(QualifiedPath::root);

    // Find main in the specified module's definitions (must be pub)
    let main_path = module_path.child("main");
    let main_def = package
        .definitions
        .get(&main_path)
        .and_then(|d| d.as_function())
        .ok_or_else(|| {
            EvalError::RuntimeError(format!("no pub fn main() found in {}", module_path))
        })?;

    if !main_def.params.is_empty() {
        return Err(EvalError::RuntimeError(
            "main() must not take any parameters".to_string(),
        ));
    }

    // Use provided return type or fall back to the one from the checked package
    let return_type = return_type.unwrap_or_else(|| main_def.return_type.clone());

    // Generate JS module code (ESM with exports)
    let output = codegen(&package);

    // Create virtual modules and register the generated code
    let virtual_modules = VirtualModules::new();
    let module_name = format!("root_{}", output.hash);
    virtual_modules.register(&module_name, output.code);

    // Build the entry function path from module path segments
    // e.g., ["root", "utils"] -> "$root$utils$main"
    let entry_func = format!("${}$main", module_path.segments().join("$"));

    // Create runtime with module loader
    let (_runtime, context) = eval::create_module_runtime(virtual_modules)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Evaluate the module and call main
    context.with(|ctx| eval::eval_module(&ctx, &module_name, &entry_func, return_type))
}

/// Load, check, and run source code from a string
pub fn run_source(source: &str) -> Result<Value, EvalError> {
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    let checked = check(&package).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    run(checked, None, None)
}

/// Load, check, and run source code from a file
pub fn run_file(path: &Path) -> Result<Value, EvalError> {
    let package =
        load_package(path).map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;
    let checked = check(&package).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    run(checked, None, None)
}
