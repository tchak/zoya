# Error Handling Overhaul Plan

## Current State

**No `thiserror` or `anyhow` anywhere** — all `Display` and `Error` impls are hand-written. The codebase has a mix of:

- **Structured error enums** (good): `ConfigError`, `LoaderError`, `SourceError`, `NewError`, `EvalError` — these have proper variants with context
- **Message-only structs** (weak): `LexError`, `ParseError`, `TypeError` — just a `String` message field, no variants
- **`Result<T, String>`** (bad): used throughout the CLI commands (`check.rs`, `build.rs`, `repl.rs`) and `zoya-std`

**No `From` impls exist anywhere** — every error conversion is manual `.map_err()`.

## Error Flow

```
LexError, ParseError ──→ LoaderError (wraps them, but extracts .message as String)
ConfigError ───────────→ LoaderError::ConfigError(String)  ← loses structure!
SourceError ───────────→ LoaderError::SourceError { .. }   ← preserved
TypeError ─────────────→ EvalError::RuntimeError(String)   ← loses structure!
LoaderError ───────────→ EvalError::RuntimeError(String)   ← loses structure!
Everything ────────────→ String at CLI boundary
```

The pattern: structured errors exist in the middle layers but get flattened to strings both upstream (into `LoaderError`) and downstream (into CLI commands).

## What the Overhaul Involves

### Per-crate work (roughly bottom-up):

| Crate | Work | Difficulty |
|-------|------|-----------|
| **zoya-lexer** | Add variants to `LexError` (unexpected char, unterminated string, etc.), derive `thiserror` | Small |
| **zoya-parser** | Add variants to `ParseError`, derive `thiserror` | Small-Medium |
| **zoya-package** | `ConfigError` already has good variants — just swap manual impls for `thiserror` derives | Trivial |
| **zoya-ir** | `TypeError` needs real variants (mismatch, unbound var, arity, visibility, etc.) — this is the **hardest** piece since the type checker creates ~50+ distinct error messages via `format!` | **Large** |
| **zoya-loader** | `LoaderError` and `SourceError` already structured — swap to `thiserror`, add `From` impls, preserve `LexError`/`ParseError` as nested sources instead of extracting `.message` | Medium |
| **zoya-check** | All the `TypeError { message: format!(...) }` call sites need updating to use new variants | **Large** |
| **zoya-run** | `EvalError` needs to wrap `LoaderError`/`TypeError` as `#[source]` instead of `.to_string()` | Medium |
| **zoya-std** | Replace `Result<_, String>` with a proper `StdError` | Small |
| **zoya** (CLI) | Replace all `Result<_, String>` with a `CliError` or per-command errors, use `anyhow` at the top level, or a unified error type | Medium |

### The hard part: `TypeError`

The type checker currently has ~50+ different error messages all stuffed into `TypeError { message: String }`. Designing good variants for these is the bulk of the design work. Examples of distinct errors today:

- Type mismatch in argument/return/assignment
- Unbound variable/type
- Arity mismatch
- Visibility violations
- Missing/extra fields
- Pattern match exhaustiveness
- Recursive type without indirection

Each of these ideally becomes a variant carrying structured data (expected type, got type, span, etc.).

## Should You Start Bottom-Up?

**Yes, absolutely.** The leaf crates (`zoya-lexer`, `zoya-parser`, `zoya-package`) can be converted independently with zero impact on the rest of the codebase. Each one is a self-contained change. This lets you:

1. Establish the `thiserror` pattern
2. Get familiar with the conversion
3. Each crate compiles and tests in isolation

Then move to `zoya-loader` (which wraps the leaf errors), then `zoya-ir`/`zoya-check` (the big one), then `zoya-run`, then `zoya-std`, and finally the CLI.

## Should You Create a `zoya-error` Crate?

**Probably not.** Here's why:

- Each crate's errors are specific to its domain — there's very little shared infrastructure
- `thiserror` already provides the shared infrastructure (derive macros for `Display`, `Error`, `From`)
- A shared crate would create a circular-dependency risk or force error types out of the crates that define them
- The one thing you might share is a `Span` or `SourceLocation` type for attaching source positions to errors — but that already lives in `zoya-ast` and could be reused

The only scenario where `zoya-error` makes sense is if you want a **unified compiler diagnostic type** (like `Diagnostic { severity, message, span, notes }`) that all crates emit. That's a different (larger) design — more like what `rustc` or `ariadne` does. Worth considering but not required for the `thiserror` migration.

## Recommended Order

1. `zoya-package` (trivial, already well-structured)
2. `zoya-lexer` (small, self-contained)
3. `zoya-parser` (small, self-contained)
4. `zoya-loader` (medium, wraps the above — add `From` impls)
5. `zoya-ir` + `zoya-check` (large — design `TypeError` variants, update all call sites)
6. `zoya-run` (medium, wrap errors properly)
7. `zoya-std` (small, eliminate `String` errors)
8. `zoya` CLI (medium, proper command errors or `anyhow` at the boundary)

Steps 1-4 are safe, incremental, and independently shippable. Step 5 is the big design decision. Steps 6-8 follow naturally.
