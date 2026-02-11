use std::collections::HashMap;
use std::path::Path;

use zoya_check::check;
use zoya_codegen::codegen;
use zoya_ir::CheckedPackage;
use zoya_loader::{MemorySource, load_memory_package, load_package};
use zoya_package::QualifiedPath;

use zoya_ir::Definition;

use crate::eval::{self, EvalError, TypeLookup, Value, VirtualModules};

/// Run an already-checked package by executing its main function
///
/// If `module` is `None`, the root module is used.
/// If `module` is `Some("repl")`, the repl submodule's main is used.
pub fn run(
    package: CheckedPackage,
    deps: &[&CheckedPackage],
    module: Option<&str>,
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

    let return_type = main_def.return_type.clone();

    // Build type lookup for resolving recursive type stubs
    let type_lookup = build_type_lookup(&package, deps);

    // Build module map with dependency modules first
    let mut modules = HashMap::new();
    for dep in deps {
        let dep_output = codegen(dep);
        modules.insert(dep.name.clone(), dep_output.code);
    }

    // Generate JS module code (ESM with exports)
    let output = codegen(&package);

    // Register the generated code
    let module_name = format!("{}_{}", package.name, output.hash);
    modules.insert(module_name.clone(), output.code);
    let virtual_modules = VirtualModules::new(modules);

    // Build the entry function name using the package name
    let entry_func = match module {
        Some(m) => format!("${}${}$main", package.name, m),
        None => format!("${}$main", package.name),
    };

    // Create runtime with module loader
    let (_runtime, context) = eval::create_module_runtime(virtual_modules)
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Evaluate the module and call main
    context
        .with(|ctx| eval::eval_module(&ctx, &module_name, &entry_func, return_type, &type_lookup))
}

/// Load, check, and run source code from a string
pub fn run_source(source: &str) -> Result<Value, EvalError> {
    let std = zoya_std::std();
    let mem_source = MemorySource::new().with_module("root", source);
    let package =
        load_memory_package(&mem_source).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    let checked = check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    run(checked, &[std], None)
}

/// Load, check, and run source code from a file
pub fn run_file(path: &Path) -> Result<Value, EvalError> {
    let std = zoya_std::std();
    let package =
        load_package(path).map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;
    let checked = check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    run(checked, &[std], None)
}

/// Build a TypeLookup from a package and its dependencies for resolving
/// recursive type stubs during JS→Zoya value deserialization.
fn build_type_lookup(package: &CheckedPackage, deps: &[&CheckedPackage]) -> TypeLookup {
    let mut enums = HashMap::new();
    let mut structs = HashMap::new();

    let all_defs = deps
        .iter()
        .flat_map(|d| d.definitions.values())
        .chain(package.definitions.values());

    for def in all_defs {
        match def {
            Definition::Enum(enum_type) if !enum_type.variants.is_empty() => {
                enums.insert(
                    enum_type.name.clone(),
                    (enum_type.type_var_ids.clone(), enum_type.variants.clone()),
                );
            }
            Definition::Struct(struct_type) if !struct_type.fields.is_empty() => {
                structs.insert(
                    struct_type.name.clone(),
                    (struct_type.type_var_ids.clone(), struct_type.fields.clone()),
                );
            }
            _ => {}
        }
    }

    TypeLookup { enums, structs }
}
