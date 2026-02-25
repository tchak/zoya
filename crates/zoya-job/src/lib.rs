use std::sync::Arc;

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use zoya_build::BuildOutput;
use zoya_package::QualifiedPath;
use zoya_run::EvalError;
use zoya_value::{Value, ValueData};

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
    if !output.jobs.contains(&request.path) {
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
    storage: &mut MemoryStorage<JobRequest>,
    output: &BuildOutput,
    path: QualifiedPath,
    args: Vec<Value>,
) -> Result<(), JobError> {
    let request = JobRequest { path, args };
    validate(output, &request)?;
    storage
        .enqueue(request)
        .await
        .map_err(|_| JobError::RuntimeError("failed to enqueue job".to_string()))
}

/// Shared state for the job worker.
struct JobWorkerState {
    output: BuildOutput,
}

/// Create and run an apalis worker that processes `JobRequest` items.
///
/// The worker re-validates each request before execution, then runs the
/// function via `zoya_run::run_async()`. Non-retryable validation errors
/// are logged and skipped; transient errors (panics, runtime errors, job
/// errors) are returned as `Err` so apalis can retry them.
pub async fn worker(
    storage: MemoryStorage<JobRequest>,
    output: BuildOutput,
) -> Result<(), std::io::Error> {
    let state = Arc::new(JobWorkerState { output });

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

async fn handle_job(request: JobRequest, state: Data<Arc<JobWorkerState>>) -> Result<(), JobError> {
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
        Ok(value) => interpret_job_result(&value),
        Err(EvalError::Panic(msg)) => Err(JobError::Panic(msg)),
        Err(e) => Err(JobError::RuntimeError(e.to_string())),
    }
}

/// Interpret a job function's return value.
///
/// - `()` → Ok
/// - `Result::Ok(...)` → Ok
/// - `Result::Err(e)` → Err (retryable)
/// - `Task(inner)` → recurse
pub fn interpret_job_result(value: &Value) -> Result<(), JobError> {
    match value {
        Value::Tuple(elems) if elems.is_empty() => Ok(()),
        Value::EnumVariant {
            enum_name,
            variant_name,
            data: ValueData::Tuple(_),
            ..
        } if enum_name == "Result" && variant_name == "Ok" => Ok(()),
        Value::EnumVariant {
            enum_name,
            variant_name,
            data: ValueData::Tuple(values),
            ..
        } if enum_name == "Result" && variant_name == "Err" => {
            let msg = values
                .first()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "job failed".to_string());
            Err(JobError::JobReturnedError(msg))
        }
        Value::Task(inner) => interpret_job_result(inner),
        _ => Err(JobError::RuntimeError(format!(
            "unexpected job return value: {value}"
        ))),
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

    // ── interpret_job_result tests ──────────────────────────────────

    #[test]
    fn interpret_unit_ok() {
        let value = Value::Tuple(vec![]);
        assert_eq!(interpret_job_result(&value), Ok(()));
    }

    #[test]
    fn interpret_result_ok() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert_eq!(interpret_job_result(&value), Ok(()));
    }

    #[test]
    fn interpret_result_err() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            data: ValueData::Tuple(vec![Value::String("something failed".to_string())]),
        };
        let err = interpret_job_result(&value).unwrap_err();
        assert!(err.is_retryable());
        assert!(err.to_string().contains("something failed"));
    }

    #[test]
    fn interpret_task_unit() {
        let value = Value::Task(Box::new(Value::Tuple(vec![])));
        assert_eq!(interpret_job_result(&value), Ok(()));
    }

    #[test]
    fn interpret_task_result_ok() {
        let value = Value::Task(Box::new(Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        }));
        assert_eq!(interpret_job_result(&value), Ok(()));
    }

    #[test]
    fn interpret_task_result_err() {
        let value = Value::Task(Box::new(Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            data: ValueData::Tuple(vec![Value::String("async failed".to_string())]),
        }));
        let err = interpret_job_result(&value).unwrap_err();
        assert!(err.is_retryable());
        assert!(err.to_string().contains("async failed"));
    }

    #[test]
    fn interpret_unexpected_value() {
        let value = Value::Int(42);
        let err = interpret_job_result(&value).unwrap_err();
        assert!(err.is_retryable()); // RuntimeError is retryable
        assert!(err.to_string().contains("unexpected job return value"));
    }

    // ── validate tests ─────────────────────────────────────────────

    #[test]
    fn validate_not_found() {
        let output = BuildOutput {
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
        };
        let request = JobRequest {
            path: QualifiedPath::root().child("missing"),
            args: vec![],
        };
        let err = validate(&output, &request).unwrap_err();
        assert_eq!(err, JobError::NotFound("root::missing".to_string()));
        assert!(!err.is_retryable());
    }
}
