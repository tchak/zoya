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
| zoya-std | Yes | `StdError` (2 variants) | None | **Done** |
| zoya (CLI) | Yes | `InitError` (thiserror), `BuildError` (anyhow) | None | **Done** |

### What's been done

The leaf crates are solid — `zoya-package`, `zoya-lexer`, `zoya-parser`, `zoya-loader`, and `zoya-value` all use `thiserror` with properly structured error variants. `LoaderError` preserves upstream `LexError`/`ParseError` context. `zoya-value::Error` has 5 specific variants for runtime value errors.

`TypeError` in `zoya-ir` is now a proper `thiserror`-based enum with ~30 structured variants covering all error categories: `TypeMismatch`, `TypeMismatchIn`, `UnboundVariable`, `UnboundPath`, `UnboundMethod`, `ArityMismatch`, `TupleLengthMismatch`, `TypeArgCount`, `PrivateAccess`, `PrivateReExport`, `MissingField`, `UnknownField`, `RefutablePattern`, `DuplicateBinding`, `NonExhaustiveMatch`, `UnreachablePattern`, `NamingConvention`, `KindMisuse`, `VariantMismatch`, `InvalidAttribute`, `UnboundImport`, `DuplicateImport`, `ImportValidation`, `InvalidOperatorType`, `InfiniteType`, `CircularTypeAlias`, `CircularReExport`, `InvalidImpl`, `DuplicateDefinition`, `InvalidIndex`, `InvalidInterpolation`, `PathResolution`, `EmptyMatch`, `AssociatedFunctionAsMethod`, `SelfOutsideImpl`. All ~197 creation sites in `zoya-check` have been updated to use structured variants.

In `zoya-run`, `create_runtime()` now returns `Result<_, EvalError>` instead of `Result<_, String>`. Test results use a new `TestError` enum with 4 variants (`Panic`, `RuntimeError`, `Failed`, `UnexpectedReturn`) instead of plain strings. `run_single_test()` and `interpret_test_value()` map errors to structured `TestError` variants.

In `zoya-std`, `build_std()` now returns `Result<_, StdError>` instead of `Result<_, String>`. `StdError` wraps `LoaderError` and `TypeError` via `#[from]`, using `?` instead of `.map_err(|e| format!(...))`.

In the CLI crate (`zoya`), all command `execute()` functions now use `anyhow::Result<()>` instead of `Result<(), String>`. This eliminates all `.map_err(|e| e.to_string())` boilerplate — upstream errors propagate directly via `?`. Ad-hoc errors use `bail!()` and `anyhow!()`. `InitError` (formerly `NewError`) uses `thiserror` for structured error variants. `BuildError` in `dev.rs` holds `anyhow::Error` instead of `String`. The `test.rs` command was already using `Result<(), EvalError>` and required no changes.

## Remaining Work

### 1. `zoya-loader` — Minor cleanup (Small)

`ConfigError(String)` variant wraps a plain `String` rather than the actual `ConfigError` type. `LexError` and `ParseError` variants store `message: String` rather than embedding upstream error types directly.
