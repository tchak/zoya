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
use zoya_ir::CheckedItem;
use zoya_loader::ModulePath;

// Get the standard library (lazily compiled and cached)
let std_pkg = std();

// Access the option module
let option_path = ModulePath::root().child("option");
let option_module = std_pkg.get(&option_path).unwrap();

// Inspect its items
for item in &option_module.items {
    match item {
        CheckedItem::Enum(e) => println!("enum {}", e.name),
        _ => {}
    }
}
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
