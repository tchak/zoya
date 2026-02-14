# zoya-codegen

JavaScript code generation for the Zoya programming language.

Transforms typed IR into executable JavaScript code.

## Features

- **Package codegen** - Generates JS for complete packages with all modules and dependencies
- **Pattern compilation** - Compiles patterns to JS conditionals and bindings
- **Function generation** - Handles generic functions, lambdas, closures, and methods
- **Enum encoding** - Tagged objects with `$tag` field for variant discrimination
- **Impl methods** - Generates method dispatch for user-defined and primitive types
- **String interpolation** - Compiles to JS template literals
- **Runtime helpers** - Prelude functions for deep equality, division checks, BigInt operations, Dict operations

## Usage

```rust
use zoya_check::check;
use zoya_codegen::codegen;
use zoya_loader::{load_package, Mode};
use zoya_std::std;
use std::path::Path;

// Load and type-check with standard library
let std = std();
let pkg = load_package(Path::new("src/main.zy"), Mode::Dev)?;
let checked_pkg = check(&pkg, &[std])?;

// Generate JavaScript (pass dependencies for cross-package codegen)
let output = codegen(&checked_pkg, &[std]);

// output.code contains the generated JavaScript
// output.hash is a Blake3 hash for caching/deduplication
println!("Generated {} bytes of JS", output.code.len());
println!("Content hash: {}", output.hash);

// Write to file
std::fs::write("output.js", &output.code)?;
```

## Generated Code Structure

```javascript
// Runtime helpers (prelude)
function $$eq(a, b) { /* deep equality */ }
function $$div(a, b) { /* checked division */ }
// ... more helpers

// User functions with qualified names
export function $root$main() { return 42; }
export function $root$utils$helper($x) { return ($x + 1); }
```

## Public API

```rust
pub struct CodegenOutput {
    /// Generated JavaScript code
    pub code: String,
    /// Blake3 hash of the code (64 hex chars)
    pub hash: String,
}

/// Generate JavaScript for a checked package with its dependencies.
pub fn codegen(package: &CheckedPackage, deps: &[&CheckedPackage]) -> CodegenOutput;

/// Format a qualified path as a JS export name (e.g., `$root$utils$helper`).
pub fn format_export_path(path: &QualifiedPath, pkg_name: &str) -> String;
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for operators)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-package](../zoya-package) - Module path types
- [blake3](https://github.com/BLAKE3-team/BLAKE3) - Content hashing
