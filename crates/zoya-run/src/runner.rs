use zoya_codegen::codegen;
use zoya_ir::{CheckedPackage, DefinitionLookup};
use zoya_package::QualifiedPath;

use zoya_value::Value;

use crate::eval::{self, EvalError};

/// Runner for executing a checked Zoya package.
///
/// Eagerly compiles the package to JavaScript and builds a definition lookup
/// on construction, so that `run()`/`run_async()` can be called multiple
/// times without repeating this work. Fully owned — no lifetime parameter.
pub struct Runner {
    name: String,
    code: String,
    definitions: DefinitionLookup,
}

impl Runner {
    /// Create a new runner for the given package and its dependencies.
    ///
    /// This eagerly runs codegen and builds the definition lookup table.
    pub fn new<'a>(
        package: &'a CheckedPackage,
        deps: impl IntoIterator<Item = &'a CheckedPackage>,
    ) -> Self {
        let deps: Vec<_> = deps.into_iter().collect();
        let code = codegen(package, &deps).code;
        let definitions = DefinitionLookup::from_packages(package, &deps);
        Runner {
            name: package.name.clone(),
            code,
            definitions,
        }
    }

    /// Get the definition lookup table.
    pub fn definitions(&self) -> &DefinitionLookup {
        &self.definitions
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
        eval::run_code(&self.name, &self.code, &self.definitions, path, args).await
    }
}
