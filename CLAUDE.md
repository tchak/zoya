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
│       ├── check.rs       # Type checker → TypedExpr
│       ├── unify.rs       # Type unification (Union-Find)
│       ├── usefulness.rs  # Pattern exhaustiveness (Maranget)
│       ├── ir.rs          # Typed IR
│       ├── types.rs       # Type definitions
│       ├── codegen.rs     # JavaScript generation
│       ├── eval.rs        # JS execution (rquickjs)
│       ├── repl.rs        # REPL (rustyline)
│       └── runner.rs      # File runner
├── zoya-ast/          # Untyped AST types
├── zoya-lexer/        # Tokenizer (logos)
└── zoya-parser/       # Parser (chumsky)
```

## Commands

```bash
cargo run -p zoya -- run              # REPL
cargo run -p zoya -- run file.zoya    # Run file
cargo run -p zoya -- check file.zoya  # Type-check only
cargo run -p zoya -- build file.zoya  # Compile to JS
cargo test --workspace                # Run all tests
cargo clippy --workspace              # Lint
```

## Version Control

Uses **jj (Jujutsu)**, not git directly.

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
| `zoya` (check) | Type checking pass/fail |
| `zoya` (codegen) | Generated JS correctness |
| `zoya` (runner) | End-to-end execution |

```bash
cargo test --workspace              # All tests
cargo test -p zoya-parser           # Single crate
cargo test -p zoya check            # Module tests
cargo test -- --nocapture
```

## Coverage

```bash
cargo llvm-cov --workspace          # Summary report
cargo llvm-cov --workspace --html   # HTML report (target/llvm-cov/html/)
cargo llvm-cov --workspace --open   # Generate and open HTML report
```
