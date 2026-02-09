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
```

The standard library is a `&'static CheckedPackage` - it is compiled once and cached for the lifetime of the process.

## Module Structure

```
root
├── option    # Option<T> enum
└── result    # Result<T, E> enum
```

## Dependencies

- [zoya-check](../zoya-check) - Type checker (compiles the .zoya sources)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-loader](../zoya-loader) - Package loading (via `MemorySource`)
