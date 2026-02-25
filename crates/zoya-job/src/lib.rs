use std::sync::Arc;

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use tower::layer::util::Identity;
use zoya_build::BuildOutput;
use zoya_package::QualifiedPath;
use zoya_run::EvalError;
use zoya_value::{TerminationError, Value};

/// A serializable job request for background execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub path: QualifiedPath,
    pub args: Vec<Value>,
}

/// Errors that can occur during job validation or execution.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum JobError {
    #[error("job not found: {0}")]
    NotFound(String),
    #[error("arity mismatch: expected {expected} argument(s), got {actual}")]
    ArityMismatch { expected: usize, actual: usize },
    #[error("argument {index} type mismatch: {detail}")]
    TypeMismatch { index: usize, detail: String },
    #[error("panic: {0}")]
    Panic(String),
    #[error("runtime error: {0}")]
    RuntimeError(String),
    #[error("job returned error: {0}")]
    JobReturnedError(String),
}

impl JobError {
    /// Returns `true` for transient errors that should be retried.
    pub fn is_retryable(&self) -> bool {
        match self {
            JobError::NotFound(_)
            | JobError::ArityMismatch { .. }
            | JobError::TypeMismatch { .. } => false,
            JobError::Panic(_) | JobError::RuntimeError(_) | JobError::JobReturnedError(_) => true,
        }
    }
}

/// Validate a job request against a build output.
///
/// Checks that:
/// 1. The path refers to a known job function
/// 2. The argument count matches the parameter count
/// 3. Each argument matches the expected parameter type
pub fn validate(output: &BuildOutput, request: &JobRequest) -> Result<(), JobError> {
    // Check the path exists in the jobs list
    if !output.jobs.iter().any(|(p, _)| p == &request.path) {
        return Err(JobError::NotFound(request.path.to_string()));
    }

    // Look up the function definition
    let func = output
        .definitions
        .get_function(&request.path)
        .ok_or_else(|| JobError::NotFound(request.path.to_string()))?;

    // Check arity
    if func.params.len() != request.args.len() {
        return Err(JobError::ArityMismatch {
            expected: func.params.len(),
            actual: request.args.len(),
        });
    }

    // Check each argument type
    for (i, (arg, param_type)) in request.args.iter().zip(func.params.iter()).enumerate() {
        arg.check_type(param_type, &output.definitions)
            .map_err(|e| JobError::TypeMismatch {
                index: i,
                detail: e.to_string(),
            })?;
    }

    Ok(())
}

/// Enqueue a job for background processing.
///
/// Validates the request before enqueuing. Returns early with a `JobError`
/// if validation fails (fail-fast on structural errors).
pub async fn enqueue<S>(
    storage: &mut S,
    output: &BuildOutput,
    path: QualifiedPath,
    args: Vec<Value>,
) -> Result<(), JobError>
where
    S: MessageQueue<JobRequest>,
{
    let request = JobRequest { path, args };
    validate(output, &request)?;
    storage
        .enqueue(request)
        .await
        .map_err(|_| JobError::RuntimeError("failed to enqueue job".to_string()))
}

/// Shared state for the job worker.
struct JobWorkerState<S> {
    output: BuildOutput,
    storage: S,
}

/// Create and run an apalis worker that processes `JobRequest` items.
///
/// The worker re-validates each request before execution, then runs the
/// function via `zoya_run::run_async()`. Non-retryable validation errors
/// are logged and skipped; transient errors (panics, runtime errors, job
/// errors) are returned as `Err` so apalis can retry them.
pub async fn worker<S>(
    storage: S,
    output: BuildOutput,
) -> Result<(), std::io::Error>
where
    S: MessageQueue<JobRequest>
        + Backend<Request<JobRequest, S::Context>, Layer = Identity>
        + Clone
        + Send
        + Sync
        + 'static,
    S::Context: Send + Sync + 'static,
    <S as Backend<Request<JobRequest, S::Context>>>::Stream: Unpin + Send + 'static,
{
    let state = Arc::new(JobWorkerState {
        output,
        storage: storage.clone(),
    });

    Monitor::new()
        .register(
            WorkerBuilder::new(&state.output.name)
                .data(state.clone())
                .backend(storage)
                .build_fn(handle_job::<S>),
        )
        .run()
        .await
}

async fn handle_job<S>(
    request: JobRequest,
    state: Data<Arc<JobWorkerState<S>>>,
) -> Result<(), JobError>
where
    S: MessageQueue<JobRequest> + Clone + Send + Sync + 'static,
{
    // Re-validate on the worker side
    if let Err(e) = validate(&state.output, &request) {
        if !e.is_retryable() {
            tracing::warn!("skipping non-retryable job {}: {e}", request.path);
            return Ok(());
        }
        return Err(e);
    }

    // Execute the job function
    match zoya_run::run_async(&state.output, &request.path, &request.args).await {
        Ok((value, jobs)) => {
            let result = value.termination().map_err(|e| match e {
                TerminationError::Failed(msg) => JobError::JobReturnedError(msg),
                TerminationError::UnexpectedReturn(msg) => JobError::RuntimeError(msg),
            });

            if result.is_ok() {
                for job in jobs {
                    let mut storage = state.storage.clone();
                    if let Err(e) = enqueue(&mut storage, &state.output, job.path, job.args).await {
                        tracing::warn!("failed to enqueue child job: {e}");
                    }
                }
            }

            result
        }
        Err(EvalError::Panic(msg)) => Err(JobError::Panic(msg)),
        Err(e) => Err(JobError::RuntimeError(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── JobError::is_retryable tests ────────────────────────────────

    #[test]
    fn not_found_is_not_retryable() {
        assert!(!JobError::NotFound("foo".into()).is_retryable());
    }

    #[test]
    fn arity_mismatch_is_not_retryable() {
        assert!(
            !JobError::ArityMismatch {
                expected: 1,
                actual: 2
            }
            .is_retryable()
        );
    }

    #[test]
    fn type_mismatch_is_not_retryable() {
        assert!(
            !JobError::TypeMismatch {
                index: 0,
                detail: "Int vs String".into()
            }
            .is_retryable()
        );
    }

    #[test]
    fn panic_is_retryable() {
        assert!(JobError::Panic("boom".into()).is_retryable());
    }

    #[test]
    fn runtime_error_is_retryable() {
        assert!(JobError::RuntimeError("oops".into()).is_retryable());
    }

    #[test]
    fn job_returned_error_is_retryable() {
        assert!(JobError::JobReturnedError("failed".into()).is_retryable());
    }

    // ── validate tests ─────────────────────────────────────────────

    #[test]
    fn validate_not_found() {
        let output = empty_output();
        let request = JobRequest {
            path: QualifiedPath::root().child("missing"),
            args: vec![],
        };
        let err = validate(&output, &request).unwrap_err();
        assert_eq!(err, JobError::NotFound("root::missing".to_string()));
        assert!(!err.is_retryable());
    }

    // ── helpers ─────────────────────────────────────────────────────

    fn empty_output() -> BuildOutput {
        BuildOutput {
            name: "test".to_string(),
            output: zoya_codegen::CodegenOutput {
                code: String::new(),
                hash: String::new(),
            },
            definitions: zoya_ir::DefinitionLookup::empty(),
            functions: vec![],
            tests: vec![],
            jobs: vec![],
            routes: vec![],
        }
    }

    fn build_source(source: &str) -> BuildOutput {
        let mem_source = zoya_loader::MemorySource::new().with_module("root", source);
        let package = zoya_loader::load_memory_package(&mem_source, zoya_loader::Mode::Dev)
            .expect("failed to load package");
        zoya_build::build(&package).expect("failed to build package")
    }

    // ── enqueue tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn enqueue_valid_job() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let mut storage = MemoryStorage::<JobRequest>::new();
        enqueue(
            &mut storage,
            &output,
            QualifiedPath::root().child("my_job"),
            vec![],
        )
        .await
        .unwrap();

        let dequeued = storage.dequeue().await.unwrap();
        assert!(dequeued.is_some());
        let request = dequeued.unwrap();
        assert_eq!(request.path, QualifiedPath::root().child("my_job"));
        assert!(request.args.is_empty());
    }

    #[tokio::test]
    async fn enqueue_not_found() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let mut storage = MemoryStorage::<JobRequest>::new();
        let err = enqueue(
            &mut storage,
            &output,
            QualifiedPath::root().child("missing"),
            vec![],
        )
        .await
        .unwrap_err();
        assert_eq!(err, JobError::NotFound("root::missing".to_string()));
    }

    #[tokio::test]
    async fn enqueue_arity_mismatch() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job(x: Int) -> () { () }
        "#,
        );
        let mut storage = MemoryStorage::<JobRequest>::new();
        let err = enqueue(
            &mut storage,
            &output,
            QualifiedPath::root().child("my_job"),
            vec![],
        )
        .await
        .unwrap_err();
        assert_eq!(
            err,
            JobError::ArityMismatch {
                expected: 1,
                actual: 0
            }
        );
    }

    #[tokio::test]
    async fn enqueue_type_mismatch() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job(x: Int) -> () { () }
        "#,
        );
        let mut storage = MemoryStorage::<JobRequest>::new();
        let err = enqueue(
            &mut storage,
            &output,
            QualifiedPath::root().child("my_job"),
            vec![Value::String("hello".to_string())],
        )
        .await
        .unwrap_err();
        assert!(matches!(err, JobError::TypeMismatch { index: 0, .. }));
    }

    // ── handle_job tests ────────────────────────────────────────────

    #[tokio::test]
    async fn handle_job_success() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let state = Arc::new(JobWorkerState {
            output,
            storage: storage.clone(),
        });
        let request = JobRequest {
            path: QualifiedPath::root().child("my_job"),
            args: vec![],
        };
        let result = handle_job(request, Data::new(state)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handle_job_with_args() {
        let output = build_source(
            r#"
            #[job]
            pub fn add(x: Int, y: Int) -> () { () }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let state = Arc::new(JobWorkerState {
            output,
            storage: storage.clone(),
        });
        let request = JobRequest {
            path: QualifiedPath::root().child("add"),
            args: vec![Value::Int(1), Value::Int(2)],
        };
        let result = handle_job(request, Data::new(state)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handle_job_panic() {
        let output = build_source(
            r#"
            #[job]
            pub fn bad_job() -> () { panic("boom") }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let state = Arc::new(JobWorkerState {
            output,
            storage: storage.clone(),
        });
        let request = JobRequest {
            path: QualifiedPath::root().child("bad_job"),
            args: vec![],
        };
        let result = handle_job(request, Data::new(state)).await;
        assert_eq!(result, Err(JobError::Panic("boom".to_string())));
    }

    #[tokio::test]
    async fn handle_job_skips_invalid_non_retryable() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let state = Arc::new(JobWorkerState {
            output,
            storage: storage.clone(),
        });
        let request = JobRequest {
            path: QualifiedPath::root().child("missing"),
            args: vec![],
        };
        // Non-retryable validation error → returns Ok (skipped)
        let result = handle_job(request, Data::new(state)).await;
        assert!(result.is_ok());
    }

    // ── integration tests (TestWrapper) ─────────────────────────────

    fn make_service(
        output: BuildOutput,
        storage: MemoryStorage<JobRequest>,
    ) -> impl tower::Service<
        Request<JobRequest, ()>,
        Response = (),
        Error = JobError,
        Future = impl Send,
    > + Send
           + Sync
           + Clone {
        let state = Arc::new(JobWorkerState { output, storage });
        tower::service_fn(move |req: Request<JobRequest, ()>| {
            let state = state.clone();
            async move { handle_job(req.args, Data::new(state)).await }
        })
    }

    #[tokio::test]
    async fn worker_processes_job() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let svc = make_service(output, storage.clone());

        let (mut tester, poller) =
            apalis_core::test_utils::TestWrapper::new_with_service(storage, svc);
        tokio::spawn(poller);

        tester
            .enqueue(JobRequest {
                path: QualifiedPath::root().child("my_job"),
                args: vec![],
            })
            .await
            .unwrap();

        let (_, result) = tester.execute_next().await.unwrap();
        assert_eq!(result, Ok("()".to_string()));
    }

    #[tokio::test]
    async fn worker_reports_panic() {
        let output = build_source(
            r#"
            #[job]
            pub fn bad_job() -> () { panic("boom") }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let svc = make_service(output, storage.clone());

        let (mut tester, poller) =
            apalis_core::test_utils::TestWrapper::new_with_service(storage, svc);
        tokio::spawn(poller);

        tester
            .enqueue(JobRequest {
                path: QualifiedPath::root().child("bad_job"),
                args: vec![],
            })
            .await
            .unwrap();

        let (_, result) = tester.execute_next().await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("panic"));
    }

    #[tokio::test]
    async fn worker_processes_job_with_args() {
        let output = build_source(
            r#"
            #[job]
            pub fn greet(name: String) -> () { () }
        "#,
        );
        let storage = MemoryStorage::<JobRequest>::new();
        let svc = make_service(output, storage.clone());

        let (mut tester, poller) =
            apalis_core::test_utils::TestWrapper::new_with_service(storage, svc);
        tokio::spawn(poller);

        tester
            .enqueue(JobRequest {
                path: QualifiedPath::root().child("greet"),
                args: vec![Value::String("world".to_string())],
            })
            .await
            .unwrap();

        let (_, result) = tester.execute_next().await.unwrap();
        assert_eq!(result, Ok("()".to_string()));
    }
}
