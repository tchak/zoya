# zoya-job

Background job processing for the Zoya programming language.

Provides job validation, enqueueing, persistence, and worker execution for functions annotated with `#[job]`. Jobs are stored in SQLite via [apalis](https://github.com/geofmureithi/apalis) and processed asynchronously with automatic retry.

## Features

- **Job validation** - Validates path, arity, and argument types against the build output
- **SQLite persistence** - Durable job storage with apalis-sql
- **Automatic retry** - Transient errors (panics, runtime errors) are retried; structural errors (arity, type mismatch) are skipped
- **Worker processing** - apalis-based async worker that executes jobs in QuickJS
- **Job chaining** - Jobs can enqueue child jobs during execution
- **In-memory storage** - Convenience constructor for testing and development

## Usage

### Validate a job request

```rust
use zoya_build::build_from_path;
use zoya_job::validate;
use zoya_loader::Mode;
use zoya_package::QualifiedPath;
use zoya_value::{Job, Value};
use std::path::Path;

let output = build_from_path(Path::new("my_project"), Mode::Dev)?;

let job = Job {
    path: QualifiedPath::root().child("deploy"),
    args: vec![Value::String("production".into())],
};

// Validates path exists, arity matches, and argument types are correct
validate(&output, &job)?;
```

### Enqueue and list jobs

```rust
use zoya_job::{enqueue, list, memory_storage};
use zoya_value::Job;
use zoya_package::QualifiedPath;

// Create in-memory storage (use SqliteStorage::new(pool) for production)
let mut storage = memory_storage().await?;

// Enqueue a job (validates before storing)
let job = Job {
    path: QualifiedPath::root().child("deploy"),
    args: vec![],
};
enqueue(&mut storage, &output, job).await?;

// List all jobs grouped by definition
let jobs = list(&storage, &output).await?;
for (path, variant_name, pending) in &jobs {
    println!("{}: {} pending", path, pending.len());
}
```

### Run a worker

```rust
use zoya_job::worker;

// Start processing jobs (blocks until shutdown)
worker(storage, output).await?;
```

## Public API

```rust
/// Validate a job request against a build output.
pub fn validate(output: &BuildOutput, request: &Job) -> Result<(), JobError>;

/// Enqueue a job for background processing (validates first).
pub async fn enqueue(
    storage: &mut SqliteStorage<Job>,
    output: &BuildOutput,
    job: Job,
) -> Result<(), JobError>;

/// Create an in-memory SQLite job storage.
pub async fn memory_storage() -> Result<SqliteStorage<Job>, JobError>;

/// List all defined jobs with their pending enqueued instances.
pub async fn list(
    storage: &SqliteStorage<Job>,
    output: &BuildOutput,
) -> Result<Vec<(QualifiedPath, String, Vec<Job>)>, JobError>;

/// Create and run an apalis worker that processes jobs.
pub async fn worker(
    storage: SqliteStorage<Job>,
    output: BuildOutput,
) -> Result<(), std::io::Error>;
```

## Error Handling

```rust
/// Errors during job validation or execution.
pub enum JobError {
    NotFound(String),
    ArityMismatch { expected: usize, actual: usize },
    TypeMismatch { index: usize, detail: String },
    Panic(String),
    RuntimeError(String),
    JobReturnedError(String),
}

impl JobError {
    /// Returns `true` for transient errors that should be retried.
    pub fn is_retryable(&self) -> bool;
}
```

| Variant | Retryable | Description |
|---------|-----------|-------------|
| `NotFound` | No | Job path not found in build output |
| `ArityMismatch` | No | Wrong number of arguments |
| `TypeMismatch` | No | Argument type doesn't match parameter |
| `Panic` | Yes | Zoya `panic()` was called |
| `RuntimeError` | Yes | JavaScript runtime error |
| `JobReturnedError` | Yes | Job function returned an error value |

## Dependencies

- [zoya-build](../zoya-build) - Build pipeline
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-package](../zoya-package) - Package data structures
- [zoya-run](../zoya-run) - Runtime execution
- [zoya-value](../zoya-value) - Runtime value types
- [apalis](https://github.com/geofmureithi/apalis) - Background job processing
- [apalis-sql](https://github.com/geofmureithi/apalis) - SQLite storage backend
- [sqlx](https://github.com/launchbadge/sqlx) - SQLite driver
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
- [tracing](https://github.com/tokio-rs/tracing) - Structured logging
