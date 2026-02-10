# zoya-run

Runtime execution for the Zoya programming language.

Provides functions to run Zoya programs by compiling to JavaScript and executing via QuickJS.

## Features

- **Package execution** - Run type-checked packages with module and dependency support
- **Source execution** - Compile and run source strings directly
- **File execution** - Load, check, and run `.zy` files
- **Value marshaling** - Convert JavaScript results to typed Zoya values

## Usage

### Run from source string

```rust
use zoya_run::{run_source, Value};

let result = run_source("fn main() -> Int { 42 }")?;
assert_eq!(result, Value::Int(42));
```

### Run from file

```rust
use zoya_run::run_file;
use std::path::Path;

let result = run_file(Path::new("program.zy"))?;
println!("Result: {}", result);
```

### Run a checked package

```rust
use zoya_check::check;
use zoya_loader::load_package;
use zoya_run::run;
use zoya_std::std;
use std::path::Path;

// Load and type-check with standard library
let std = std();
let pkg = load_package(Path::new("src/main.zy"))?;
let checked_pkg = check(&pkg, &[std])?;

// Run the main function in the root module
let result = run(checked_pkg, &[std], None)?;
println!("Result: {}", result);
```

### Run a specific module's main function

```rust
use zoya_run::run;

// Run main() from the "repl" submodule
let result = run(checked_pkg, &[std], Some("repl"))?;
```

## Public API

```rust
/// Run a checked package by executing its main function.
/// `deps` provides dependency packages (e.g., standard library) for codegen.
/// `module` selects which module's main() to call (None = root).
pub fn run(
    package: CheckedPackage,
    deps: &[&CheckedPackage],
    module: Option<&str>,
) -> Result<Value, EvalError>;

/// Load, check, and run source code from a string.
/// Automatically includes the standard library.
pub fn run_source(source: &str) -> Result<Value, EvalError>;

/// Load, check, and run source code from a file.
/// Automatically includes the standard library.
pub fn run_file(path: &Path) -> Result<Value, EvalError>;
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
    DivisionByZero,
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
