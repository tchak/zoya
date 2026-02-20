use zoya_check::check;
use zoya_codegen::{codegen, format_export_path};
use zoya_ir::{CheckedPackage, DefinitionLookup};
use zoya_loader::{MemorySource, load_memory_package};
use zoya_package::QualifiedPath;

use zoya_value::Value;

use crate::eval::{self, EvalError};

/// Which function to invoke inside a checked package.
enum EntryPoint {
    /// Run `main()` in the root module or a named submodule.
    Main(Option<String>),
    /// Run an arbitrary function by its full qualified path, with optional args.
    Entry(QualifiedPath, Vec<Value>),
}

/// Entry point for building a run configuration.
///
/// Use `Runner::new()` then choose an input source:
/// - `.package(pkg, deps)` → `PackageRunner`
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
            entry_point: EntryPoint::Main(None),
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
    entry_point: EntryPoint,
}

impl<'a> PackageRunner<'a> {
    /// Select a submodule whose `main()` to run (e.g., `"repl"`).
    pub fn main_module(mut self, module: impl Into<String>) -> Self {
        self.entry_point = EntryPoint::Main(Some(module.into()));
        self
    }

    /// Select an arbitrary function to run by its full qualified path, with args.
    pub fn entry(mut self, path: QualifiedPath, args: Vec<Value>) -> Self {
        self.entry_point = EntryPoint::Entry(path, args);
        self
    }

    /// Execute the package and return the result.
    pub fn run(self) -> Result<Value, EvalError> {
        run_checked(self.package, &self.deps, &self.entry_point)
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
        let package = load_memory_package(&mem_source, self.mode)?;
        let checked = check(&package, &[std])?;
        run_checked(&checked, &[std], &EntryPoint::Main(None))
    }
}

/// Load, check, and run source code from a string (convenience function).
pub fn run_source(source: &str) -> Result<Value, EvalError> {
    Runner::new().source(source).run()
}

/// Internal: execute an already-checked package.
fn run_checked(
    package: &CheckedPackage,
    deps: &[&CheckedPackage],
    entry_point: &EntryPoint,
) -> Result<Value, EvalError> {
    // Resolve the function path and args from the entry point
    let (function_path, args) = match entry_point {
        EntryPoint::Main(module) => {
            let module_path = match module {
                Some(m) => QualifiedPath::root().child(m),
                None => QualifiedPath::root(),
            };
            (module_path.child("main"), vec![])
        }
        EntryPoint::Entry(path, args) => (path.clone(), args.clone()),
    };

    // Find the function in the package definitions
    let func_def = package
        .definitions
        .get(&function_path)
        .and_then(|d| d.as_function())
        .ok_or_else(|| {
            EvalError::RuntimeError(match entry_point {
                EntryPoint::Main(_) => {
                    let module_path = function_path.parent().unwrap_or_else(QualifiedPath::root);
                    format!("no pub fn main() found in {}", module_path)
                }
                EntryPoint::Entry(path, _) => format!("function {} not found", path),
            })
        })?;

    // Validate argument count
    if func_def.params.len() != args.len() {
        return Err(EvalError::RuntimeError(format!(
            "{}() expects {} argument(s), got {}",
            function_path.last(),
            func_def.params.len(),
            args.len()
        )));
    }

    let return_type = func_def.return_type.clone();

    // Build type lookup for resolving recursive type stubs
    let type_lookup = DefinitionLookup::from_packages(package, deps);

    // Validate each arg's type
    for (i, (arg, param_type)) in args.iter().zip(func_def.params.iter()).enumerate() {
        arg.check_type(param_type, &type_lookup)
            .map_err(|e| EvalError::RuntimeError(format!("argument {} type mismatch: {}", i, e)))?;
    }

    // Generate single concatenated JS
    let output = codegen(package, deps);

    // Build the entry function name using the package name
    let entry_func = format_export_path(&function_path, &package.name);

    // Create runtime (no module system needed)
    let (_runtime, context) = eval::create_runtime()?;

    // Evaluate the script and call the entry function
    context.with(|ctx| {
        eval::eval_script(
            &ctx,
            &output.code,
            &entry_func,
            &args,
            return_type,
            &type_lookup,
        )
    })
}
