# zoya-ir

Typed Intermediate Representation for the Zoya programming language.

This crate defines the type-checked IR produced by the type checker. All expressions, patterns, and items carry resolved type information.

## Types

- **Primitive types** - `Int`, `BigInt`, `Float`, `Bool`, `String`
- **Compound types** - `List<T>`, tuples `(T, U, ...)`, functions `T -> U`
- **User-defined types** - Structs, enums with generics
- **Type variables** - For inference and polymorphism
- **Visibility** - Re-exported from `zoya-ast` for use throughout the compiler

## Usage

```rust
use zoya_ir::{Type, TypedExpr, CheckedItem, CheckedPackage, Visibility, QualifiedPath};

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

// QualifiedPath represents fully resolved paths
let path = QualifiedPath::new(vec!["root".into(), "utils".into(), "helper".into()]);
assert_eq!(path.to_string(), "root::utils::helper");

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
| `QualifiedPath` | Fully resolved path (e.g., `root::utils::helper`) |
| `Definition` | Type definition with visibility and defining module |
| `FunctionType` | Function signature with visibility, module, type params |
| `StructType` | Struct definition with visibility, module, fields |
| `EnumType` | Enum definition with visibility, module, variants |
| `TypeAliasType` | Type alias with visibility, module, underlying type |
| `Visibility` | Re-exported `Private`/`Public` enum from `zoya-ast` |
| `TypeError` | Type checking error with message |

## Dependencies

- [zoya-ast](../zoya-ast) - Untyped AST types (for operators and shared definitions)
- [zoya-package](../zoya-package) - Package data structures (for module paths)
