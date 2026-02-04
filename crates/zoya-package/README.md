# zoya-package

Package data structures for the Zoya programming language.

This crate defines the core package-related types used across the Zoya compiler:

- **ModulePath** - Logical path to a module in the package (e.g., `root::utils::helpers`)
- **Module** - A loaded module containing parsed items and child module references
- **Package** - The complete package of loaded modules

## Usage

```rust
use zoya_package::{ModulePath, Module, Package};

// Create a root path
let root = ModulePath::root();

// Create nested paths
let utils = root.child("utils");
let helpers = utils.child("helpers");

// Check relationships
assert!(root.is_root());
assert_eq!(helpers.parent(), Some(utils));
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for Item definitions)
