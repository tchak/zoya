# zoya-ast

Abstract Syntax Tree types for the Zoya programming language.

This crate defines the untyped AST produced by the parser. These types represent the structure of Zoya source code before type checking.

## Types

- **Expressions** - Literals, operators, function calls, match expressions, lambdas, blocks, interpolated strings
- **Patterns** - Variable bindings, destructuring, wildcards, rest patterns, as-patterns
- **Items** - Function definitions, struct definitions, enum definitions, type aliases, use declarations, impl blocks
- **Type annotations** - Named types, generics, tuples, function types
- **Module structure** - Module declarations (`ModDecl`), use declarations (`UseDecl`)
- **Visibility** - `Visibility` enum (`Private`, `Public`) for controlling item access
- **Attributes** - `#[test]`, `#[builtin]`, `#[mode(test)]` annotations on items

## Usage

```rust
use zoya_ast::{Expr, Pattern, Item, FunctionDef, TypeAnnotation, Path, Visibility};

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

// Visibility defaults to Private
let vis = Visibility::default(); // Private

// String interpolation
let parts = vec![
    StringPart::Literal("hello ".to_string()),
    StringPart::Expr(Box::new(Expr::Path(Path::simple("name".to_string())))),
    StringPart::Literal("!".to_string()),
];
let interp = Expr::InterpolatedString(parts);

// List with spread
let list = Expr::List(vec![
    ListElement::Item(Expr::Int(0)),
    ListElement::Spread(Expr::Path(Path::simple("xs".to_string()))),
]);
```

## Key Types

| Type | Description |
|------|-------------|
| `Expr` | Expression nodes (literals, calls, operators, match, lambda, interpolated strings) |
| `Pattern` | Pattern nodes for destructuring and matching |
| `Item` | Top-level items (functions, structs, enums, type aliases, use declarations, impl blocks) |
| `ImplBlock` | Impl block with target type, type params, and methods |
| `ImplMethod` | Method definition with optional `self` parameter |
| `TypeAnnotation` | Type syntax in source code |
| `Path` | Qualified paths with optional type arguments and prefixes (`root::`, `self::`, `super::`) |
| `PathPrefix` | Path prefix (`None`, `Root`, `Self_`, `Super`) |
| `Visibility` | Item visibility (`Private`, `Public`) |
| `ModDecl` | Module declaration with visibility and name |
| `UseDecl` | Use/import declaration with visibility and path |
| `ListElement` | List element: `Item(Expr)` or `Spread(Expr)` |
| `StringPart` | String interpolation part: `Literal(String)` or `Expr(Box<Expr>)` |
| `Attribute` | Item attributes (`#[test]`, `#[builtin]`, `#[mode(...)]`) |

This crate has no dependencies - it contains only pure data structures.
