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
/// If `module` is `Some("repl")`, the repl submodule's main is used.
/// If `return_type` is `Some`, it overrides the return type from the checked package.
/// This is useful when the main function has an inferred return type that may contain
/// unresolved type variables.
pub fn run(
    package: CheckedPackage,
    deps: &[&CheckedPackage],
    module: Option<&str>,
    return_type: Option<Type>,
) -> Result<Value, EvalError> {
    // Build the definition lookup path (always uses "root" prefix)
    let module_path = match module {
        Some(m) => QualifiedPath::root().child(m),
        None => QualifiedPath::root(),
    };
    let main_path = module_path.child("main");

    // Find main in the specified module's definitions (must be pub)
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

    // Create virtual modules and register dependency modules first
    let virtual_modules = VirtualModules::new();
    for dep in deps {
        let dep_output = codegen(dep);
        virtual_modules.register(&dep.name, dep_output.code);
    }

    // Generate JS module code (ESM with exports)
    let output = codegen(&package);

    // Register the generated code
    let module_name = format!("{}_{}", package.name, output.hash);
    virtual_modules.register(&module_name, output.code);

    // Build the entry function name using the package name
    let entry_func = match module {
        Some(m) => format!("${}${}$main", package.name, m),
        None => format!("${}$main", package.name),
    };

    // Create runtime with module loader
    let (_runtime, context) = eval::create_module_runtime(virtual_modules)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Evaluate the module and call main
    context.with(|ctx| eval::eval_module(&ctx, &module_name, &entry_func, return_type))
}

/// Load, check, and run source code from a string
pub fn run_source(source: &str) -> Result<Value, EvalError> {
    let std = zoya_std::std();
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    let checked = check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    run(checked, &[std], None, None)
}

/// Load, check, and run source code from a file
pub fn run_file(path: &Path) -> Result<Value, EvalError> {
    let std = zoya_std::std();
    let package =
        load_package(path).map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;
    let checked = check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    run(checked, &[std], None, None)
}
