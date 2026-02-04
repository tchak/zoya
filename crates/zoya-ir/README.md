# zoya-ir

Typed Intermediate Representation for the Zoya programming language.

This crate defines the type-checked IR produced by the type checker. All expressions, patterns, and items carry resolved type information.

## Types

- **Primitive types** - `Int`, `BigInt`, `Float`, `Bool`, `String`
- **Compound types** - `List<T>`, tuples `(T, U, ...)`, functions `T -> U`
- **User-defined types** - Structs, enums with generics
- **Type variables** - For inference and polymorphism

## Usage

```rust
use zoya_ir::{Type, TypedExpr, CheckedItem, CheckedPackage};

// Type represents resolved types
let list_int = Type::List(Box::new(Type::Int));
let pair = Type::Tuple(vec![Type::Int, Type::String]);
let func = Type::Function {
    params: vec![Type::Int],
    ret: Box::new(Type::Bool),
};

// TypedExpr is an expression with type information
let expr = TypedExpr::Int(42);
assert_eq!(expr.ty(), Type::Int);

// CheckedPackage contains all type-checked modules
fn process_package(pkg: &CheckedPackage) {
    for (path, module) in &pkg.modules {
        for item in &module.items {
            match item {
                CheckedItem::Function(f) => {
                    println!("fn {}: {:?}", f.name, f.return_type);
                }
                CheckedItem::Struct(s) => {
                    println!("struct {}", s.name);
                }
                CheckedItem::Enum(e) => {
                    println!("enum {}", e.name);
                }
                CheckedItem::TypeAlias(t) => {
                    println!("type {}", t.name);
                }
            }
        }
    }
}
```

## Key Types

| Type | Description |
|------|-------------|
| `Type` | Resolved type (Int, List<T>, structs, enums, functions) |
| `TypedExpr` | Expression with attached type information |
| `TypedPattern` | Pattern with resolved types for codegen |
| `CheckedItem` | Type-checked function, struct, enum, or type alias |
| `CheckedModule` | A module's checked items |
| `CheckedPackage` | Complete package of checked modules |
| `TypeError` | Type checking error with message and span |

## Dependencies

- [zoya-ast](../zoya-ast) - Untyped AST types (for operators and shared definitions)
- [zoya-package](../zoya-package) - Package data structures (for module paths)
