# Type Error Diagnostics — Implementation Plan

## Goal

Render type errors with source-annotated diagnostics (like we already do for lex/parse errors), so users see the exact source location of type mismatches, unbound variables, arity errors, etc.

## Current State

- **AST** (`zoya-ast`): No spans. `Expr`, `Pattern`, and all other nodes are plain data.
- **Parser** (`zoya-parser`): Chumsky 0.10, spans available via `map_with(|val, extra| extra.span())` but unused. Byte spans stored separately, only used for error conversion.
- **Type checker** (`zoya-check`): 220 `TypeError` creation sites across 8 files. No span info in `TypeError`.
- **CLI** (`zoya`): Phase 1 miette rendering already done for lex/parse errors.

## Approach

Rename `Expr` → `ExprKind` and `Pattern` → `PatternKind`. Introduce wrapper structs:

```rust
pub type Span = std::ops::Range<usize>;  // byte offsets

pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}
```

This is the standard approach used by rustc (`Expr { kind: ExprKind, span }`) and other mature compilers.

**What gets spans**: `Expr` and `Pattern` only (Phase 1). This covers 166 of 220 TypeError creation sites (75%). The remaining 54 sites involve `TypeAnnotation`, `Path`, `UseDecl`, and definition-level nodes — those can be addressed in a future Phase 2.

**What does NOT get spans**: `TypedExpr`, `TypedPattern` (IR output of successful type checking — no errors possible), codegen input.

## Scope Analysis

### Sites that need updating

| Area | `Expr::` refs | `Pattern::` refs | Total |
|------|--------------|-----------------|-------|
| zoya-parser (source) | 376 | 153 | 529 |
| zoya-check (source) | 192 | 450 | 642 |
| zoya-check (tests) | 481 | 68 | 549 |
| zoya-fmt | 27 | 20 | 47 |
| zoya (CLI/REPL) | 6 | 26 | 32 |
| zoya-ir (re-exports) | 21 | 0 | 21 |
| **Total** | **1,103** | **717** | **~1,820** |

**Not affected**: `zoya-codegen` (uses `TypedExpr`/`TypedPattern` from IR, not AST types), `zoya-run`, `zoya-value`, `zoya-loader`.

### TypeError creation sites (what spans cover)

| File | Sites | Has Expr/Pattern access? |
|------|-------|--------------------------|
| `check.rs` | 104 | Yes — walks Expr via `check_expr` |
| `pattern.rs` | 62 | Yes — walks Pattern via `check_pattern` |
| `type_resolver.rs` | 16 | No — walks TypeAnnotation |
| `definition.rs` | 10 | No — walks Item definitions |
| `unify.rs` | 9 | No — receives Type, not Expr |
| `imports.rs` | 9 | No — walks UseDecl |
| `resolution.rs` | 8 | Partial — resolves Path |
| `usefulness.rs` | 2 | No — walks TypedMatchArm |

**Phase 1 coverage**: check.rs (104) + pattern.rs (62) = **166 sites (75%)**

## Phases

### Phase 1: AST rename + wrapper structs

**Goal**: Rename `Expr` → `ExprKind`, `Pattern` → `PatternKind`, introduce `Expr { kind, span }` and `Pattern { kind, span }` wrappers. All spans are `0..0` (dummy) — no parser changes yet.

**zoya-ast/src/lib.rs**:
- Rename `pub enum Expr` → `pub enum ExprKind`
- Rename `pub enum Pattern` → `pub enum PatternKind`
- Add `pub type Span = std::ops::Range<usize>;`
- Add wrapper structs:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub struct Expr {
      pub kind: ExprKind,
      pub span: Span,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub struct Pattern {
      pub kind: PatternKind,
      pub span: Span,
  }
  ```
- Add convenience constructors for tests:
  ```rust
  impl Expr {
      pub fn new(kind: ExprKind, span: Span) -> Self { Self { kind, span } }
      pub fn unspanned(kind: ExprKind) -> Self { Self { kind, span: 0..0 } }
  }
  impl Pattern {
      pub fn new(kind: PatternKind, span: Span) -> Self { Self { kind, span } }
      pub fn unspanned(kind: PatternKind) -> Self { Self { kind, span: 0..0 } }
  }
  ```
- Update `LetBinding`, `MatchArm`, `FunctionDef`, `LambdaParam`, `Param`, `Stmt`, etc. — these reference `Expr`/`Pattern` by value and should continue working since the type names don't change, but inner code that pattern-matches needs updating.
- Update internal tests in zoya-ast.

**All downstream crates** (mechanical find-and-replace):
- `Expr::Variant { .. }` → `ExprKind::Variant { .. }` (in match arms)
- `match expr {` → `match expr.kind {` or `match &expr.kind {`
- `Expr::Variant { fields }` (construction) → `Expr::unspanned(ExprKind::Variant { fields })` (in tests) or `Expr::new(ExprKind::Variant { fields }, span)` (in parser)
- `Pattern::Variant { .. }` → same treatment
- Update `use` imports to include `ExprKind`/`PatternKind`

**Affected crates**: zoya-parser, zoya-check, zoya-fmt, zoya (REPL), zoya-ir (minor)

**Not affected**: zoya-codegen (uses TypedExpr), zoya-run, zoya-value, zoya-loader

### Phase 2: Parser span capture

**Goal**: Replace dummy `0..0` spans with real byte-offset spans from the parser.

**Strategy**: Use chumsky's `map_with(|val, extra| extra.span())` to capture token-index spans, then convert to byte-offset spans post-parse.

**Token-index → byte-offset conversion**: Same logic already in `convert_errors`:
```rust
fn token_span_to_byte_span(token_span: SimpleSpan, byte_spans: &[Span]) -> Span {
    let start = byte_spans.get(token_span.start).map(|s| s.start).unwrap_or(0);
    let end = byte_spans.get(token_span.end.saturating_sub(1)).map(|s| s.end).unwrap_or(start);
    start..end
}
```

**Parser changes** (~52 combinator sites):
- `expressions.rs`: ~28 sites — change `.map()` → `.map_with()`, `.to()` → `.map_with()`, `foldl`/`foldr` closures need span computation (e.g., `left.span.start..right.span.end`)
- `patterns.rs`: ~13 sites — same treatment
- `items.rs`: ~10 sites — for `FunctionDef` bodies
- `helpers.rs`, `types.rs`, `statements.rs`: ~11 sites combined

**Post-parse span conversion**: Add a tree-walk function that converts all spans in a parsed `Vec<Item>` from token indices to byte offsets. Call it after `parser.parse()` but before returning from `parse_module`/`parse_input`.

### Phase 3: Add span to TypeError + thread through type checker

**Goal**: `TypeError` carries an `Option<Span>` so the CLI can render source annotations.

**zoya-ir/src/types.rs**:
- Add `span: Option<Span>` field to every `TypeError` variant (or use a wrapper). The `Option` allows the 54 sites without span access to continue working.
- Alternative: wrapper struct `SpannedTypeError { error: TypeError, span: Option<Span> }`. This avoids modifying every variant but changes the error type signature.

**zoya-check source files** (166 sites in check.rs + pattern.rs):
- `check_expr` has access to `&Expr` → use `expr.span` when creating errors
- `check_pattern` has access to `&Pattern` → use `pattern.span`
- Helper functions that receive destructured parts (e.g., `check_bin_op(op, left, right, ...)`) need the span passed as an additional parameter, OR receive `&Expr` instead of destructured fields
- `unify()` (9 sites): callers wrap with `.map_err()` and have `Expr` access — add span there
- Remaining 54 sites: pass `None` for span

**LoaderError propagation**: `LoaderError` wraps `TypeError` (via `EvalError`). The `source_text` is already stored in `LoaderError::LexError`/`ParseError` from Phase 1 of the miette work. For type errors, the source text needs to be carried similarly — add `source_text: String` and `path: P` to a new `LoaderError::TypeError` variant, or add them to the existing `EvalError::TypeError`.

### Phase 4: CLI diagnostic rendering

**Goal**: Extend `diagnostic.rs` to render `TypeError` with miette, similar to lex/parse errors.

**crates/zoya/src/diagnostic.rs**:
- Add `render_type_error(path, source_text, error, span)` function
- Build `MietteDiagnostic` with `LabeledSpan` from the span
- Extend `try_render_diagnostic` to handle `TypeError` (via `EvalError::TypeError`)

**Source text availability**: The type checker runs after loading (which reads file content). The source text needs to be plumbed from the loader to the CLI error handler. Options:
- Store source text in `CheckedPackage` (add a `sources: HashMap<QualifiedPath, String>` field)
- Store source text alongside `TypeError` (add `source_text: String` to the error or a wrapper)
- Re-read the file at error time (simple but wasteful)

## Execution Order

The phases can be executed incrementally with the codebase compiling and tests passing after each:

1. **Phase 1** (AST rename) — largest change by line count (~1,820 sites), but purely mechanical. Can be done with search-and-replace + targeted fixes. Tests remain passing with dummy spans.

2. **Phase 2** (parser spans) — moderate complexity, ~52 combinator sites. Parser tests need updating to check span values.

3. **Phase 3** (TypeError + type checker) — moderate complexity, 166 sites get spans, 54 get `None`. Requires decisions on source text plumbing.

4. **Phase 4** (CLI rendering) — small, builds on existing miette infrastructure.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Phase 1 is huge (~1,820 mechanical changes) | Could introduce subtle bugs in match arms | Run full test suite after each file; changes are purely mechanical |
| `foldl`/`foldr` span computation in parser | Spans might be slightly off for binary ops | Compute as `left.span.start..right.span.end`; verify with manual tests |
| Source text plumbing for type errors | Architectural decision needed | Decide in Phase 3; simplest is storing sources in CheckedPackage |
| 54 TypeError sites without spans | Incomplete coverage | Accept `None` spans for Phase 1; add spans to TypeAnnotation/Path/UseDecl in a future Phase 2 |
| Formatter must handle new Expr struct | Extra `.kind` access everywhere | Mechanical change, same as other crates |

## Future Work (beyond this plan)

- **Spans on TypeAnnotation, Path, UseDecl, Item-level nodes** — covers the remaining 54 TypeError sites
- **Multi-span errors** — some type errors (e.g., type mismatch) could show both the expected and actual locations
- **Type error help text** — miette supports `help` annotations for suggestions
