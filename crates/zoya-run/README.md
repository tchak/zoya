# zoya-run

Runtime execution for the Zoya programming language.

Provides a builder-pattern API to run Zoya programs by compiling to JavaScript and executing via QuickJS.

## Features

- **Builder pattern** - Composable, type-safe run configuration
- **Package execution** - Run type-checked packages with module and dependency support
- **Source execution** - Compile and run source strings directly
- **File execution** - Load, check, and run `.zy` files
- **Custom entry points** - Run any function by path with arguments
- **Test runner** - Discover and run `#[test]` functions with streaming results
- **Value marshaling** - Convert JavaScript results to typed Zoya values

## Usage

### Run from source string

```rust
use zoya_run::{run_source, Value};

let result = run_source("pub fn main() -> Int { 42 }")?;
assert_eq!(result, Value::Int(42));
```

### Run from file

```rust
use zoya_run::{Runner, run_path};
use std::path::Path;

// Convenience function
let result = run_path(Path::new("program.zy"))?;

// Or with builder for mode control
let result = Runner::new()
    .path(Path::new("program.zy"))
    .mode(zoya_loader::Mode::Test)
    .run()?;
```

### Run a checked package

```rust
use zoya_check::check;
use zoya_loader::{load_package, Mode};
use zoya_run::Runner;
use zoya_std::std;
use std::path::Path;

// Load and type-check with standard library
let std = std();
let pkg = load_package(Path::new("src/main.zy"), Mode::Dev)?;
let checked_pkg = check(&pkg, &[std])?;

// Run the main function in the root module
let result = Runner::new()
    .package(&checked_pkg, [std])
    .run()?;
println!("Result: {}", result);
```

### Run a specific module's main function

```rust
use zoya_run::Runner;

// Run main() from the "repl" submodule
let result = Runner::new()
    .package(&checked_pkg, [std])
    .module("repl")
    .run()?;
```

### Run a specific function with arguments

```rust
use zoya_run::{Runner, Value};
use zoya_package::QualifiedPath;

// Run any function by its qualified path, passing arguments
let fn_path = QualifiedPath::root().child("add");
let result = Runner::new()
    .package(&checked_pkg, [std])
    .entry(fn_path, vec![Value::Int(1), Value::Int(2)])
    .run()?;
assert_eq!(result, Value::Int(3));
```

### Run tests

```rust
use zoya_run::{Runner, TestRunner, TestReport};
use std::path::Path;

// Discover and run tests
let test_runner = Runner::new().test(Path::new("src/main.zy"))?;
println!("Found {} tests", test_runner.tests.len());

// Run all tests
let report = test_runner.run()?;
println!("{} passed, {} failed", report.passed(), report.failed());

// Or with streaming results
let test_runner = Runner::new().test(Path::new("src/main.zy"))?;
let report = test_runner.execute(|result| {
    let status = if result.outcome.is_ok() { "PASS" } else { "FAIL" };
    println!("[{}] {}", status, result.path);
})?;
```

## Public API

```rust
/// Entry point — choose an input source.
pub struct Runner;

impl Runner {
    pub fn new() -> Self;
    pub fn package<'a>(self, pkg: &'a CheckedPackage, deps: impl IntoIterator<Item = &'a CheckedPackage>) -> PackageRunner<'a>;
    pub fn path(self, path: &Path) -> PathRunner;
    pub fn source(self, source: &str) -> SourceRunner;
    pub fn test(self, path: &Path) -> Result<TestRunner, EvalError>;
}

/// Run a pre-checked package. Optionally select a submodule.
pub struct PackageRunner<'a>;

impl PackageRunner<'_> {
    pub fn module(self, module: impl Into<String>) -> Self;
    pub fn entry(self, path: QualifiedPath, args: Vec<Value>) -> Self;
    pub fn run(self) -> Result<Value, EvalError>;
}

/// Load, check, and run a file. Optionally set compilation mode.
pub struct PathRunner;

impl PathRunner {
    pub fn mode(self, mode: Mode) -> Self;
    pub fn run(self) -> Result<Value, EvalError>;
}

/// Compile and run a source string. Optionally set compilation mode.
pub struct SourceRunner;

impl SourceRunner {
    pub fn mode(self, mode: Mode) -> Self;
    pub fn run(self) -> Result<Value, EvalError>;
}

/// A test run: tests discovered, ready to execute.
pub struct TestRunner {
    pub tests: Vec<QualifiedPath>,
}

impl TestRunner {
    pub fn run(self) -> Result<TestReport, EvalError>;
    pub fn execute(self, on_result: impl FnMut(&TestResult)) -> Result<TestReport, EvalError>;
}

/// Summary of all test results.
pub struct TestReport {
    pub results: Vec<TestResult>,
}

impl TestReport {
    pub fn passed(&self) -> usize;
    pub fn failed(&self) -> usize;
    pub fn total(&self) -> usize;
    pub fn is_success(&self) -> bool;
}

/// Convenience: compile and run a source string.
pub fn run_source(source: &str) -> Result<Value, EvalError>;

/// Convenience: load, check, and run a file.
pub fn run_path(path: &Path) -> Result<Value, EvalError>;
```

## Value Types

Runtime values are defined in the [zoya-value](../zoya-value) crate and re-exported from `zoya-run`:

```rust
use zoya_run::{Value, ValueData};

let result = run_source("pub fn main() -> Int { 42 }")?;
assert_eq!(result, Value::Int(42));

// JSON serialization
println!("{}", result.to_json()); // "42"
```

See [zoya-value](../zoya-value) for full `Value`, `ValueData`, and `JSValue` documentation.

## Error Handling

```rust
pub enum EvalError {
    Panic(String),
    RuntimeError(String),
}
```

## Dependencies

- [zoya-check](../zoya-check) - Type checker
- [zoya-codegen](../zoya-codegen) - JavaScript code generation
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-loader](../zoya-loader) - Package file loading
- [zoya-package](../zoya-package) - Package data structures
- [zoya-std](../zoya-std) - Standard library
- [zoya-value](../zoya-value) - Runtime value types and serialization
- [rquickjs](https://github.com/aspect-build/rquickjs) - JavaScript runtime (QuickJS)
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
