# Zoya Language

A strongly-typed functional programming language that compiles to JavaScript.

## Project Overview

**Goal:** Build a Hindley-Milner type-inferred language inspired by Rust's syntax.

### Compilation Pipeline

```
Source → Lexer → Parser → Type Checker → Typed IR → Codegen → JavaScript → rquickjs
```

### Module Structure

```
src/
├── main.rs      # CLI entry point
├── lexer.rs     # Tokenizer (logos)
├── parser.rs    # Parser (chumsky)
├── ast.rs       # Untyped AST
├── check.rs     # Type checker (returns TypedExpr)
├── ir.rs        # Typed IR (TypedExpr)
├── types.rs     # Type definitions
├── codegen.rs   # JavaScript code generation
├── eval.rs      # JS execution via rquickjs
├── repl.rs      # Interactive REPL
└── runner.rs    # File runner
```

### Current Features

- **Types:** `Int32`, `Int64`, `Float`, `Bool`, `String`, type variables (`T`, `U`)
- **Literals:** integers (`42`, `1_000`), floats (`3.14`, `.5`, `1.`), booleans (`true`, `false`), strings (`"hello"`, `"line\nbreak"`)
- **Operators:**
  - Arithmetic: `+`, `-`, `*`, `/`
  - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
  - Unary: `-` (negation)
- **Functions:** definitions with `fn`, generic type parameters, type annotations, calls
- **Let bindings:** `let x = expr` or `let x: Type = expr`
  - In function bodies (semicolons optional): `fn foo() { let x = 1 let y = 2 x + y }`
  - In REPL (persists across inputs)
- **Type checking:** operands must match types (no implicit coercion)

### Running

```bash
cargo run -- run           # Start REPL
cargo run -- run file.zoya # Run a file
cargo test                 # Run tests
cargo clippy               # Lint
```

### Key Dependencies

- `logos` - Lexer generator
- `chumsky` - Parser combinators
- `rquickjs` - QuickJS JavaScript engine bindings
- `clap` - CLI argument parsing

---

## Version Control

This project uses **jj (Jujutsu)** for version control, not git directly.

### Committing Changes

Use `jj commit` unless more complicated flow is required:

```bash
jj commit -m "<type>: <description>"
```

### Commit Message Format

Follow **Conventional Commits** specification:

```
<type>[optional scope]: <description>
```

**Types:**
- `feat` - new feature
- `fix` - bug fix
- `refactor` - code change that neither fixes a bug nor adds a feature
- `docs` - documentation only
- `test` - adding or updating tests
- `chore` - maintenance tasks, dependencies, tooling
- `perf` - performance improvement
- `style` - formatting, whitespace (not CSS)
- `build` - build system or external dependencies
- `ci` - CI/CD configuration

**Examples:**
```bash
jj commit -m "feat: add pattern matching to parser"
jj commit -m "fix: resolve unification failure with recursive types"
jj commit -m "refactor(codegen): simplify JS emission for let bindings"
jj commit -m "docs: update README with build instructions"
```

**Breaking changes:** Add `!` after type:
```bash
jj commit -m "refactor!: rename Expr to Expression in AST"
```

### Common jj Commands

```bash
jj status          # show working copy status
jj log             # view commit history
jj diff            # show changes in working copy
jj commit -m "..." # commit with message
jj describe -m "..." # change message of current working copy commit
jj new             # start a new change on top of current
jj squash          # squash into parent commit
```

### Guidelines

- Keep commits focused and atomic
- Write descriptions in imperative mood ("add feature" not "added feature")
- Keep the description line under 72 characters
- Use scope sparingly, only when it adds clarity
