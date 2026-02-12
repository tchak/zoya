use std::collections::HashMap;
use std::path::Path;

use zoya_check::check;
use zoya_codegen::{codegen, esm_module_name};
use zoya_ir::{CheckedPackage, Definition};
use zoya_loader::{MemorySource, load_memory_package, load_package};
use zoya_package::QualifiedPath;

use crate::eval::{self, EvalError, TypeLookup, Value, VirtualModules};

/// Entry point for building a run configuration.
///
/// Use `Runner::new()` then choose an input source:
/// - `.package(pkg, deps)` → `PackageRunner`
/// - `.path(p)` → `PathRunner`
/// - `.source(s)` → `SourceRunner`
#[derive(Default)]
pub struct Runner;

impl Runner {
    /// Create a new runner.
    pub fn new() -> Self {
        Runner
    }

    /// Run an already-checked package with its dependencies.
    pub fn package<'a>(
        self,
        package: &'a CheckedPackage,
        deps: impl IntoIterator<Item = &'a CheckedPackage>,
    ) -> PackageRunner<'a> {
        PackageRunner {
            package,
            deps: deps.into_iter().collect(),
            module: None,
        }
    }

    /// Load, check, and run a `.zy` file at the given path.
    pub fn path(self, path: &Path) -> PathRunner {
        PathRunner {
            path: path.to_path_buf(),
            mode: zoya_loader::Mode::Dev,
        }
    }

    /// Load, check, and run source code from a string.
    pub fn source(self, source: &str) -> SourceRunner {
        SourceRunner {
            source: source.to_string(),
            mode: zoya_loader::Mode::Dev,
        }
    }
}

/// Runner configured with a pre-checked package.
pub struct PackageRunner<'a> {
    package: &'a CheckedPackage,
    deps: Vec<&'a CheckedPackage>,
    module: Option<String>,
}

impl<'a> PackageRunner<'a> {
    /// Select a submodule whose `main()` to run (e.g., `"repl"`).
    pub fn module(mut self, module: impl Into<String>) -> Self {
        self.module = Some(module.into());
        self
    }

    /// Execute the package and return the result.
    pub fn run(self) -> Result<Value, EvalError> {
        run_checked(self.package, &self.deps, self.module.as_deref())
    }
}

/// Runner configured to load and run a file.
pub struct PathRunner {
    path: std::path::PathBuf,
    mode: zoya_loader::Mode,
}

impl PathRunner {
    /// Set the compilation mode (default: `Mode::Dev`).
    pub fn mode(mut self, mode: zoya_loader::Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Load, check, and execute the file.
    pub fn run(self) -> Result<Value, EvalError> {
        let std = zoya_std::std();
        let package = load_package(&self.path, self.mode)
            .map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;
        let checked =
            check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        run_checked(&checked, &[std], None)
    }
}

/// Runner configured to compile and run a source string.
pub struct SourceRunner {
    source: String,
    mode: zoya_loader::Mode,
}

impl SourceRunner {
    /// Set the compilation mode (default: `Mode::Dev`).
    pub fn mode(mut self, mode: zoya_loader::Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Compile and execute the source string.
    pub fn run(self) -> Result<Value, EvalError> {
        let std = zoya_std::std();
        let mem_source = MemorySource::new().with_module("root", &self.source);
        let package = load_memory_package(&mem_source, self.mode)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        let checked =
            check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        run_checked(&checked, &[std], None)
    }
}

/// Load, check, and run source code from a string (convenience function).
pub fn run_source(source: &str) -> Result<Value, EvalError> {
    Runner::new().source(source).run()
}

/// Load, check, and run a `.zy` file (convenience function).
pub fn run_path(path: &Path) -> Result<Value, EvalError> {
    Runner::new().path(path).run()
}

/// Internal: execute an already-checked package.
fn run_checked(
    package: &CheckedPackage,
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
    let type_lookup = build_type_lookup(package, deps);

    // Generate all modules (deps + main package)
    let outputs = codegen(package, deps);
    let modules_ref: HashMap<&str, &zoya_codegen::CodegenOutput> =
        outputs.iter().map(|(k, v)| (k.as_str(), v)).collect();

    // Build virtual modules map with ESM module names
    let mut modules = HashMap::new();
    for (name, output) in &outputs {
        let esm_name = esm_module_name(name, &modules_ref);
        modules.insert(esm_name, output.code.clone());
    }
    let module_name = esm_module_name(&package.name, &modules_ref);
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
