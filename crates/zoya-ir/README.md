# zoya-ir

Typed Intermediate Representation for the Zoya programming language.

This crate defines the type-checked IR produced by the type checker, including:

- **Types** - Int, Float, Bool, String, List, Tuple, Struct, Enum, Function types
- **Typed expressions** - Type-annotated AST nodes after type checking
- **Typed patterns** - Patterns with resolved types for code generation
- **Checked items** - Type-checked function, struct, enum, and type alias definitions

## Usage

```rust
use zoya_ir::{Type, TypedExpr, CheckedItem};
```

## Dependencies

- [zoya-ast](../zoya-ast) - Untyped AST types (for operators and shared definitions)
