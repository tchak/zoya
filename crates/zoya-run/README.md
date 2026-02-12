# zoya-run

Runtime execution for the Zoya programming language.

Provides a builder-pattern API to run Zoya programs by compiling to JavaScript and executing via QuickJS.

## Features

- **Builder pattern** - Composable, type-safe run configuration with compile-time guarantees
- **Package execution** - Run type-checked packages with module and dependency support
- **Source execution** - Compile and run source strings directly
- **File execution** - Load, check, and run `.zy` files
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
use zoya_loader::load_package;
use zoya_run::Runner;
use zoya_std::std;
use std::path::Path;

// Load and type-check with standard library
let std = std();
let pkg = load_package(Path::new("src/main.zy"))?;
let checked_pkg = check(&pkg, &[std])?;

// Run the main function in the root module
let result = Runner::new()
    .package(checked_pkg, [std])
    .run()?;
println!("Result: {}", result);
```

### Run a specific module's main function

```rust
use zoya_run::Runner;

// Run main() from the "repl" submodule
let result = Runner::new()
    .package(checked_pkg, [std])
    .module("repl")
    .run()?;
```

## Public API

```rust
/// Entry point — choose an input source.
pub struct Runner;

impl Runner {
    pub fn new() -> Self;
    pub fn package(self, pkg: CheckedPackage, deps: impl IntoIterator<Item = &CheckedPackage>) -> PackageRunner;
    pub fn path(self, path: &Path) -> PathRunner;
    pub fn source(self, source: &str) -> SourceRunner;
}

/// Run a pre-checked package. Optionally select a submodule.
pub struct PackageRunner<'a>;

impl PackageRunner<'_> {
    pub fn module(self, module: impl Into<String>) -> Self;
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

/// Convenience: compile and run a source string.
pub fn run_source(source: &str) -> Result<Value, EvalError>;

/// Convenience: load, check, and run a file.
pub fn run_path(path: &Path) -> Result<Value, EvalError>;
```

## Value Types

The `Value` enum represents Zoya runtime values:

```rust
pub enum Value {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Struct { name: String, fields: Vec<(String, Value)> },
    Fn { params: Vec<Type>, ret: Box<Type> },
    Enum { enum_name: String, variant_name: String, fields: EnumValueFields },
}

pub enum EnumValueFields {
    Unit,
    Tuple(Vec<Value>),
    Struct(Vec<(String, Value)>),
}
```

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
- [rquickjs](https://github.com/aspect-build/rquickjs) - JavaScript runtime (QuickJS)
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
