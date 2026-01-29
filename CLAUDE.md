# Zoya Development Guide

Strongly-typed functional language compiling to JavaScript. See [README.md](README.md) for language documentation.

## Architecture

```
Source → Lexer → Parser → Type Checker → Typed IR → Codegen → JavaScript
```

### Source Files

```
src/
├── main.rs        # CLI (clap)
├── lexer.rs       # Tokenizer (logos)
├── parser.rs      # Parser (chumsky)
├── ast.rs         # Untyped AST
├── check.rs       # Type checker → TypedExpr
├── unify.rs       # Type unification (Union-Find)
├── usefulness.rs  # Pattern exhaustiveness (Maranget)
├── ir.rs          # Typed IR
├── types.rs       # Type definitions
├── codegen.rs     # JavaScript generation
├── eval.rs        # JS execution (rquickjs)
├── repl.rs        # REPL (rustyline)
└── runner.rs      # File runner
```

## Commands

```bash
cargo run -- run              # REPL
cargo run -- run file.zoya    # Run file
cargo run -- check file.zoya  # Type-check only
cargo run -- build file.zoya  # Compile to JS
cargo test                    # Run tests
cargo clippy                  # Lint
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

| File | Tests |
|------|-------|
| `lexer.rs` | Token recognition |
| `parser.rs` | AST structure |
| `check.rs` | Type checking pass/fail |
| `codegen.rs` | Generated JS correctness |
| `runner.rs` | End-to-end execution |

```bash
cargo test              # All tests
cargo test parser       # Module tests
cargo test -- --nocapture
```

## Coverage

```bash
cargo llvm-cov          # Summary report
cargo llvm-cov --html   # HTML report (target/llvm-cov/html/)
cargo llvm-cov --open   # Generate and open HTML report
```
