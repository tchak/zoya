# zoya-ast

Abstract Syntax Tree types for the Zoya programming language.

This crate defines the untyped AST produced by the parser, including:

- **Expressions** - literals, operators, function calls, match expressions, lambdas
- **Patterns** - variable bindings, destructuring, wildcards, rest patterns
- **Items** - function definitions, struct definitions, enum definitions, type aliases
- **Type annotations** - named types, generics, tuples, function types

## Usage

```rust
use zoya_ast::{Expr, Pattern, Item, TypeAnnotation};
```

This crate has no dependencies - it contains only pure data structures.
