use zoya_codegen::{codegen, format_export_path};
use zoya_ir::{CheckedPackage, DefinitionLookup};
use zoya_package::QualifiedPath;

use zoya_value::Value;

use crate::eval::{self, EvalError};

/// Runner for executing a checked Zoya package.
///
/// Eagerly compiles the package to JavaScript and builds a type lookup
/// on construction, so that `run()`/`run_async()` can be called multiple
/// times without repeating this work.
pub struct Runner<'a> {
    package: &'a CheckedPackage,
    code: String,
    type_lookup: DefinitionLookup,
}

impl<'a> Runner<'a> {
    /// Create a new runner for the given package and its dependencies.
    ///
    /// This eagerly runs codegen and builds the type lookup table.
    pub fn new(
        package: &'a CheckedPackage,
        deps: impl IntoIterator<Item = &'a CheckedPackage>,
    ) -> Self {
        let deps: Vec<_> = deps.into_iter().collect();
        let code = codegen(package, &deps).code;
        let type_lookup = DefinitionLookup::from_packages(package, &deps);
        Runner {
            package,
            code,
            type_lookup,
        }
    }

    /// Execute a function synchronously and return the result.
    ///
    /// Creates a single-threaded tokio runtime internally. Use `run_async()`
    /// when already inside a tokio runtime (e.g., HTTP handlers).
    pub fn run(&self, path: QualifiedPath, args: Vec<Value>) -> Result<Value, EvalError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| EvalError::RuntimeError(format!("failed to create tokio runtime: {e}")))?;
        rt.block_on(self.run_async(path, args))
    }

    /// Execute a function asynchronously and return the result.
    ///
    /// Use this when already inside a tokio runtime (e.g., HTTP handlers).
    pub async fn run_async(
        &self,
        path: QualifiedPath,
        args: Vec<Value>,
    ) -> Result<Value, EvalError> {
        // Find the function in the package definitions
        let func_def = self
            .package
            .definitions
            .get(&path)
            .and_then(|d| d.as_function())
            .ok_or_else(|| EvalError::RuntimeError(format!("function {} not found", path)))?;

        // Validate argument count
        if func_def.params.len() != args.len() {
            return Err(EvalError::RuntimeError(format!(
                "{}() expects {} argument(s), got {}",
                path.last(),
                func_def.params.len(),
                args.len()
            )));
        }

        let return_type = func_def.return_type.clone();

        // Validate each arg's type
        for (i, (arg, param_type)) in args.iter().zip(func_def.params.iter()).enumerate() {
            arg.check_type(param_type, &self.type_lookup).map_err(|e| {
                EvalError::RuntimeError(format!("argument {} type mismatch: {}", i, e))
            })?;
        }

        // Build the entry function name using the package name
        let entry_func = format_export_path(&path, &self.package.name);

        // Create async runtime (no module system needed)
        let (_runtime, context) = eval::create_async_runtime().await?;

        // Evaluate the script inside the async context
        let code = &self.code;
        let type_lookup = &self.type_lookup;
        rquickjs::async_with!(context => |ctx| {
            eval::inject_globals(&ctx)?;
            eval::eval_script_async(
                &ctx,
                code,
                &entry_func,
                &args,
                return_type,
                type_lookup,
            )
            .await
        })
        .await
    }
}
