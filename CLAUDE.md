# Zoya Development Guide

Strongly-typed functional language compiling to JavaScript. See [README.md](README.md) for language documentation.

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
│       ├── eval.rs        # JS execution (rquickjs)
│       └── runner.rs      # File runner
│       └── commands/
│           ├── build.rs   # Build command
│           ├── check.rs   # Check command
│           ├── repl.rs    # REPL (rustyline)
│           └── run.rs     # Run command
├── zoya-ast/          # Untyped AST types
├── zoya-check/        # Type checker (Hindley-Milner)
├── zoya-codegen/      # JavaScript code generation
├── zoya-ir/           # Typed IR and type definitions
├── zoya-lexer/        # Tokenizer (logos)
├── zoya-loader/       # Package file loading
├── zoya-package/      # Package data structures
└── zoya-parser/       # Parser (chumsky)
```

## Commands

```bash
cargo run -p zoya -- repl             # REPL
cargo run -p zoya -- run file.zoya    # Run file
cargo run -p zoya -- check file.zoya  # Type-check only
cargo run -p zoya -- build file.zoya  # Compile to JS
cargo test --workspace                # Run all tests
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

## Testing

New features need tests at each pipeline stage:

| Crate | Tests |
|-------|-------|
| `zoya-lexer` | Token recognition |
| `zoya-parser` | AST structure |
| `zoya-package` | Module path operations |
| `zoya-loader` | Package loading and resolution |
| `zoya-check` | Type inference and errors |
| `zoya-codegen` | Generated JS correctness |
| `zoya` (runner) | End-to-end execution |

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
