# zoya-ast

Abstract Syntax Tree types for the Zoya programming language.

This crate defines the untyped AST produced by the parser. These types represent the structure of Zoya source code before type checking.

## Types

- **Expressions** - Literals, operators, function calls, match expressions, lambdas, blocks
- **Patterns** - Variable bindings, destructuring, wildcards, rest patterns, as-patterns
- **Items** - Function definitions, struct definitions, enum definitions, type aliases
- **Type annotations** - Named types, generics, tuples, function types
- **Module structure** - Module declarations, use declarations

## Usage

```rust
use zoya_ast::{Expr, Pattern, Item, FunctionDef, TypeAnnotation, Path};

// Create a simple integer expression
let expr = Expr::Int(42);

// Create a function call
let call = Expr::Call {
    path: Path::simple("add".to_string()),
    args: vec![Expr::Int(1), Expr::Int(2)],
};

// Create a pattern
let pattern = Pattern::Tuple(TuplePattern::Exact(vec![
    Pattern::Path(Path::simple("x".to_string())),
    Pattern::Path(Path::simple("y".to_string())),
]));

// Create a type annotation
let ty = TypeAnnotation::Parameterized(
    Path::simple("List".to_string()),
    vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
);
```

## Key Types

| Type | Description |
|------|-------------|
| `Expr` | Expression nodes (literals, calls, operators, match, lambda) |
| `Pattern` | Pattern nodes for destructuring and matching |
| `Item` | Top-level items (functions, structs, enums, type aliases) |
| `TypeAnnotation` | Type syntax in source code |
| `Path` | Qualified paths with optional type arguments |
| `ModuleDef` | A parsed module file |

This crate has no dependencies - it contains only pure data structures.
