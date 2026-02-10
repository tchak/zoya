# zoya-run

Runtime execution for the Zoya programming language.

Provides functions to run Zoya programs by compiling to JavaScript and executing via QuickJS.

## Features

- **Package execution** - Run type-checked packages with module support
- **Source execution** - Compile and run source strings directly
- **File execution** - Load, check, and run `.zoya` files
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

let result = run_file(Path::new("program.zoya"))?;
println!("Result: {}", result);
```

### Run a checked package

```rust
use zoya_check::check;
use zoya_loader::load_package;
use zoya_run::run;
use std::path::Path;

// Load and type-check
let pkg = load_package(Path::new("src/main.zoya"))?;
let checked_pkg = check(&pkg)?;

// Run the main function in the root module
let result = run(checked_pkg, None, None)?;
println!("Result: {}", result);
```

### Run a specific module's main function

```rust
use zoya_run::run;

// Run main() from the "utils" submodule
let result = run(checked_pkg, Some("utils"), None)?;
```

## Public API

```rust
/// Run a checked package by executing its main function
pub fn run(
    package: CheckedPackage,
    module: Option<&str>,          // None = root module, Some("repl") = repl submodule
    return_type: Option<Type>,     // None = infer from main signature
) -> Result<Value, EvalError>;

/// Load, check, and run source code from a string
pub fn run_source(source: &str) -> Result<Value, EvalError>;

/// Load, check, and run source code from a file
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
- [rquickjs](https://github.com/DelSkaorth/rquickjs) - JavaScript runtime (QuickJS)
