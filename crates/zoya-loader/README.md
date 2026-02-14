# zoya-loader

Package file loading for the Zoya programming language.

Handles reading, parsing, and organizing Zoya source files into a package. Supports both filesystem and in-memory sources for flexibility in testing and tooling.

## Features

- **Recursive module loading** - Follows `mod` declarations to build complete packages
- **Pluggable sources** - `FsSource` for filesystem, `MemorySource` for testing
- **Compilation modes** - `Dev`, `Test`, and `Release` modes for filtering `#[test]` and `#[mode(test)]` items
- **Module name validation** - Enforces `snake_case` module names, rejects reserved names
- **Error handling** - Detailed errors for missing modules, duplicates, invalid names, and parse failures

## Usage

```rust
use std::path::Path;
use zoya_loader::{load_package, load_memory_package, MemorySource, Mode, QualifiedPath};

// Load from filesystem (file, directory, or package.toml)
let pkg = load_package(Path::new("src/main.zy"), Mode::Dev)?;

// Load in test mode (includes #[test] items)
let pkg = load_package(Path::new("src/main.zy"), Mode::Test)?;

// Access loaded modules
let root = pkg.root().unwrap();
println!("Root module has {} items", root.items.len());
for (name, (child_path, visibility)) in &root.children {
    println!("  Child module: {} ({})", name, child_path);
}

// In-memory source for testing
let source = MemorySource::new()
    .with_module("root", "mod utils\nfn main() -> Int { 42 }")
    .with_module("utils", "pub fn helper() -> Int { 10 }");
let pkg = load_memory_package(&source, Mode::Dev)?;
```

## Compilation Modes

| Mode | Description |
|------|-------------|
| `Mode::Dev` | Development mode — excludes `#[test]` items (default) |
| `Mode::Test` | Test mode — includes all items including `#[test]` |
| `Mode::Release` | Release mode — excludes `#[test]` items |

## Module Resolution

Given a file `main.zy` containing `mod utils`, the loader looks for:
- `utils.zy` in the same directory as `main.zy`

For nested modules like `mod helpers` inside `utils.zy`:
- `utils/helpers.zy` relative to the base directory

Module names must be valid `snake_case` identifiers and not reserved names (`root`, `self`, `super`, `std`, `zoya`).

## Error Types

```rust
use zoya_loader::{load_package, LoaderError, Mode};

match load_package(Path::new("missing.zy"), Mode::Dev) {
    Err(LoaderError::SourceError { path, error }) => {
        println!("Failed to read {}: {}", path, error);
    }
    Err(LoaderError::ModuleNotFound { mod_name, expected_path }) => {
        println!("Module '{}' not found at {}", mod_name, expected_path);
    }
    Err(LoaderError::DuplicateMod { mod_name }) => {
        println!("Duplicate module declaration: {}", mod_name);
    }
    Err(LoaderError::InvalidModName { mod_name, suggestion }) => {
        println!("Invalid module name '{}', try '{}'", mod_name, suggestion);
    }
    Err(LoaderError::ReservedModName { mod_name }) => {
        println!("Reserved module name: {}", mod_name);
    }
    Err(LoaderError::LexError { path, message }) => {
        println!("Lexer error in {}: {}", path, message);
    }
    Err(LoaderError::ParseError { path, message }) => {
        println!("Parse error in {}: {}", path, message);
    }
    Err(e) => println!("Error: {}", e),
    Ok(pkg) => { /* success */ }
}
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-naming](../zoya-naming) - Name validation
- [zoya-package](../zoya-package) - Package data structures
- [zoya-parser](../zoya-parser) - Parser
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
