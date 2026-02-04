# zoya-loader

Package file loading for the Zoya programming language.

Handles reading, parsing, and organizing Zoya source files into a package. Supports both filesystem and in-memory sources for flexibility in testing and tooling.

## Features

- **Recursive module loading** - Follows `mod` declarations to build complete packages
- **Pluggable sources** - `FsSource` for filesystem, `MemorySource` for testing
- **Error handling** - Detailed errors for missing modules, duplicates, and parse failures

## Usage

```rust
use std::path::Path;
use zoya_loader::{load_package, load_package_with, MemorySource, ModulePath};

// Load from filesystem
let pkg = load_package(Path::new("src/main.zoya"))?;

// Access loaded modules
let root = pkg.root().unwrap();
println!("Root module has {} items", root.items.len());
for (name, child_path) in &root.children {
    println!("  Child module: {}", name);
}

// In-memory source for testing
let source = MemorySource::new()
    .with_module("root", "mod utils\nfn main() -> Int { 42 }")
    .with_module("utils", "pub fn helper() -> Int { 10 }");
let pkg = load_package_with(&source, &"root".to_string())?;
```

## Module Resolution

Given a file `main.zoya` containing `mod utils`, the loader looks for:
- `utils.zoya` in the same directory as `main.zoya`

For nested modules like `mod helpers` inside `utils.zoya`:
- `utils/helpers.zoya` relative to the base directory

## Error Types

```rust
use zoya_loader::{load_package, LoaderError};

match load_package(Path::new("missing.zoya")) {
    Err(LoaderError::SourceError { path, error }) => {
        println!("Failed to read {}: {}", path, error);
    }
    Err(LoaderError::ModuleNotFound { mod_name, expected_path }) => {
        println!("Module '{}' not found at {}", mod_name, expected_path);
    }
    Err(LoaderError::DuplicateMod { mod_name }) => {
        println!("Duplicate module declaration: {}", mod_name);
    }
    Err(LoaderError::LexError { path, message }) => {
        println!("Lexer error in {}: {}", path, message);
    }
    Err(LoaderError::ParseError { path, message }) => {
        println!("Parse error in {}: {}", path, message);
    }
    Ok(pkg) => { /* success */ }
}
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-package](../zoya-package) - Package data structures
- [zoya-parser](../zoya-parser) - Parser
