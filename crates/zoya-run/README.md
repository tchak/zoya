# zoya-run

Runtime execution for the Zoya programming language.

Provides a function-based API to run Zoya programs by compiling to JavaScript and executing via QuickJS.

## Features

- **Simple API** - Free functions `run()` and `run_async()` operate on `BuildOutput`
- **Package execution** - Run type-checked packages with module and dependency support
- **Custom entry points** - Run any function by path with arguments
- **Value marshaling** - Convert JavaScript results to typed Zoya values
- **Job collection** - Captures any jobs enqueued during execution

## Usage

### Run a built package

```rust
use zoya_build::build_from_path;
use zoya_loader::Mode;
use zoya_package::QualifiedPath;
use std::path::Path;

// Build (load, type-check, codegen)
let output = build_from_path(Path::new("src/main.zy"), Mode::Dev)?;

// Run the main function in the root module
let path = QualifiedPath::root().child("main");
let (result, jobs) = zoya_run::run(&output, &path, &[])?;
println!("Result: {}", result);
println!("Enqueued {} jobs", jobs.len());
```

### Run a specific function with arguments

```rust
use zoya_run::Value;
use zoya_package::QualifiedPath;

// Run any function by its qualified path, passing arguments
let fn_path = QualifiedPath::root().child("add");
let (result, _jobs) = zoya_run::run(&output, &fn_path, &[Value::Int(1), Value::Int(2)])?;
assert_eq!(result, Value::Int(3));
```

### Async execution

```rust
use zoya_run::run_async;

// Use run_async when already inside a tokio runtime (e.g., HTTP handlers)
let (result, jobs) = run_async(&output, &path, &args).await?;
```

## Public API

```rust
/// Execute a function from a `BuildOutput` synchronously.
/// Creates a single-threaded tokio runtime internally.
pub fn run(
    output: &BuildOutput,
    entry: &QualifiedPath,
    args: &[Value],
) -> Result<(Value, Vec<Job>), EvalError>;

/// Execute a function from a `BuildOutput` asynchronously.
/// Use when already inside a tokio runtime.
pub async fn run_async(
    output: &BuildOutput,
    entry: &QualifiedPath,
    args: &[Value],
) -> Result<(Value, Vec<Job>), EvalError>;
```

## Value Types

Runtime values are defined in the [zoya-value](../zoya-value) crate and re-exported from `zoya-run`:

```rust
use zoya_run::{Value, ValueData, Job, TerminationError};

// JSON serialization
println!("{}", result.to_json()); // "42"

// Check for test/job termination errors
result.termination()?;
```

See [zoya-value](../zoya-value) for full `Value`, `ValueData`, and `JSValue` documentation.

## Error Handling

```rust
/// Main runtime error type for execution failures.
pub enum EvalError {
    /// Zoya `panic()` was called
    Panic(String),
    /// Any other runtime error (JS execution, value conversion)
    RuntimeError(String),
    /// Package loading error (file IO, lex, parse, config)
    LoadError(zoya_loader::LoaderError<String>),
    /// Type checking error
    TypeError(zoya_ir::TypeError),
}
```

`EvalError` implements `From` for `zoya_value::Error`, `LoaderError<String>`, and `TypeError` for automatic `?` propagation.

## Dependencies

- [zoya-build](../zoya-build) - Build pipeline (for `BuildOutput`)
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-package](../zoya-package) - Package data structures
- [zoya-value](../zoya-value) - Runtime value types and serialization
- [rquickjs](https://github.com/aspect-build/rquickjs) - JavaScript runtime (QuickJS)
- [tokio](https://github.com/tokio-rs/tokio) - Async runtime
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
