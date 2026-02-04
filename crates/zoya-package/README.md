# zoya-package

Package data structures for the Zoya programming language.

This crate defines the core package-related types used across the Zoya compiler for organizing modules into a coherent compilation unit.

## Types

- **ModulePath** - Logical path to a module (e.g., `root::utils::helpers`)
- **Module** - A loaded module containing parsed items and child references
- **Package** - The complete package of loaded modules

## Usage

```rust
use zoya_package::{ModulePath, Module, Package};
use std::collections::HashMap;

// Create module paths
let root = ModulePath::root();
let utils = root.child("utils");
let helpers = utils.child("helpers");

// Check path relationships
assert!(root.is_root());
assert!(!utils.is_root());
assert_eq!(helpers.parent(), Some(utils.clone()));
assert_eq!(helpers.depth(), 3);

// Path display
assert_eq!(root.to_string(), "root");
assert_eq!(helpers.to_string(), "root::utils::helpers");

// Build a package manually (typically done by zoya-loader)
let mut modules = HashMap::new();
modules.insert(ModulePath::root(), Module {
    items: vec![],
    uses: vec![],
    path: ModulePath::root(),
    children: HashMap::new(),
});
let pkg = Package { modules };

// Access modules
if let Some(root_module) = pkg.root() {
    println!("Root has {} items", root_module.items.len());
}
```

## ModulePath Methods

| Method | Description |
|--------|-------------|
| `root()` | Create the root module path |
| `child(name)` | Create a child path |
| `parent()` | Get parent path (None for root) |
| `is_root()` | Check if this is the root path |
| `depth()` | Number of path segments |
| `segments()` | Get path segments as slice |

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for Item and UseDecl)
