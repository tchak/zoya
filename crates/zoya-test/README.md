# zoya-test

Test runner for the Zoya programming language.

Discovers and executes `#[test]` functions from a built package, collecting results into a structured report.

## Features

- **Test discovery** - Finds all `#[test]` functions from `BuildOutput`
- **Structured results** - Each test produces a `TestResult` with pass/fail outcome
- **Aggregate reporting** - `TestReport` provides counts and overall success status
- **Progress callbacks** - `execute()` method calls a hook after each test completes
- **Job collection** - Captures any jobs enqueued during test execution

## Usage

### Run all tests

```rust
use zoya_build::build_from_path;
use zoya_loader::Mode;
use zoya_test::{TestRunner, TestReport};
use std::path::Path;

// Build the package in test mode
let output = build_from_path(Path::new("my_project"), Mode::Test)?;

// Run all tests
let runner = TestRunner::new(&output);
let report = runner.run()?;

println!("{} passed, {} failed", report.passed(), report.failed());
assert!(report.is_success());
```

### Run with progress reporting

```rust
use zoya_test::{TestRunner, TestResult};

let runner = TestRunner::new(&output);
let report = runner.execute(|result: &TestResult| {
    match &result.outcome {
        Ok(()) => println!("  PASS {}", result.path),
        Err(e) => println!("  FAIL {}: {}", result.path, e),
    }
})?;
```

## Public API

```rust
/// Structured error for test failures.
pub enum TestError {
    Panic(String),
    RuntimeError(String),
    Failed(String),
    UnexpectedReturn(String),
}

/// A single test result.
pub struct TestResult {
    pub path: QualifiedPath,
    pub outcome: Result<(), TestError>,
    pub jobs: Vec<Job>,
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

/// Test runner: discovers and executes tests.
pub struct TestRunner<'a> {
    pub tests: Vec<QualifiedPath>,
    // ...
}

impl<'a> TestRunner<'a> {
    pub fn new(output: &'a BuildOutput) -> Self;
    pub fn run(self) -> Result<TestReport, EvalError>;
    pub fn execute(self, on_result: impl FnMut(&TestResult)) -> Result<TestReport, EvalError>;
}
```

## Error Handling

`TestError` covers the four failure modes for individual tests:

| Variant | Description |
|---------|-------------|
| `Panic` | The test called `panic()` |
| `RuntimeError` | JavaScript runtime error during execution |
| `Failed` | Assertion failure (e.g., `assert_eq` mismatch) |
| `UnexpectedReturn` | Test returned an unexpected value |

The runner itself can return `EvalError` for infrastructure failures (e.g., runtime creation).

## Dependencies

- [zoya-build](../zoya-build) - Build pipeline
- [zoya-package](../zoya-package) - Package data structures
- [zoya-run](../zoya-run) - Runtime execution
- [zoya-value](../zoya-value) - Runtime value types
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
