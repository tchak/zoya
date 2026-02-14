# zoya-package

Package data structures for the Zoya programming language.

This crate defines the core package-related types used across the Zoya compiler for organizing modules into a coherent compilation unit.

## Types

- **QualifiedPath** - Qualified path to a module, definition, or variant (e.g., `root::utils::helpers`)
- **Module** - A loaded module containing parsed items and child references with visibility
- **Package** - The complete package of loaded modules with name and optional output path
- **PackageConfig** - Package configuration loaded from `package.toml`

## Usage

```rust
use zoya_package::{QualifiedPath, Module, Package, PackageConfig};
use std::collections::HashMap;

// Create module paths
let root = QualifiedPath::root();
let utils = root.child("utils");
let helpers = utils.child("helpers");

// Check path relationships
assert_eq!(root, QualifiedPath::root());
assert_ne!(utils, QualifiedPath::root());
assert_eq!(helpers.parent(), Some(utils.clone()));
assert_eq!(helpers.depth(), 3);

// Path display
assert_eq!(root.to_string(), "root");
assert_eq!(helpers.to_string(), "root::utils::helpers");

// Replace root segment (e.g., for std library remapping)
let std_path = helpers.with_root("std");
assert_eq!(std_path.to_string(), "std::utils::helpers");

// Build a package manually (typically done by zoya-loader)
let mut modules = HashMap::new();
modules.insert(QualifiedPath::root(), Module {
    items: vec![],
    path: QualifiedPath::root(),
    children: HashMap::new(),
});
let pkg = Package {
    name: "my_project".to_string(),
    output: None,
    modules,
};

// Access modules
if let Some(root_module) = pkg.root() {
    println!("Root has {} items", root_module.items.len());
}
```

## Package Configuration

Load and create `package.toml` files:

```rust
use zoya_package::PackageConfig;
use std::path::Path;

// Load from directory
let config = PackageConfig::load(Path::new("my_project"))?;
println!("Package: {}", config.name);
println!("Entry: {}", config.main_path().display());

// Get the module name (hyphens replaced with underscores)
let config_name = "my-project";
// config.module_name() -> "my_project"

// Validate package names
assert!(PackageConfig::is_valid_name("my_project"));   // OK
assert!(!PackageConfig::is_valid_name("My-Project"));   // Invalid

// Sanitize names
assert_eq!(PackageConfig::sanitize_name("My-Project"), "my_project");

// Serialize to TOML
let toml = config.to_toml();
```

### package.toml Format

```toml
[package]
name = "my_project"            # required
main = "src/main.zy"           # optional (default: src/main.zy)
output = "build"               # optional (default: build)
```

## QualifiedPath Methods

| Method | Description |
|--------|-------------|
| `root()` | Create the root module path |
| `child(name)` | Create a child path |
| `parent()` | Get parent path (None for root) |
| `is_root()` | Check if this is the root path |
| `depth()` | Number of path segments |
| `segments()` | Get path segments as slice |
| `head()` | Get the first segment |
| `tail()` | Get all segments after the first |
| `last()` | Get the last segment |
| `with_root(name)` | Replace the root segment with a new name |

## PackageConfig Methods

| Method | Description |
|--------|-------------|
| `load(dir)` | Load from a directory's `package.toml` |
| `load_from(path)` | Load from a specific file path |
| `to_toml()` | Serialize to TOML string |
| `main_path()` | Get entry file path (default: `src/main.zy`) |
| `output_path()` | Get output directory (default: `build`) |
| `module_name()` | Get module name (hyphens to underscores) |
| `is_valid_name(name)` | Check if name is valid |
| `sanitize_name(input)` | Sanitize string to valid package name |

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for Item and Visibility)
- [zoya-naming](../zoya-naming) - Name validation and reserved names
- [serde](https://github.com/serde-rs/serde) - Serialization
- [toml](https://github.com/toml-rs/toml) - TOML parsing
