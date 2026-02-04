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
use zoya_loader::{load_package, load_package_with, FsSource, MemorySource};

// Load from filesystem
let pkg = load_package(Path::new("src/main.zoya"))?;

// Load with custom source
let source = FsSource::new("/project/src");
let pkg = load_package_with(&source, &FilePath::new("/project/src/main.zoya"))?;

// In-memory source for testing
let source = MemorySource::new()
    .with_module("root", "mod utils\nfn main() -> Int 42")
    .with_module("utils", "fn helper() -> Int 10");
let pkg = load_package_with(&source, &"root".to_string())?;
```

## Module Resolution

Given a file `main.zoya` containing `mod utils`, the loader looks for:
- `utils.zoya` in the same directory as `main.zoya`

For nested modules like `mod helpers` inside `utils.zoya`:
- `utils/helpers.zoya` relative to the base directory

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-package](../zoya-package) - Package data structures
- [zoya-parser](../zoya-parser) - Parser
