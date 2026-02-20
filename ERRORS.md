# Error Handling

## Current State

| Crate | thiserror | Error types | `Result<_, String>` | Status |
|-------|-----------|-------------|---------------------|--------|
| zoya-package | Yes | `ConfigError` (3 variants) | None | **Done** |
| zoya-lexer | Yes | `LexError` (1 variant) | None | **Done** |
| zoya-parser | Yes | `ParseError` + `SyntaxError` | None | **Done** |
| zoya-loader | Yes | `LoaderError` (10+ variants), `SourceError` | None | **Done** |
| zoya-value | Yes | `Error` (5 variants) | None | **Done** |
| zoya-ir | No | `TypeError { message: String }` | `Pathname::new()` | **Not started** |
| zoya-check | No | Uses `TypeError` from ir | `check_irrefutable()` | **Not started** |
| zoya-run | Yes | `EvalError` (2 variants) | `create_runtime()`, test helpers | **Partially done** |
| zoya-std | No | None | `build_std()` | **Not started** |
| zoya (CLI) | No | `NewError` (manual), `BuildError` (String) | All `execute()` fns | **Partially done** |

### What's been done

The leaf crates are solid — `zoya-package`, `zoya-lexer`, `zoya-parser`, `zoya-loader`, and `zoya-value` all use `thiserror` with properly structured error variants. `LoaderError` preserves upstream `LexError`/`ParseError` context. `zoya-value::Error` has 5 specific variants for runtime value errors.

### What remains

The core problem is `TypeError` in `zoya-ir` — it's a `String` wrapper that poisons everything downstream. The type checker (`zoya-check`) creates ~50+ distinct error messages via `format!()`, all flattened into `TypeError { message: String }`. Every layer above then erases errors further:

```
TypeError { message: String }
  → zoya-run: EvalError::RuntimeError(String)
  → zoya-std: Result<_, String>
  → CLI: Result<(), String>
```

## Remaining Work

### 1. `zoya-ir` — Design `TypeError` variants (Large)

The hardest piece. Replace `TypeError { message: String }` with a proper enum. Distinct error categories in the type checker today:

- Type mismatch (argument, return, assignment, pattern)
- Unbound variable / unbound type
- Arity mismatch (function args, type params, tuple elements)
- Visibility violations (private function, private module)
- Missing / extra / duplicate struct fields
- Pattern match exhaustiveness
- Recursive type without indirection
- Invalid operations (comparison on non-comparable types, arithmetic on strings)
- Import errors (not found, duplicate, ambiguous)

Each variant should carry structured data (expected type, actual type, span, name, etc.).

### 2. `zoya-check` — Update all error creation sites (Large)

~50+ call sites currently using `TypeError { message: format!(...) }` need updating to use new variants. This is mechanical but extensive.

### 3. `zoya-run` — Eliminate remaining `Result<_, String>` (Small)

- `create_runtime()` returns `Result<_, String>` — should use `EvalError`
- `TestResult::outcome` is `Result<(), String>` — could use a `TestError` type
- Test helper functions return `Result<_, String>`

### 4. `zoya-std` — Add proper error type (Small)

`build_std()` returns `Result<_, String>`. Replace with a `StdError` enum wrapping `LoaderError` and `TypeError`.

### 5. `zoya` CLI — Proper command errors (Medium)

Every command's `execute()` returns `Result<(), String>`. Options:
- Use `anyhow` at the CLI boundary for ergonomic error propagation
- Or define a `CliError` enum wrapping all upstream error types
- `NewError` already has proper variants but uses manual `Display` — switch to `thiserror`
- `BuildError` in `dev.rs` has `Fatal(String)` / `Recoverable(String)` — needs real variants

### 6. `zoya-loader` — Minor cleanup (Small)

`ConfigError(String)` variant wraps a plain `String` rather than the actual `ConfigError` type. `LexError` and `ParseError` variants store `message: String` rather than embedding upstream error types directly.

## Recommended Order

1. **`zoya-ir` + `zoya-check`** — Design and implement `TypeError` variants (the big one, everything else depends on this)
2. **`zoya-run`** — Wrap errors properly with `EvalError`
3. **`zoya-std`** — Add `StdError`, eliminate `Result<_, String>`
4. **`zoya` CLI** — Adopt `anyhow` or `CliError`, propagate structured errors
5. **`zoya-loader`** — Preserve upstream error types instead of extracting `.message`
