# zoya-build

Build orchestration for the Zoya programming language.

Coordinates the three-stage compilation pipeline: package loading, type checking, and JavaScript code generation. Produces a `BuildOutput` containing generated code and metadata about functions, tests, jobs, and HTTP routes.

## Features

- **Unified pipeline** - Single entry point for load + type-check + codegen
- **Path-based and package-based APIs** - Build from a filesystem path or a pre-loaded `Package`
- **Check-only mode** - Type-check without generating code
- **Metadata extraction** - Collects functions, tests, jobs, and HTTP routes from checked packages
- **Serializable output** - `BuildOutput` implements `Serialize`/`Deserialize` for caching

## Usage

### Build from a file path

```rust
use zoya_build::{build_from_path, BuildOutput};
use zoya_loader::Mode;
use std::path::Path;

// Build a package (load, type-check, codegen)
let output = build_from_path(Path::new("src/main.zy"), Mode::Dev)?;

// Access generated JavaScript
println!("Generated {} bytes of JS", output.output.code.len());

// Inspect discovered metadata
println!("Functions: {}", output.functions.len());
println!("Tests: {}", output.tests.len());
println!("Jobs: {}", output.jobs.len());
println!("HTTP routes: {}", output.routes.len());
```

### Build from a loaded package

```rust
use zoya_build::build;
use zoya_loader::{load_package, Mode};
use std::path::Path;

// Load separately, then build
let package = load_package(Path::new("my_project"), Mode::Dev)?;
let output = build(&package)?;
```

### Type-check only

```rust
use zoya_build::{check_from_path, check};
use zoya_loader::Mode;
use std::path::Path;

// Check from path
check_from_path(Path::new("src/main.zy"), Mode::Dev)?;

// Or check a pre-loaded package
let package = zoya_loader::load_package(Path::new("my_project"), Mode::Dev)?;
check(&package)?;
```

## Public API

```rust
/// Build output containing generated code and package metadata.
pub struct BuildOutput {
    pub name: String,
    pub output: CodegenOutput,
    pub definitions: DefinitionLookup,
    pub functions: Vec<(QualifiedPath, Vec<String>)>,
    pub tests: Vec<QualifiedPath>,
    pub jobs: Vec<(QualifiedPath, String)>,
    pub routes: Vec<(QualifiedPath, HttpMethod, Pathname)>,
}

/// Load, type-check, and generate code from a filesystem path.
pub fn build_from_path(path: &Path, mode: Mode) -> Result<BuildOutput, BuildError>;

/// Type-check and generate code from a pre-loaded package.
pub fn build(package: &Package) -> Result<BuildOutput, BuildError>;

/// Load and type-check from a filesystem path (no codegen).
pub fn check_from_path(path: &Path, mode: Mode) -> Result<(), BuildError>;

/// Type-check a pre-loaded package (no codegen).
pub fn check(package: &Package) -> Result<(), BuildError>;
```

## Error Handling

```rust
/// Unified error for loading and type-checking failures.
pub enum BuildError {
    /// Package loading failed (file IO, lex, parse, config)
    Load(zoya_loader::LoaderError),
    /// Type checking failed
    Check(zoya_ir::TypeError),
}
```

`BuildError` implements `From` for both `LoaderError` and `TypeError`, enabling `?` propagation from either stage.

## Dependencies

- [zoya-check](../zoya-check) - Type checker
- [zoya-codegen](../zoya-codegen) - JavaScript code generation
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-loader](../zoya-loader) - Package file loading
- [zoya-package](../zoya-package) - Package data structures
- [zoya-std](../zoya-std) - Standard library
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
