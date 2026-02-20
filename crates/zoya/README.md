# zoya

The Zoya programming language CLI.

This is the main binary crate that provides the `zoya` CLI tool for compiling and running Zoya programs.

## Commands

```bash
zoya init my_project                # Create a new project
zoya repl                          # Start interactive REPL
zoya repl file.zy                # REPL with file preloaded
zoya run file.zy                 # Execute a file
zoya run                           # Run package in current directory
zoya run path/to/project           # Run package at path
zoya run --mode test               # Run in test mode
zoya run --json file.zy          # Output result as JSON
zoya check file.zy               # Type-check without running
zoya check                         # Check package in current directory
zoya build file.zy               # Compile to JavaScript (stdout)
zoya build file.zy -o out.js     # Compile to file
zoya fmt                           # Format source files in current directory
zoya fmt file.zy                 # Format a single file
zoya fmt --check                   # Check formatting without writing
zoya test                          # Run tests in current package
zoya test path/to/project          # Run tests at path
```

## Init Project

Create a new Zoya project with `zoya init`:

```bash
zoya init my_project
zoya init my_project --name custom_name
```

This creates a directory with `package.toml` and `src/main.zy`.

## REPL Features

- Persistent function and type definitions across inputs
- Let bindings accumulate in scope
- Load files with `zoya repl file.zy`
- Expression evaluation with immediate results

```
> fn double(x: Int) -> Int { x * 2 }
defined: double
> double(21)
42
> let nums = [1, 2, 3]
let nums: List<Int>
> nums.len()
3
```

## Compilation Modes

The `run`, `check`, and `build` commands support a `--mode` flag:

| Mode | Description |
|------|-------------|
| `dev` | Development mode — excludes `#[test]` items (default) |
| `test` | Test mode — includes all items including `#[test]` |
| `release` | Release mode — excludes `#[test]` items |

```bash
zoya run --mode test       # Run with test items included
zoya check --mode release  # Check in release mode
```

## Programmatic Usage

For programmatic execution, use the [zoya-run](../zoya-run) crate:

```rust
use zoya_run::{Runner, run_source, run_path, Value};
use std::path::Path;

// Run from source string
let result = run_source("pub fn main() -> Int { 42 }")?;
assert_eq!(result, Value::Int(42));

// Run from file
let result = run_path(Path::new("program.zy"))?;
println!("Result: {}", result);

// Builder with options
let result = Runner::new()
    .path(Path::new("program.zy"))
    .mode(zoya_loader::Mode::Test)
    .run()?;

// Run tests
let test_runner = Runner::new().test(Path::new("src/main.zy"))?;
let report = test_runner.run()?;
println!("{} passed, {} failed", report.passed(), report.failed());
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-check](../zoya-check) - Type checker
- [zoya-codegen](../zoya-codegen) - JavaScript code generation
- [zoya-fmt](../zoya-fmt) - Source code formatter
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-loader](../zoya-loader) - Package file loading
- [zoya-package](../zoya-package) - Package data structures and config
- [zoya-parser](../zoya-parser) - Parser
- [zoya-run](../zoya-run) - Runtime execution
- [zoya-std](../zoya-std) - Standard library
- [zoya-value](../zoya-value) - Runtime value types
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [console](https://github.com/console-rs/console) - Terminal styling and colors
- [rustyline](https://github.com/kkawakam/rustyline) - REPL line editing
