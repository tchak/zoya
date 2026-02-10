# zoya-codegen

JavaScript code generation for the Zoya programming language.

Transforms typed IR into executable JavaScript code with ESM exports.

## Features

- **Package codegen** - Generates JS for complete packages with all modules
- **Pattern compilation** - Compiles patterns to JS conditionals and bindings
- **Function generation** - Handles generic functions, lambdas, and closures
- **Enum encoding** - Tagged objects with `$tag` field for variant discrimination
- **Runtime helpers** - Prelude functions for deep equality, division checks, BigInt operations

## Usage

```rust
use zoya_check::check;
use zoya_codegen::codegen;
use zoya_loader::load_package;
use zoya_std::std;
use std::path::Path;

// Load and type-check with standard library
let std = std();
let pkg = load_package(Path::new("src/main.zoya"))?;
let checked_pkg = check(&pkg, &[std])?;

// Generate JavaScript
let output = codegen(&checked_pkg);

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

## CodegenOutput

```rust
pub struct CodegenOutput {
    /// Generated JavaScript code (ESM format)
    pub code: String,
    /// Blake3 hash of the code (64 hex chars)
    pub hash: String,
}
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for operators)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-package](../zoya-package) - Module path types
- [blake3](https://github.com/BLAKE3-team/BLAKE3) - Content hashing
