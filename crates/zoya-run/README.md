# zoya-run

Runtime execution for the Zoya programming language.

Provides a builder-pattern API to run Zoya programs by compiling to JavaScript and executing via QuickJS.

## Features

- **Builder pattern** - Composable, type-safe run configuration
- **Package execution** - Run type-checked packages with module and dependency support
- **Custom entry points** - Run any function by path with arguments
- **Value marshaling** - Convert JavaScript results to typed Zoya values

## Usage

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

## Public API

```rust
/// Entry point — choose an input source.
pub struct Runner;

impl Runner {
    pub fn new() -> Self;
    pub fn package<'a>(self, pkg: &'a CheckedPackage, deps: impl IntoIterator<Item = &'a CheckedPackage>) -> PackageRunner<'a>;
}

/// Run a pre-checked package. Optionally select a submodule.
pub struct PackageRunner<'a>;

impl PackageRunner<'_> {
    pub fn module(self, module: impl Into<String>) -> Self;
    pub fn entry(self, path: QualifiedPath, args: Vec<Value>) -> Self;
    pub fn run(self) -> Result<Value, EvalError>;
}

```

## Value Types

Runtime values are defined in the [zoya-value](../zoya-value) crate and re-exported from `zoya-run`:

```rust
use zoya_run::{Value, ValueData};

// JSON serialization
println!("{}", result.to_json()); // "42"
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

- [zoya-codegen](../zoya-codegen) - JavaScript code generation
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-loader](../zoya-loader) - Package file loading
- [zoya-package](../zoya-package) - Package data structures
- [zoya-value](../zoya-value) - Runtime value types and serialization
- [rquickjs](https://github.com/aspect-build/rquickjs) - JavaScript runtime (QuickJS)
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
