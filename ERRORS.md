# Error Handling

## Current State

| Crate | thiserror | Error types | `Result<_, String>` | Status |
|-------|-----------|-------------|---------------------|--------|
| zoya-package | Yes | `ConfigError` (3 variants) | None | **Done** |
| zoya-lexer | Yes | `LexError` (1 variant) | None | **Done** |
| zoya-parser | Yes | `ParseError` + `SyntaxError` | None | **Done** |
| zoya-loader | Yes | `LoaderError` (10+ variants), `SourceError` | None | **Done** |
| zoya-value | Yes | `Error` (5 variants) | None | **Done** |
| zoya-ir | Yes | `TypeError` (~30 variants) | `Pathname::new()` | **Done** |
| zoya-check | Yes | Uses `TypeError` from ir | None | **Done** |
| zoya-run | Yes | `EvalError` (2 variants), `TestError` (4 variants) | None | **Done** |
| zoya-std | No | None | `build_std()` | **Not started** |
| zoya (CLI) | No | `NewError` (manual), `BuildError` (String) | All `execute()` fns | **Partially done** |

### What's been done

The leaf crates are solid — `zoya-package`, `zoya-lexer`, `zoya-parser`, `zoya-loader`, and `zoya-value` all use `thiserror` with properly structured error variants. `LoaderError` preserves upstream `LexError`/`ParseError` context. `zoya-value::Error` has 5 specific variants for runtime value errors.

`TypeError` in `zoya-ir` is now a proper `thiserror`-based enum with ~30 structured variants covering all error categories: `TypeMismatch`, `TypeMismatchIn`, `UnboundVariable`, `UnboundPath`, `UnboundMethod`, `ArityMismatch`, `TupleLengthMismatch`, `TypeArgCount`, `PrivateAccess`, `PrivateReExport`, `MissingField`, `UnknownField`, `RefutablePattern`, `DuplicateBinding`, `NonExhaustiveMatch`, `UnreachablePattern`, `NamingConvention`, `KindMisuse`, `VariantMismatch`, `InvalidAttribute`, `UnboundImport`, `DuplicateImport`, `ImportValidation`, `InvalidOperatorType`, `InfiniteType`, `CircularTypeAlias`, `CircularReExport`, `InvalidImpl`, `DuplicateDefinition`, `InvalidIndex`, `InvalidInterpolation`, `PathResolution`, `EmptyMatch`, `AssociatedFunctionAsMethod`, `SelfOutsideImpl`. All ~197 creation sites in `zoya-check` have been updated to use structured variants.

In `zoya-run`, `create_runtime()` now returns `Result<_, EvalError>` instead of `Result<_, String>`. Test results use a new `TestError` enum with 4 variants (`Panic`, `RuntimeError`, `Failed`, `UnexpectedReturn`) instead of plain strings. `run_single_test()` and `interpret_test_value()` map errors to structured `TestError` variants.

Downstream consumers still use `.to_string()` which continues to work since `Display` is derived via `thiserror`.

### What remains

The remaining work is in the downstream crates — propagating structured errors instead of flattening to strings:

```
TypeError (structured enum)
  → zoya-std: Result<_, String>                ← still flattens
  → CLI: Result<(), String>                    ← still flattens
```

## Remaining Work

### 1. `zoya-std` — Add proper error type (Small)

`build_std()` returns `Result<_, String>`. Replace with a `StdError` enum wrapping `LoaderError` and `TypeError`.

### 2. `zoya` CLI — Proper command errors (Medium)

Every command's `execute()` returns `Result<(), String>`. Options:
- Use `anyhow` at the CLI boundary for ergonomic error propagation
- Or define a `CliError` enum wrapping all upstream error types
- `NewError` already has proper variants but uses manual `Display` — switch to `thiserror`
- `BuildError` in `dev.rs` has `Fatal(String)` / `Recoverable(String)` — needs real variants

### 3. `zoya-loader` — Minor cleanup (Small)

`ConfigError(String)` variant wraps a plain `String` rather than the actual `ConfigError` type. `LexError` and `ParseError` variants store `message: String` rather than embedding upstream error types directly.

## Recommended Order

1. **`zoya-std`** — Add `StdError`, eliminate `Result<_, String>`
2. **`zoya` CLI** — Adopt `anyhow` or `CliError`, propagate structured errors
3. **`zoya-loader`** — Preserve upstream error types instead of extracting `.message`
