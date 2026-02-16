# Zoya Development Guide

Strongly-typed functional language compiling to JavaScript. See [README.md](README.md) for language documentation.

## Documentation

**IMPORTANT: Always consult the spec and docs when working on language features.**

- **Specification** (`docs/src/specification/`): Formal language spec — grammar, type system, expressions, definitions, modules. Read these to understand exact semantics and grammar rules before implementing or modifying language features.
- **Language Guide** (`docs/src/language/`): User-facing tutorials and examples.
- **Reference** (`docs/src/reference/`): CLI and naming conventions.

When adding or changing language features, **update the spec and docs alongside the code**. Documentation should stay in sync with the implementation.

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
│           ├── new.rs     # New project command
│           ├── repl.rs    # REPL (rustyline)
│           ├── run.rs     # Run command
│           └── test.rs    # Test command
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
├── zoya-run/          # Runtime execution (rquickjs)
│   └── src/
│       ├── lib.rs         # Public API
│       ├── eval.rs        # JS execution
│       └── runner.rs      # Run functions
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
            ├── io.zy          # IO operations
            └── json.zy        # JSON type and methods
└── zoya-value/        # Runtime value types & serialization
    └── src/
        └── lib.rs         # Value, JSValue, serde support
```

## Commands

```bash
cargo run -p zoya -- repl             # REPL
cargo run -p zoya -- run file.zy    # Run file
cargo run -p zoya -- run              # Run package in current directory
cargo run -p zoya -- run --json       # Output result as JSON
cargo run -p zoya -- check file.zy  # Type-check only
cargo run -p zoya -- build file.zy  # Compile to JS
cargo run -p zoya -- fmt              # Format current package
cargo run -p zoya -- fmt --check      # Check formatting
cargo run -p zoya -- test             # Run tests
cargo run -p zoya -- new my_project   # Create new project
cargo test --workspace                # Run all Rust tests
cargo clippy --workspace              # Lint
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
| `zoya-run` | End-to-end execution |
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
