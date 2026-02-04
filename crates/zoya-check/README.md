# zoya-check

Type checker for the Zoya programming language.

Implements Hindley-Milner type inference with constraint-based unification, transforming untyped AST into fully typed IR.

## Features

- **Type inference** - Hindley-Milner algorithm with let-polymorphism
- **Unification** - Constraint-based type unification with occurs check
- **Pattern exhaustiveness** - Maranget algorithm for match completeness
- **Cross-module checking** - Handles imports, visibility, and qualified paths
- **Generics** - Parametric polymorphism for structs, enums, and functions

## Usage

```rust
use zoya_check::check;
use zoya_loader::load_package;

// Load and type-check a package
let pkg = load_package(Path::new("src/main.zoya"))?;
let checked_pkg = check(&pkg)?;

// Access checked modules
let root = checked_pkg.root().unwrap();
for item in &root.items {
    match item {
        CheckedItem::Function(f) => println!("{}: {}", f.name, f.return_type),
        CheckedItem::Struct(s) => println!("struct {}", s.name),
        CheckedItem::Enum(e) => println!("enum {}", e.name),
        CheckedItem::TypeAlias(t) => println!("type {}", t.name),
    }
}
```

## Type Checking Pipeline

1. **Declaration registration** - Register all type/function signatures
2. **Import resolution** - Resolve `use` declarations to qualified paths
3. **Body checking** - Type-check function bodies with inference
4. **Exhaustiveness checking** - Verify pattern match coverage

## Dependencies

- [zoya-ast](../zoya-ast) - Untyped AST types
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-package](../zoya-package) - Package data structures
