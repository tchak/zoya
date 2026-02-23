use zoya_codegen::{codegen, format_export_path};
use zoya_ir::{CheckedPackage, DefinitionLookup};
use zoya_package::QualifiedPath;

use zoya_value::Value;

use crate::eval::{self, EvalError};

/// Runner for executing a checked Zoya package.
pub struct Runner<'a> {
    package: &'a CheckedPackage,
    deps: Vec<&'a CheckedPackage>,
    entry_path: QualifiedPath,
    entry_args: Vec<Value>,
}

impl<'a> Runner<'a> {
    /// Create a new runner for the given package and its dependencies.
    pub fn new(
        package: &'a CheckedPackage,
        deps: impl IntoIterator<Item = &'a CheckedPackage>,
    ) -> Self {
        Runner {
            package,
            deps: deps.into_iter().collect(),
            entry_path: QualifiedPath::root().child("main"),
            entry_args: vec![],
        }
    }

    /// Select an arbitrary function to run by its full qualified path, with args.
    pub fn entry(mut self, path: QualifiedPath, args: Vec<Value>) -> Self {
        self.entry_path = path;
        self.entry_args = args;
        self
    }

    /// Execute the package synchronously and return the result.
    ///
    /// Creates a single-threaded tokio runtime internally. Use `run_async()`
    /// when already inside a tokio runtime (e.g., HTTP handlers).
    pub fn run(self) -> Result<Value, EvalError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| EvalError::RuntimeError(format!("failed to create tokio runtime: {e}")))?;
        rt.block_on(self.run_async())
    }

    /// Execute the package asynchronously and return the result.
    ///
    /// Use this when already inside a tokio runtime (e.g., HTTP handlers).
    pub async fn run_async(self) -> Result<Value, EvalError> {
        run_checked_async(self.package, &self.deps, &self.entry_path, &self.entry_args).await
    }
}

/// Internal: execute an already-checked package asynchronously.
async fn run_checked_async(
    package: &CheckedPackage,
    deps: &[&CheckedPackage],
    function_path: &QualifiedPath,
    args: &[Value],
) -> Result<Value, EvalError> {
    // Find the function in the package definitions
    let func_def = package
        .definitions
        .get(function_path)
        .and_then(|d| d.as_function())
        .ok_or_else(|| {
            EvalError::RuntimeError(format!("function {} not found", function_path))
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
    let entry_func = format_export_path(function_path, &package.name);

    // Create async runtime (no module system needed)
    let (_runtime, context) = eval::create_async_runtime().await?;

    // Evaluate the script inside the async context
    let code = output.code;
    rquickjs::async_with!(context => |ctx| {
        eval::inject_globals(&ctx)?;
        eval::eval_script_async(
            &ctx,
            &code,
            &entry_func,
            args,
            return_type,
            &type_lookup,
        )
        .await
    })
    .await
}
