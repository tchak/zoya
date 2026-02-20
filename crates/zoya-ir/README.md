# zoya-ir

Typed Intermediate Representation for the Zoya programming language.

This crate defines the type-checked IR produced by the type checker. All expressions, patterns, and items carry resolved type information.

## Types

- **Primitive types** - `Int`, `BigInt`, `Float`, `Bool`, `String`
- **Compound types** - `List<T>`, `Set<T>`, `Dict<K, V>`, tuples `(T, U, ...)`, functions `T -> U`
- **User-defined types** - Structs, enums with generics
- **Type variables** - For inference and polymorphism
- **Visibility** - Re-exported from `zoya-ast` for use throughout the compiler

## Usage

```rust
use zoya_ir::{Type, TypedExpr, Definition, CheckedPackage, Visibility, QualifiedPath};

// Type represents resolved types
let list_int = Type::List(Box::new(Type::Int));
let dict = Type::Dict(Box::new(Type::String), Box::new(Type::Int));
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

// CheckedPackage contains all type-checked modules and definitions
fn process_package(pkg: &CheckedPackage) {
    // Iterate over functions
    for (path, func) in &pkg.items {
        println!("fn {}: {:?}", path, func.return_type);
    }

    // Inspect type definitions
    for (path, def) in &pkg.definitions {
        match def {
            Definition::Struct(s) => println!("struct at {}", path),
            Definition::Enum(e) => println!("enum at {}", path),
            Definition::ImplMethod(m) => println!("method at {}", path),
            _ => {}
        }
    }

    // Query HTTP routes
    for (path, method, pathname) in pkg.routes() {
        println!("{} {} -> {}", method.attr_name(), pathname.as_str(), path);
    }

    // Query task functions
    for path in pkg.tasks() {
        println!("task: {}", path);
    }

    // Query public functions
    for path in pkg.fns() {
        println!("fn: {}", path);
    }
}
```

## Key Types

| Type | Description |
|------|-------------|
| `Type` | Resolved type (Int, List<T>, Set<T>, Dict<K, V>, structs, enums, functions) |
| `TypedExpr` | Expression with attached type information |
| `TypedPattern` | Pattern with resolved types for codegen |
| `TypedFunction` | Function with typed body, return type, and `FunctionKind` |
| `FunctionKind` | Function classification: `Regular`, `Builtin`, `Test`, `Task`, `Http(HttpMethod, Pathname)` |
| `HttpMethod` | HTTP method: `Get`, `Post`, `Put`, `Patch`, `Delete` |
| `Pathname` | Validated URL pathname for HTTP routes |
| `CheckedPackage` | Complete package of checked items, definitions, re-exports, and imports |
| `QualifiedPath` | Fully resolved path (e.g., `root::utils::helper`) |
| `Definition` | Type definition: Function, Struct, Enum, EnumVariant, TypeAlias, Module, ImplMethod |
| `FunctionType` | Function signature with visibility, module, type params |
| `StructType` | Struct definition with visibility, module, fields |
| `EnumType` | Enum definition with visibility, module, variants |
| `TypeAliasType` | Type alias with visibility, module, underlying type |
| `ImplMethodType` | Method definition with target type, self param, type params |
| `TypeScheme` | Polymorphic type with quantified type variables |
| `Visibility` | Re-exported `Private`/`Public` enum from `zoya-ast` |

## Dependencies

- [zoya-ast](../zoya-ast) - Untyped AST types (for operators and shared definitions)
- [zoya-package](../zoya-package) - Package data structures (for module paths)
