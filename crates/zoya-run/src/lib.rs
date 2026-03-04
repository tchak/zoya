mod eval;

pub use eval::EvalError;
pub use zoya_build::BuildOutput;
pub use zoya_value::{FetchService, Job, TerminationError, Value, ValueData};

use zoya_package::QualifiedPath;

/// Execute a function from a `BuildOutput` synchronously and return the result.
///
/// Creates a single-threaded tokio runtime internally. Use `run_async()`
/// when already inside a tokio runtime (e.g., HTTP handlers).
///
/// The `fetch` service handles all `fetch()` calls from JS. Use
/// `HttpFetchService::new().into_service()` for standard HTTP(S) fetching.
pub fn run(
    output: &BuildOutput,
    entry: &QualifiedPath,
    args: &[Value],
    fetch: FetchService,
) -> Result<(Value, Vec<Job>), EvalError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| EvalError::RuntimeError(format!("failed to create tokio runtime: {e}")))?;
    rt.block_on(run_async(output, entry, args, fetch))
}

/// Execute a function from a `BuildOutput` asynchronously and return the result.
///
/// Use this when already inside a tokio runtime (e.g., HTTP handlers).
///
/// The `fetch` service handles all `fetch()` calls from JS. Use
/// `HttpFetchService::new().into_service()` for standard HTTP(S) fetching.
pub async fn run_async(
    output: &BuildOutput,
    entry: &QualifiedPath,
    args: &[Value],
    fetch: FetchService,
) -> Result<(Value, Vec<Job>), EvalError> {
    eval::run_code(
        &output.name,
        &output.output.code,
        &output.definitions,
        entry,
        args,
        &output.jobs,
        fetch,
    )
    .await
}
