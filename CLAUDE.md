# Zoya Development Guide

Strongly-typed functional language compiling to JavaScript. See [README.md](README.md) for language documentation.

## Documentation

**IMPORTANT: Always consult the spec and docs when working on language features.**

- **Specification** (`docs/src/specification/`): Formal language spec — grammar, type system, expressions, definitions, modules. Read these to understand exact semantics and grammar rules before implementing or modifying language features.
- **Language Guide** (`docs/src/language/`): User-facing tutorials and examples.
- **Reference** (`docs/src/reference/`): CLI and naming conventions.

When adding or changing language features, **update the spec and docs alongside the code**. Documentation should stay in sync with the implementation.

## Tree-sitter Grammar

**IMPORTANT: When modifying the lexer or parser (adding/changing tokens, syntax, or grammar rules), always review and update the tree-sitter grammar to match.**

The tree-sitter grammar lives in `editors/tree-sitter-zoya/`. The Zed extension in `editors/zed-zoya/` wraps it.

```bash
cd editors/tree-sitter-zoya && npx tree-sitter generate   # Regenerate parser from grammar.js
cd editors/tree-sitter-zoya && npx tree-sitter test        # Run grammar test corpus
cd editors/tree-sitter-zoya && npx tree-sitter parse FILE  # Parse a .zy file and print tree
```

Key files:
- `editors/tree-sitter-zoya/grammar.js` — Grammar definition
- `editors/tree-sitter-zoya/src/scanner.c` — External scanner (interpolated strings)
- `editors/tree-sitter-zoya/test/corpus/` — Test corpus (63 tests)
- `editors/zed-zoya/languages/zoya/highlights.scm` — Syntax highlighting queries
- `editors/zed-zoya/languages/zoya/locals.scm` — Variable scoping queries

## Architecture

```
Source → Lexer → Parser → Type Checker → Typed IR → Codegen → JavaScript
```

### Workspace Structure

```
crates/
├── zoya/              # Main compiler & CLI
│   └── src/
│       ├── main.rs        # CLI (clap)
│       └── commands/
│           ├── build.rs   # Build command
│           ├── check.rs   # Check command
│           ├── fmt.rs     # Format command
│           ├── init.rs    # Init project command
│           ├── repl.rs    # REPL (rustyline)
│           ├── run.rs     # Run command
│           ├── test.rs    # Test command
│           └── dev.rs     # Dev server command
├── zoya-ast/          # Untyped AST types
├── zoya-check/        # Type checker (Hindley-Milner)
├── zoya-codegen/      # JavaScript code generation
├── zoya-fmt/          # Source code formatter (pretty)
├── zoya-ir/           # Typed IR and type definitions
├── zoya-lexer/        # Tokenizer (logos)
├── zoya-loader/       # Package file loading
├── zoya-naming/       # Naming conventions & validation
├── zoya-package/      # Package data structures & config
├── zoya-parser/       # Parser (chumsky)
├── zoya-router/       # HTTP router (Axum integration)
├── zoya-run/          # Runtime execution (rquickjs)
│   └── src/
│       ├── lib.rs         # Public API
│       ├── eval.rs        # JS execution
│       └── runner.rs      # Run functions
├── zoya-test/         # Test runner (TestRunner, TestReport)
├── zoya-std/          # Standard library
    └── src/
        ├── lib.rs         # Loads and caches std package
        └── std/           # Zoya source files
            ├── main.zy        # Entry point, panic, assert
            ├── prelude.zy     # Re-exports for auto-injection
            ├── option.zy      # Option<T> type and methods
            ├── result.zy      # Result<T, E> type and methods
            ├── int.zy         # Int methods
            ├── float.zy       # Float methods
            ├── bigint.zy      # BigInt methods
            ├── string.zy      # String methods
            ├── list.zy        # List<T> methods
            ├── dict.zy        # Dict<K, V> methods
            ├── set.zy         # Set<T> methods
            ├── io.zy          # IO operations
            ├── json.zy        # JSON type and methods
            └── http.zy        # HTTP Request/Response types
└── zoya-value/        # Runtime value types & serialization
    └── src/
        └── lib.rs         # Value, JSValue, serde support
examples/
├── tests/             # Native Zoya integration tests (language features)
├── std-tests/         # Native Zoya integration tests (std library methods)
└── algorithms/        # Example algorithm implementations
editors/
├── tree-sitter-zoya/  # Tree-sitter grammar
│   ├── grammar.js         # Grammar definition
│   ├── src/scanner.c      # External scanner (interpolated strings)
│   └── test/corpus/       # Test corpus
└── zed-zoya/          # Zed editor extension
    ├── extension.toml     # Extension manifest
    └── languages/zoya/    # Highlighting & config
packages/
└── zoya-runtime/      # JS runtime (TypeScript, bundled with tsdown)
    ├── src/               # TypeScript source modules
    ├── tests/             # Vitest tests
    └── dist/              # Built bundle (committed, used by codegen)
```

## Zoya Syntax Quick Reference

**Items are only allowed at the module top level** — you cannot define `fn`, `struct`, `enum`, `type`, `impl`, `mod`, or `use` inside expressions or blocks. Blocks (`{ ... }`) contain only `let` bindings and a final expression.

```zoya
// --- Top-level items ---
mod utils                              // module declaration (file-based)
pub mod utils                          // public module

use self::utils::add                   // import item
use self::utils::{add, mul}            // group import
use self::utils::*                     // glob import

type UserId = Int                      // type alias
type Pair<A, B> = (A, B)              // generic type alias

struct Point { x: Int, y: Int }        // named struct
struct Pair<T, U> { first: T, second: U }  // generic struct
struct Wrapper(Int)                    // tuple struct
struct UnitStruct                      // unit struct

enum Option<T> { None, Some(T) }      // enum with unit + tuple variants
enum Message { Quit, Move { x: Int, y: Int }, Write(String) }  // mixed variants

impl Point {                           // impl block
  fn sum(self) -> Int { self.x + self.y }         // method (has self)
  fn origin() -> Self { Point { x: 0, y: 0 } }   // associated function (no self)
}
impl<T> Wrapper<T> { ... }            // generic impl block

fn add(x: Int, y: Int) -> Int { x + y }           // function with block body
fn square(x: Int) -> Int x * x                     // function with expression body
pub fn helper() -> Int 42                           // public function
fn identity<T>(x: T) -> T x                        // generic function

#[test]
fn test_add() -> () assert_eq(add(1, 2), 3)       // test function

// --- Expressions ---
let x = 42;                            // let binding (SEMICOLON required, not a comma)
let y: Int = 42;                       // let with type annotation
let (a, b) = (1, 2);                  // destructuring let
let Point { x, .. } = p;             // struct destructuring let

// Lambdas use |params| body syntax (NOT fn keyword)
let f = |x| x + 1;                    // lambda, inferred types
let g = |x: Int| -> Int x * 2;        // lambda with type annotations
let add = |x, y| x + y;              // multi-param lambda
let thunk = || 42;                     // no-param lambda
let nested = |x| |y| x + y;          // curried lambda
let f: Int -> Int = |x| x * 2;        // function type annotation
let add: (Int, Int) -> Int = |x, y| x + y;

// Blocks: let bindings + final expression (the result)
{
  let a = 10;
  let b = 20;
  a + b                                // last expression is the block's value
}

// Match (used for conditionals — there is NO if/else)
match x {
  0 => "zero",
  n => "other",
}

// Method calls and field access
point.x                                // field access
list[0]                                // index access
"hello".to_uppercase()                 // method call
Builder::new().add(10).build()         // chaining

// Enum construction
Option::Some(42)
Option::None
Option::None::<Int>                    // turbofish

// Struct construction
Point { x: 10, y: 20 }
Point { x: 10, ..other }              // spread
Point { x, y }                        // field shorthand
```

## Error Handling

All crates use [`thiserror`](https://github.com/dtolnay/thiserror) for structured error enums. The CLI boundary uses [`anyhow`](https://github.com/dtolnay/anyhow). See [ERRORS.md](ERRORS.md) for full details.

### Error Types

| Crate | Error Type | Description |
|-------|-----------|-------------|
| `zoya-lexer` | `LexError` | Unexpected characters with byte spans |
| `zoya-parser` | `ParseError` | Syntax errors with spans, expected/found tokens |
| `zoya-package` | `ConfigError` | TOML config loading (IO, parse, validation) |
| `zoya-ir` | `TypeError` | 30+ structured variants (type mismatch, unbound, arity, visibility, exhaustiveness, etc.) |
| `zoya-loader` | `LoaderError<P>` | Module loading — embeds `LexError`/`ParseError` as `#[source]` |
| `zoya-value` | `Error` | Runtime value conversion errors |
| `zoya-run` | `EvalError` | Runtime execution: `Panic`, `RuntimeError`, `LoadError`, `TypeError` |
| `zoya-test` | `TestError` | Per-test errors: `Panic`, `RuntimeError`, `Failed`, `UnexpectedReturn` |
| `zoya-std` | `StdError` | Std library loading — `#[from]` for `LoaderError` and `TypeError` |
| `zoya` | `InitError` | Project creation errors |

### Conventions

- Use `thiserror::Error` derive for all error enums
- Embed upstream errors as `#[source]` fields where possible (not `.to_string()`)
- CLI commands return `anyhow::Result<()>`
- `StdError` demonstrates the ideal pattern with `#[from]` auto-conversion

## Commands

```bash
cargo run -p zoya -- repl                        # REPL
cargo run -p zoya -- run --package file.zy       # Run file
cargo run -p zoya -- run                         # Run package in current directory
cargo run -p zoya -- run --json                  # Output result as JSON
cargo run -p zoya -- check --package file.zy     # Type-check only
cargo run -p zoya -- build --package file.zy     # Compile to JS
cargo run -p zoya -- fmt                         # Format current package
cargo run -p zoya -- fmt --check                 # Check formatting
cargo run -p zoya -- test                        # Run tests
cargo run -p zoya -- init my_project              # Create new project
cargo run -p zoya -- dev                          # Start dev HTTP server
cargo run -p zoya -- dev --port 8080              # Dev server on custom port
cargo run -p zoya -- task list                    # List task functions
cargo run -p zoya -- task run deploy              # Run a task function
cargo test --workspace                           # Run all Rust tests
cargo clippy --workspace                         # Lint
cd packages/zoya-runtime && npm run build        # Build JS runtime bundle
cd packages/zoya-runtime && npm test             # Run JS runtime tests
cd packages/zoya-runtime && npm run typecheck    # Type-check JS runtime
cd editors/tree-sitter-zoya && npx tree-sitter generate  # Regenerate grammar
cd editors/tree-sitter-zoya && npx tree-sitter test      # Run grammar tests
cd editors/tree-sitter-zoya && npx tree-sitter parse FILE # Parse a .zy file
cd editors/tree-sitter-zoya && npm run parse-all         # Parse all std & example files
```

## Version Control

**IMPORTANT: ALWAYS use jj (Jujutsu). NEVER use git commands directly.**

This project uses jj as its version control interface. Do not use `git add`, `git commit`, `git status`, `git diff`, `git log`, or any other git commands. Always use the jj equivalents:

| Instead of | Use |
|------------|-----|
| `git status` | `jj status` |
| `git diff` | `jj diff` |
| `git add && git commit` | `jj commit -m "..."` |
| `git log` | `jj log` |
| `git push` | `jj git push` |

```bash
jj status
jj diff
jj commit -m "<type>: <description>"
jj log
```

### Commit Format

Conventional Commits: `<type>[scope]: <description>`

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`

```bash
jj commit -m "feat(parser): add tuple patterns"
jj commit -m "fix: resolve unification with recursive types"
```

## Formatting

**IMPORTANT: Always run `cargo fmt` before committing.** All code must be formatted with `rustfmt`. Run `cargo fmt --check` to verify.

## Testing

**Prefer native Zoya tests for integration testing.** When testing language features end-to-end, write tests as `#[test]` functions in `examples/tests/` (language features) or `examples/std-tests/` (std library methods) rather than Rust-side `runner_tests`. Run them with `cargo run -p zoya -- test --package examples/tests` or `cargo run -p zoya -- test --package examples/std-tests`. Reserve Rust integration tests for cases that need to inspect compiler internals (error messages, types, codegen output).

New features need tests at each pipeline stage:

| Crate | Tests |
|-------|-------|
| `zoya-lexer` | Token recognition |
| `zoya-parser` | AST structure |
| `zoya-package` | Module path operations, package config |
| `zoya-loader` | Package loading and resolution |
| `zoya-naming` | Name validation, case conversion |
| `zoya-check` | Type inference, visibility, and errors |
| `zoya-codegen` | Generated JS correctness |
| `zoya-fmt` | Source code formatting |
| `zoya-router` | HTTP routing and handler execution |
| `zoya-run` | End-to-end execution |
| `zoya-test` | Test runner and reporting |
| `zoya-std` | Standard library loading and caching |
| `zoya-value` | Value types, serialization, JS bridge |
| `zoya` | CLI commands, REPL, and project creation |

```bash
cargo test --workspace              # All tests
cargo test -p zoya-parser           # Single crate
cargo test -p zoya-check            # Type checker tests
cargo test -- --nocapture
```

## Coverage

```bash
cargo llvm-cov --workspace          # Summary report
cargo llvm-cov --workspace --html   # HTML report (target/llvm-cov/html/)
cargo llvm-cov --workspace --open   # Generate and open HTML report
```
