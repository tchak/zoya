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
├── main.rs        # CLI entry point
├── lexer.rs       # Tokenizer (logos)
├── parser.rs      # Parser (chumsky)
├── ast.rs         # Untyped AST
├── check.rs       # Type checker (returns TypedExpr)
├── unify.rs       # Type unification (Union-Find)
├── usefulness.rs  # Pattern exhaustiveness (Maranget's algorithm)
├── ir.rs          # Typed IR (TypedExpr)
├── types.rs       # Type definitions
├── codegen.rs     # JavaScript code generation
├── eval.rs        # JS execution via rquickjs
├── repl.rs        # Interactive REPL (rustyline)
└── runner.rs      # File runner
```

### Current Features

- **Types:** `Int`, `BigInt`, `Float`, `Bool`, `String`, `List<T>`, tuples `(T, U, ...)`, functions `T -> U`, type variables (`T`, `U`)
- **Literals:**
  - Integers (Int): `42`, `1_000`
  - BigInts: `42n`, `9_000_000_000n` (with `n` suffix)
  - Floats: `3.14`, `0.5`
  - Booleans: `true`, `false`
  - Strings: `"hello"`, `"line\nbreak"`
  - Lists: `[1, 2, 3]`, `[]`
  - Tuples: `(1, "hello")`, `()`, `(42,)` (single-element)
- **Operators:**
  - Arithmetic: `+`, `-`, `*`, `/`
  - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
  - Unary: `-` (negation)
- **Functions:** definitions with `fn`, generic type parameters, type annotations, calls
  - Simple bodies can omit braces: `fn square(x: Int) -> Int x * x`
- **Let bindings:** `let x = expr` or `let x: Type = expr`
  - In function bodies (semicolons required): `fn foo() { let x = 1; let y = 2; x + y }`
  - In REPL (persists across inputs)
- **Lambdas (anonymous functions):** Rust-inspired syntax with let polymorphism
  - Simple: `|x| x + 1`
  - Multi-param: `|x, y| x + y`
  - No params: `|| 42`
  - Type annotations: `|x: Int| x * 2`
  - Return type: `|x| -> Int x + 1`
  - Block body: `|x| { let y = x * 2; y + 1 }`
  - Function type annotations: `let f: Int -> Int = |x| x + 1`
  - Multi-param function types: `let f: (Int, Int) -> Int = |x, y| x + y`
  - Higher-order functions: `fn apply(f: Int -> Int, x: Int) -> Int f(x)`
  - Let polymorphism: `let id = |x| x; id(42); id("hello")` (both work!)
- **Structs:** product types with named fields
  - Definition: `struct Point { x: Int, y: Int }`
  - Generic structs: `struct Pair<T, U> { first: T, second: U }`
  - Construction: `Point { x: 1, y: 2 }`, shorthand `Point { x, y }` when variable names match
  - Field access: `point.x`, `pair.first`
- **Enums:** sum types with unit, tuple, and struct variants
  - Definition: `enum Option<T> { None, Some(T) }`
  - Unit variants: `enum Color { Red, Green, Blue }`
  - Tuple variants: `enum Result<T, E> { Ok(T), Err(E) }`
  - Struct variants: `enum Message { Quit, Move { x: Int, y: Int }, Write(String) }`
  - Construction: `Option::Some(42)`, `Color::Red`, `Message::Move { x: 1, y: 2 }`
  - Pattern matching with all variant types
- **Pattern matching:** `match expr { pattern => result ... }`
  - Literal patterns: `0`, `"hello"`, `true`, `3.14`
  - Variable patterns: `n` (binds the matched value)
  - Wildcard pattern: `_` (matches anything, no binding)
  - List patterns: `[]`, `[x, ..]`, `[.., x]`, `[a, .., b]`, `[a, b]`
  - Tuple patterns: `(x, y)`, `(a, ..)`, `(.., z)`, `(a, .., z)`
  - Struct patterns: `Point { x, y }`, `Point { x: px, .. }` (with shorthand and rest)
  - Enum patterns: `Option::Some(x)`, `Message::Move { x, y }`, `Color::Red`
  - Block expressions in arms: `n => { let x = n * 2; x + 1 }`
  - Exhaustiveness checking (compile error if cases missing)
  - Unreachable pattern detection (compile error for dead code)
  - Implementation: Maranget's algorithm (`src/usefulness.rs`)
- **Method calls:** `expr.method(args)` on built-in types
  - String: `len()`, `is_empty()`, `contains(s)`, `starts_with(s)`, `ends_with(s)`, `to_uppercase()`, `to_lowercase()`, `trim()`
  - Int: `abs()`, `to_string()`, `to_float()`, `min(n)`, `max(n)`
  - BigInt: `abs()`, `to_string()`, `min(n)`, `max(n)`
  - Float: `abs()`, `to_string()`, `to_int()`, `floor()`, `ceil()`, `round()`, `sqrt()`, `min(n)`, `max(n)`
  - List: `len()`, `is_empty()`, `push(x)`, `concat(list)`, `reverse()` (all return new lists, immutable)
- **Type checking:** operands must match types (no implicit coercion)
- **REPL:** line editing, history (persisted to `~/.zoya_history`)

### Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features.

### CLI Commands

```bash
cargo run -- run              # Start REPL
cargo run -- run file.zoya    # Run a file
cargo run -- check file.zoya  # Type-check without executing
cargo run -- build file.zoya  # Compile to JS (stdout)
cargo run -- build file.zoya -o out.js  # Compile to JS file
cargo test                    # Run tests
cargo clippy                  # Lint
```

### Key Dependencies

- `logos` - Lexer generator
- `chumsky` - Parser combinators
- `rquickjs` - QuickJS JavaScript engine bindings
- `clap` - CLI argument parsing
- `rustyline` - REPL line editing and history
- `dirs` - Cross-platform directory paths

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

---

## Testing Guidelines

Every new feature should include tests at multiple levels of the compilation pipeline.

### Required Tests by Module

| Module | Test Location | What to Test |
|--------|---------------|--------------|
| `lexer.rs` | `lexer::tests` | New tokens lex correctly |
| `parser.rs` | `parser::tests` | AST structure is correct |
| `check.rs` | `check::tests` | Type checking succeeds/fails appropriately |
| `codegen.rs` | `codegen::tests` | Generated JS is correct |
| `runner.rs` | `runner::tests` | End-to-end integration tests |

### Example: Adding a New Feature

When adding a feature like method calls, include:

1. **Lexer tests** - New tokens (e.g., `Dot`) are recognized
2. **Parser tests** - Expressions parse to correct AST shape
3. **Type checker tests** - Valid code type-checks, invalid code produces errors
4. **Codegen tests** - Generated JavaScript is correct (if applicable)
5. **Runner tests** - Full pipeline works end-to-end with actual execution

### Running Tests

```bash
cargo test                 # Run all tests
cargo test lexer           # Run only lexer tests
cargo test parser          # Run only parser tests
cargo test check           # Run only type checker tests
cargo test runner          # Run only integration tests
cargo test -- --nocapture  # Show println! output
```
