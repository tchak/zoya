use std::sync::Arc;

use apalis::prelude::*;
use apalis_sql::sqlite::SqliteStorage;
pub use apalis_sql::sqlx;
use sqlx::SqlitePool;
use zoya_build::BuildOutput;
use zoya_package::QualifiedPath;
use zoya_run::EvalError;
use zoya_value::{Job, TerminationError};

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
pub fn validate(output: &BuildOutput, request: &Job) -> Result<(), JobError> {
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
pub async fn enqueue(
    storage: &mut SqliteStorage<Job>,
    output: &BuildOutput,
    job: Job,
) -> Result<(), JobError> {
    validate(output, &job)?;
    storage
        .push(job)
        .await
        .map_err(|_| JobError::RuntimeError("failed to enqueue job".to_string()))?;
    Ok(())
}

/// Create an in-memory SQLite job storage (for tests and development).
pub async fn memory_storage() -> Result<SqliteStorage<Job>, JobError> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .map_err(|e| JobError::RuntimeError(format!("failed to connect: {e}")))?;
    SqliteStorage::setup(&pool)
        .await
        .map_err(|e| JobError::RuntimeError(format!("failed to run migrations: {e}")))?;
    Ok(SqliteStorage::new(pool))
}

/// List all defined jobs and their pending enqueued instances from SQL storage.
pub async fn list(
    storage: &SqliteStorage<Job>,
    output: &BuildOutput,
) -> Result<Vec<(QualifiedPath, String, Vec<Job>)>, JobError> {
    // Fetch all pending jobs, paginating through results
    let mut pending = Vec::new();
    let mut page = 1;
    loop {
        let batch = storage
            .list_jobs(&State::Pending, page)
            .await
            .map_err(|e| JobError::RuntimeError(e.to_string()))?;
        if batch.is_empty() {
            break;
        }
        pending.extend(batch);
        page += 1;
    }

    // Group by job definition
    let result = output
        .jobs
        .iter()
        .map(|(path, variant_name)| {
            let jobs = pending
                .iter()
                .filter(|r| r.args.path == *path)
                .map(|r| r.args.clone())
                .collect();
            (path.clone(), variant_name.clone(), jobs)
        })
        .collect();

    Ok(result)
}

/// Shared state for the job worker.
struct JobWorkerState {
    output: BuildOutput,
    storage: SqliteStorage<Job>,
}

/// Create and run an apalis worker that processes `Job` items.
///
/// The worker re-validates each request before execution, then runs the
/// function via `zoya_run::run_async()`. Non-retryable validation errors
/// are logged and skipped; transient errors (panics, runtime errors, job
/// errors) are returned as `Err` so apalis can retry them.
pub async fn worker(
    storage: SqliteStorage<Job>,
    output: BuildOutput,
) -> Result<(), std::io::Error> {
    let state = Arc::new(JobWorkerState {
        output,
        storage: storage.clone(),
    });

    Monitor::new()
        .register(
            WorkerBuilder::new(&state.output.name)
                .data(state.clone())
                .backend(storage)
                .build_fn(handle_job),
        )
        .run()
        .await
}

async fn handle_job(request: Job, state: Data<Arc<JobWorkerState>>) -> Result<(), JobError> {
    // Re-validate on the worker side
    if let Err(e) = validate(&state.output, &request) {
        if !e.is_retryable() {
            tracing::warn!("skipping non-retryable job {}: {e}", request.path);
            return Ok(());
        }
        return Err(e);
    }

    // Execute the job function
    match zoya_run::run_async(
        &state.output,
        &request.path,
        &request.args,
        zoya_fetch::HttpFetchService::new().into_service(),
    )
    .await
    {
        Ok((value, jobs)) => {
            let result = value.termination().map_err(|e| match e {
                TerminationError::Failed(msg) => JobError::JobReturnedError(msg),
                TerminationError::UnexpectedReturn(msg) => JobError::RuntimeError(msg),
            });

            if result.is_ok() {
                for job in jobs {
                    let mut storage = state.storage.clone();
                    if let Err(e) = enqueue(&mut storage, &state.output, job).await {
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
    use zoya_value::Value;

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
        let request = Job {
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
        let mut storage = memory_storage().await.unwrap();
        enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("my_job"),
                args: vec![],
            },
        )
        .await
        .unwrap();

        let len = storage.len().await.unwrap();
        assert_eq!(len, 1);
    }

    #[tokio::test]
    async fn enqueue_not_found() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let mut storage = memory_storage().await.unwrap();
        let err = enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("missing"),
                args: vec![],
            },
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
        let mut storage = memory_storage().await.unwrap();
        let err = enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("my_job"),
                args: vec![],
            },
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
        let mut storage = memory_storage().await.unwrap();
        let err = enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("my_job"),
                args: vec![Value::String("hello".to_string())],
            },
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
        let storage = memory_storage().await.unwrap();
        let state = Arc::new(JobWorkerState { output, storage });
        let request = Job {
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
        let storage = memory_storage().await.unwrap();
        let state = Arc::new(JobWorkerState { output, storage });
        let request = Job {
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
        let storage = memory_storage().await.unwrap();
        let state = Arc::new(JobWorkerState { output, storage });
        let request = Job {
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
        let storage = memory_storage().await.unwrap();
        let state = Arc::new(JobWorkerState { output, storage });
        let request = Job {
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
        storage: SqliteStorage<Job>,
    ) -> impl tower::Service<Request<Job, ()>, Response = (), Error = JobError, Future = impl Send>
    + Send
    + Sync
    + Clone {
        let state = Arc::new(JobWorkerState { output, storage });
        tower::service_fn(move |req: Request<Job, ()>| {
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
        let storage = memory_storage().await.unwrap();
        let svc = make_service(output, storage);

        let mem = MemoryStorage::<Job>::new();
        let (mut tester, poller) = apalis_core::test_utils::TestWrapper::new_with_service(mem, svc);
        tokio::spawn(poller);

        tester
            .enqueue(Job {
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
        let storage = memory_storage().await.unwrap();
        let svc = make_service(output, storage);

        let mem = MemoryStorage::<Job>::new();
        let (mut tester, poller) = apalis_core::test_utils::TestWrapper::new_with_service(mem, svc);
        tokio::spawn(poller);

        tester
            .enqueue(Job {
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
        let storage = memory_storage().await.unwrap();
        let svc = make_service(output, storage);

        let mem = MemoryStorage::<Job>::new();
        let (mut tester, poller) = apalis_core::test_utils::TestWrapper::new_with_service(mem, svc);
        tokio::spawn(poller);

        tester
            .enqueue(Job {
                path: QualifiedPath::root().child("greet"),
                args: vec![Value::String("world".to_string())],
            })
            .await
            .unwrap();

        let (_, result) = tester.execute_next().await.unwrap();
        assert_eq!(result, Ok("()".to_string()));
    }

    // ── list tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn list_with_enqueued_jobs() {
        let output = build_source(
            r#"
            #[job]
            pub fn job_a() -> () { () }

            #[job]
            pub fn job_b(x: Int) -> () { () }
        "#,
        );
        let mut storage = memory_storage().await.unwrap();

        enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("job_a"),
                args: vec![],
            },
        )
        .await
        .unwrap();
        enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("job_a"),
                args: vec![],
            },
        )
        .await
        .unwrap();
        enqueue(
            &mut storage,
            &output,
            Job {
                path: QualifiedPath::root().child("job_b"),
                args: vec![Value::Int(42)],
            },
        )
        .await
        .unwrap();

        let jobs = list(&storage, &output).await.unwrap();
        assert_eq!(jobs.len(), 2);

        let (_, _, enqueued_a) = jobs.iter().find(|(p, _, _)| p.last() == "job_a").unwrap();
        assert_eq!(enqueued_a.len(), 2);

        let (_, _, enqueued_b) = jobs.iter().find(|(p, _, _)| p.last() == "job_b").unwrap();
        assert_eq!(enqueued_b.len(), 1);
        assert_eq!(enqueued_b[0].args, vec![Value::Int(42)]);
    }

    #[tokio::test]
    async fn list_empty_storage() {
        let output = build_source(
            r#"
            #[job]
            pub fn my_job() -> () { () }
        "#,
        );
        let storage = memory_storage().await.unwrap();

        let jobs = list(&storage, &output).await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].2.is_empty());
    }
}
