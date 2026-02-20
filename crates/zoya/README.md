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
zoya run -- add 1 2                # Run a named function with arguments
zoya check file.zy               # Type-check without running
zoya check                         # Check package in current directory
zoya build file.zy               # Compile to JavaScript (stdout)
zoya build file.zy -o out.js     # Compile to file
zoya fmt                           # Format source files in current directory
zoya fmt file.zy                 # Format a single file
zoya fmt --check                   # Check formatting without writing
zoya test                          # Run tests in current package
zoya test path/to/project          # Run tests at path
zoya dev                           # Start HTTP dev server (port 3000)
zoya dev --port 8080               # Dev server on custom port
zoya task list                     # List available task functions
zoya task run deploy               # Run a task function
zoya task run deploy -- arg1       # Run task with arguments
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

## Development Server

Start an HTTP development server with file watching and hot-reload:

```bash
zoya dev                     # Start on default port 3000
zoya dev --port 8080         # Custom port
```

Functions annotated with HTTP method attributes (`#[get("/path")]`, `#[post("/path")]`, etc.) become routes. The server automatically rebuilds when `.zy` files change, continuing to serve the last successful build on errors.

## Task Functions

Define functions with `#[task]` and run them from the CLI:

```bash
zoya task list               # List all #[task] functions
zoya task run deploy         # Run a task function
```

Task functions can accept typed arguments parsed from the command line.

## Named Function Execution

Run any public function by name, with type-guided argument parsing:

```bash
zoya run -- add 1 2          # Calls add(1, 2) with Int arguments
zoya run -- greet             # Calls greet() with no arguments
zoya run --json -- add 1 2   # Output result as JSON
```

Arguments are parsed according to the function's parameter types.

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

## Error Handling

CLI commands return `anyhow::Result<()>` for ergonomic error propagation. The `init` command has its own structured error type:

```rust
pub enum InitError {
    AlreadyExists(PathBuf),
    InvalidPath(PathBuf),
    InvalidName(String),
    Io { path: PathBuf, source: std::io::Error },
}
```

All errors are reported to stderr via `fatal()` which prints `"error: <message>"` in red/bold using the `console` crate.

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
- [zoya-router](../zoya-router) - HTTP router for dev server
- [zoya-run](../zoya-run) - Runtime execution
- [zoya-std](../zoya-std) - Standard library
- [axum](https://github.com/tokio-rs/axum) - HTTP framework (dev server)
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [console](https://github.com/console-rs/console) - Terminal styling and colors
- [notify](https://github.com/notify-rs/notify) - File watching (dev server)
- [rustyline](https://github.com/kkawakam/rustyline) - REPL line editing
- [tokio](https://github.com/tokio-rs/tokio) - Async runtime (dev server)
- [anyhow](https://github.com/dtolnay/anyhow) - Error handling at CLI boundary
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
