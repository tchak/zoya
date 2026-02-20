# Error Handling

## Architecture

Every crate uses [`thiserror`](https://github.com/dtolnay/thiserror) for structured error enums. The CLI boundary uses [`anyhow`](https://github.com/dtolnay/anyhow) for ergonomic error propagation. No diagnostic library (ariadne, miette, etc.) is used — errors are reported as plain text via `Display` impls.

## Error Types Per Crate

| Crate | Error Type | Variants | Notes |
|-------|-----------|----------|-------|
| `zoya-lexer` | `LexError` | `UnexpectedCharacter` | Carries offending slice and byte span |
| `zoya-parser` | `ParseError` | `SyntaxErrors(Vec<SyntaxError>)` | Each `SyntaxError` has span, found, expected, label |
| `zoya-package` | `ConfigError` | `Io`, `Parse`, `InvalidName` | TOML config loading errors |
| `zoya-ir` | `TypeError` | 30+ structured variants | Type mismatch, unbound variable, arity, visibility, exhaustiveness, etc. |
| `zoya-loader` | `LoaderError<P>` | `ModuleNotFound`, `DuplicateMod`, `SourceError`, `LexError`, `ParseError`, `InvalidModName`, `ReservedModName`, `NoPackageToml`, `MainNotFound`, `ConfigError`, `MissingRoot`, `InvalidAttribute` | Generic over path type; embeds `LexError`/`ParseError` as `#[source]` |
| `zoya-value` | `Error` | `Panic`, `TypeMismatch`, `MissingField`, `UnknownVariant`, `UnsupportedConversion`, `ParseError` | Runtime value conversion errors |
| `zoya-run` | `EvalError` | `Panic`, `RuntimeError`, `LoadError`, `TypeError` | Main runtime error type; embeds `LoaderError` and `TypeError` as `#[from]` |
| `zoya-run` | `TestError` | `Panic`, `RuntimeError`, `Failed`, `UnexpectedReturn` | Per-test result error |
| `zoya-std` | `StdError` | `Load`, `Check` | Uses `#[from]` for `LoaderError<String>` and `TypeError` |
| `zoya` (CLI) | `InitError` | `AlreadyExists`, `InvalidPath`, `InvalidName`, `Io` | Project creation errors |
| `zoya-router` | `ConvertError` | (internal) | HTTP response conversion, not exported |

## Error Flow

```
                 COMPILE TIME                              RUNTIME
                 ============                              =======

zoya-lexer::LexError ──────────┐
                               ├──> zoya-loader::LoaderError ──┐
zoya-parser::ParseError ───────┘         |                     |
                                         |                     ├──> EvalError::LoadError (#[from])
zoya-package::ConfigError ──── #[from] ──┘                     |
                                                               ├──> CLI: anyhow::Result
zoya-ir::TypeError ──────────────────> EvalError::TypeError ───┤
   (from zoya-check)                       (#[from])           |
                                                               |
zoya-value::Error ──> EvalError (via From impl) ───────────────┘
                                                               |
JS runtime errors ──> parse_zoya_error() ──> EvalError ────────┘
```

### Key Propagation Details

- **`LexError`/`ParseError` into `LoaderError`**: Embedded directly as `#[source]` fields, preserving full structure.
- **`ConfigError` into `LoaderError`**: Embedded directly via `#[from]`, preserving full structure.
- **`LoaderError` into `EvalError`**: Embedded as `EvalError::LoadError` via `#[from]` (path mapped to `String`).
- **`TypeError` into `EvalError`**: Embedded as `EvalError::TypeError` via `#[from]`.
- **`zoya-value::Error` into `EvalError`**: Proper `From` impl — `Panic` maps to `Panic`, others to `RuntimeError`.
- **`StdError`**: Uses `#[from]` for automatic `?` propagation from `LoaderError<String>` and `TypeError`.
- **CLI boundary**: All commands return `anyhow::Result<()>` (except `test` which returns `Result<(), EvalError>`).

## CLI Error Reporting

Errors reach the user via `main.rs`'s `fatal()` function, which prints `"error: <message>"` in red/bold to stderr using the `console` crate.

## Remaining Work

- **Catch-all `message: String` variants in `TypeError`** — `InvalidAttribute`, `ImportValidation`, `InvalidImpl`, `InvalidIndex`, `InvalidInterpolation`, `PathResolution` could be further structured
- **Rich diagnostics** — source spans exist in `LexError` and `SyntaxError` but are only rendered as `"at 5..8"` in Display output; no source-pointing or colorized span rendering
