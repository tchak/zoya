# zoya

The Zoya programming language CLI.

This is the main binary crate that provides the `zoya` CLI tool for compiling and running Zoya programs.

## Commands

```bash
zoya init my_project                # Create a new project
zoya repl                           # Start interactive REPL
zoya repl -p file.zy               # REPL with package loaded
zoya run                            # Run main in current directory
zoya run -p file.zy                # Run main from a file
zoya run -p path/to/project        # Run main from a project
zoya run --mode test                # Run in test mode
zoya run --json                     # Output result as JSON
zoya run add 1 2                    # Run a named function with arguments
zoya run --json add 1 2            # Run named function, output as JSON
zoya check                          # Type-check package in current directory
zoya check -p file.zy              # Type-check a file
zoya build                          # Compile to JavaScript (stdout)
zoya build -p file.zy              # Compile a file
zoya build -p file.zy -o out.js   # Compile to file
zoya fmt                            # Format source files in current directory
zoya fmt -p file.zy               # Format a single file
zoya fmt --check                    # Check formatting without writing
zoya test                           # Run tests in current package
zoya test -p path/to/project       # Run tests at path
zoya dev                            # Start HTTP dev server (port 3000)
zoya dev --port 8080                # Dev server on custom port
zoya job list                       # List available job functions
zoya job run deploy                 # Run a job function
zoya job run deploy arg1            # Run job with arguments
```

## Init Project

Create a new Zoya project with `zoya init`:

```bash
zoya init my_project
zoya init my_project -n custom_name
```

This creates a directory with `package.toml` and `src/main.zy`.

## REPL Features

- Persistent function and type definitions across inputs
- Let bindings accumulate in scope
- Load a package with `zoya repl -p file.zy`
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

## Job Functions

Define functions with `#[job]` and run them from the CLI:

```bash
zoya job list               # List all #[job] functions
zoya job run deploy         # Run a job function
```

Job functions can accept typed arguments parsed from the command line.

## Named Function Execution

Run any public function by name, with type-guided argument parsing:

```bash
zoya run add 1 2             # Calls add(1, 2) with Int arguments
zoya run greet               # Calls greet() with no arguments
zoya run --json add 1 2     # Output result as JSON
```

Arguments are parsed according to the function's parameter types.

## Programmatic Usage

For programmatic execution, use the [zoya-run](../zoya-run) crate. See its README for details.

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
- [zoya-build](../zoya-build) - Build orchestration
- [zoya-check](../zoya-check) - Type checker
- [zoya-codegen](../zoya-codegen) - JavaScript code generation
- [zoya-dashboard](../zoya-dashboard) - Dev dashboard SPA
- [zoya-fmt](../zoya-fmt) - Source code formatter
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-job](../zoya-job) - Background job processing
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-loader](../zoya-loader) - Package file loading
- [zoya-package](../zoya-package) - Package data structures and config
- [zoya-parser](../zoya-parser) - Parser
- [zoya-router](../zoya-router) - HTTP router for dev server
- [zoya-run](../zoya-run) - Runtime execution
- [zoya-std](../zoya-std) - Standard library
- [zoya-test](../zoya-test) - Test runner
- [zoya-value](../zoya-value) - Runtime value types
- [axum](https://github.com/tokio-rs/axum) - HTTP framework (dev server)
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [console](https://github.com/console-rs/console) - Terminal styling and colors
- [notify](https://github.com/notify-rs/notify) - File watching (dev server)
- [rustyline](https://github.com/kkawakam/rustyline) - REPL line editing
- [tokio](https://github.com/tokio-rs/tokio) - Async runtime (dev server)
- [anyhow](https://github.com/dtolnay/anyhow) - Error handling at CLI boundary
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
