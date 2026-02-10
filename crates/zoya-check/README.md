# zoya-check

Type checker for the Zoya programming language.

Implements Hindley-Milner type inference with constraint-based unification, transforming untyped AST into fully typed IR.

## Features

- **Type inference** - Hindley-Milner algorithm with let-polymorphism
- **Unification** - Constraint-based type unification with occurs check
- **Pattern exhaustiveness** - Maranget algorithm for match completeness
- **Cross-module checking** - Handles imports, qualified paths, and `pub use` re-exports
- **Visibility enforcement** - Validates public/private access across modules
- **Naming conventions** - Enforces PascalCase/snake_case at compile time
- **Generics** - Parametric polymorphism for structs, enums, and functions
- **Multi-package support** - Type-check against dependency packages (e.g., standard library)

## Usage

```rust
use zoya_check::check;
use zoya_loader::load_package;
use zoya_std::std;
use std::path::Path;

// Load and type-check a package with standard library
let std = std();
let pkg = load_package(Path::new("src/main.zy"))?;
let checked_pkg = check(&pkg, &[std])?;

// Access checked functions
let root = checked_pkg.root().unwrap();
for f in &root.items {
    println!("{}: {}", f.name, f.return_type);
}

// Access type definitions
for (path, def) in &checked_pkg.definitions {
    println!("{}: {}", path, def.kind_name());
}
```

## Type Checking Pipeline

1. **Dependency injection** - Load definitions from dependency packages (e.g., std)
2. **Declaration registration** - Register all type/function signatures across modules
3. **Import resolution** - Resolve `use` and `pub use` declarations to qualified paths
4. **Visibility checking** - Validate that private items are not accessed from other modules
5. **Body checking** - Type-check function bodies with inference
6. **Exhaustiveness checking** - Verify pattern match coverage

## Dependencies

- [zoya-ast](../zoya-ast) - Untyped AST types
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-naming](../zoya-naming) - Name validation and conventions
- [zoya-package](../zoya-package) - Package data structures
