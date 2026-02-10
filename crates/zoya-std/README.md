# zoya-std

Standard library for the Zoya programming language.

Provides built-in type definitions (`Option`, `Result`) as a pre-compiled and cached `CheckedPackage`. The standard library is written in Zoya itself and compiled at startup.

## Included Types

### Option\<T\>

```zoya
pub enum Option<T> { None, Some(T) }
```

### Result\<T, E\>

```zoya
pub enum Result<T, E> { Ok(T), Err(E) }
```

### Prelude

The prelude module re-exports all standard types and variants for automatic injection:

```zoya
pub use root::option::*   // Option, Some, None
pub use root::result::*   // Result, Ok, Err
```

When user packages are type-checked with the standard library as a dependency, prelude definitions are automatically available without explicit imports.

## Usage

```rust
use zoya_std::std;
use zoya_ir::Definition;
use zoya_loader::QualifiedPath;

// Get the standard library (lazily compiled and cached)
let std_pkg = std();

// Access the Option enum definition
let option_path = QualifiedPath::root().child("option").child("Option");
let def = std_pkg.definitions.get(&option_path).unwrap();
assert!(matches!(def, Definition::Enum(_)));

// Pass as dependency to the type checker
use zoya_check::check;
let checked = check(&user_pkg, &[std_pkg])?;
```

The standard library is a `&'static CheckedPackage` - it is compiled once and cached for the lifetime of the process.

## Module Structure

```
root
├── option    # Option<T> enum with Some and None variants
├── prelude   # Re-exports all types and variants for auto-injection
└── result    # Result<T, E> enum with Ok and Err variants
```

## Dependencies

- [zoya-check](../zoya-check) - Type checker (compiles the .zoya sources)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-loader](../zoya-loader) - Package loading (via `MemorySource`)
