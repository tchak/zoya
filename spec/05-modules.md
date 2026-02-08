# Modules

Zoya programs are organized into a tree of modules. Each module has a path, a set of items, and zero or more child modules. Modules control namespace organization and visibility boundaries.

## Module Tree

Every program has a single root module. All other modules are descendants of the root, forming a tree:

```
root                    (main.zoya)
├── utils               (utils.zoya)
│   └── helpers         (utils/helpers.zoya)
├── types               (types.zoya)
└── app                 (app.zoya)
```

The root module has the path `root`. Child modules extend their parent's path: `root::utils`, `root::utils::helpers`, etc.

## File-to-Module Mapping

Each module corresponds to a single source file. The root module is the entry point file (typically `main.zoya`). Submodule files are located relative to their parent:

| Module path | Parent file | Expected file |
|-------------|-------------|---------------|
| `root` | — | `main.zoya` |
| `root::utils` | `main.zoya` | `utils.zoya` |
| `root::utils::helpers` | `utils.zoya` | `utils/helpers.zoya` |
| `root::app` | `main.zoya` | `app.zoya` |

A `mod` declaration in a parent module causes the compiler to load the corresponding file:

```zoya
// main.zoya (root module)
pub mod utils       // loads utils.zoya
mod internal        // loads internal.zoya
```

```zoya
// utils.zoya (root::utils)
pub mod helpers     // loads utils/helpers.zoya
```

A module name must not be declared more than once in the same parent module.

## Module Paths

Every item in a program has a fully qualified path: the module path followed by the item name. For example, a function `helper` defined in `root::utils` has the qualified path `root::utils::helper`.

### Path Prefixes

Paths in expressions and use declarations use a prefix to anchor resolution:

| Prefix | Resolution |
|--------|------------|
| `root::` | Absolute path from the root module |
| `self::` | Relative to the current module |
| `super::` | Relative to the parent module |
| *(none)* | Relative to the current module (expressions only) |

```zoya
// In module root::app
use root::utils::helper     // resolves to root::utils::helper
use self::types::Config     // resolves to root::app::types::Config
use super::shared           // resolves to root::shared
```

Using `super::` in the root module is an error because the root has no parent.

### Prefix-Free Paths

In expressions, paths without a prefix resolve relative to the current module:

```zoya
// In module root::math
fn double(x: Int) -> Int x * 2

fn quadruple(x: Int) -> Int double(double(x))  // resolves to root::math::double
```

Use declarations require an explicit prefix. Prefix-free use paths are rejected:

```zoya
use utils::helper           // Error: use root::, self::, or super:: prefix
use root::utils::helper     // OK
```

## Name Resolution

When a name is referenced in an expression, the compiler resolves it by searching in this order:

1. **Local bindings** — `let` bindings, function parameters, lambda parameters, and match arm bindings in the enclosing scopes
2. **Item imports** — names brought into scope by `use` declarations (single, glob, or group)
3. **Module-level definitions** — items defined in the current module (`fn`, `struct`, `enum`, `type`)

The first match wins. A local binding shadows an import, and an import shadows a module-level definition:

```zoya
fn value() -> Int 1

// 'value' here refers to the function defined above
fn example() -> Int value()
```

```zoya
use root::other::value      // import shadows any local definition named 'value'

fn example() -> Int value()   // calls root::other::value
```

```zoya
fn example() -> Int {
    let value = 42;           // local shadows the import
    value                     // evaluates to 42
}
```

### Multi-Segment Path Resolution

For paths with more than one segment and no prefix (e.g., `Color::Red`, `bar::add`), the compiler checks two additional sources before falling through to full path resolution:

1. **Item imports** — if the first segment matches an imported item, the remaining segments are appended to the imported item's qualified path. For example, if `Color` is imported as `root::types::Color`, then `Color::Red` resolves to `root::types::Color::Red`.

2. **Module imports** — if the first segment matches a module import, the remaining segments are appended to the module path. For example, if `bar` is imported as module `root::foo::bar`, then `bar::add` resolves to `root::foo::bar::add`.

```zoya
use root::types::Color        // item import
use root::utils               // module import

fn example() -> Int {
    let c = Color::Red;       // resolves to root::types::Color::Red (via item import)
    utils::helper()           // resolves to root::utils::helper (via module import)
}
```

## Imports

A `use` declaration imports names into the current module's scope. There are three forms: single imports, glob imports, and group imports.

```
use_decl    ::= visibility 'use' use_path
use_path    ::= path_prefix identifier ('::' identifier)* use_suffix?
use_suffix  ::= '::' '*' | '::' '{' use_group '}'
use_group   ::= identifier (',' identifier)* ','?
path_prefix ::= 'root' '::' | 'self' '::' | 'super' '::'
```

See [Definitions](04-definitions.md#use-declarations) for the full grammar.

### Single Imports

A single import brings one item or module into scope. The local name is the last segment of the path.

```zoya
use root::utils::helper           // imports item 'helper'
use self::types::Config           // imports item 'Config'
```

If the path resolves to a module rather than an item, the module is imported as a namespace. Items within it can then be accessed with qualified paths:

```zoya
// Module: root
mod math

fn main() -> Int {
    math::add(1, 2)               // qualified access through module import
}
```

```zoya
use root::math                    // imports module 'math' as namespace

fn main() -> Int {
    math::add(1, 2)               // same qualified access
}
```

Module imports also work with deeper paths:

```zoya
use root::math

fn main() -> String {
    match math::Color::Red {      // math::Color::Red resolves through module import
        math::Color::Red => "red",
        _ => "other",
    }
}
```

### Glob Imports

A glob import brings all public items from a module into scope:

```zoya
use root::math::*                 // imports all public items from root::math
```

Private items are silently skipped. Enum variants are not imported directly — only the enum type itself is imported:

```zoya
// Module: root::types
pub enum Color { Red, Green, Blue }
pub fn helper() -> Int 42
fn secret() -> Int 0              // private, not imported

// Module: root
use root::types::*                // imports Color and helper

fn main() -> String {
    let c = Color::Red;           // Color imported, variants accessed through it
    helper()                      // helper imported
}
```

### Group Imports

A group import brings multiple named items from a single module into scope:

```zoya
use root::math::{add, subtract}  // imports add and subtract from root::math
```

A trailing comma is permitted:

```zoya
use root::math::{add, subtract,}
```

An empty group `::{}` is a parse error. Each named item must exist and be visible:

```zoya
// Module: root::math
pub fn add(x: Int, y: Int) -> Int x + y
fn secret() -> Int 0

// Module: root
use root::math::{add, secret}    // Error: 'root::math::secret' is private
```

### Import Visibility Checks

An import succeeds only if:

1. **The target item exists** at the resolved path.
2. **The target item is visible** to the importing module (see [Visibility](#visibility)).
3. **All intermediate modules are visible** to the importing module.

```zoya
// Module: root
pub mod api
mod internal

// Module: root::api
pub fn endpoint() -> Int 200

// Module: root::internal
pub fn secret() -> Int 42

// Module: root::consumer
use root::api::endpoint           // OK: api is public, endpoint is public
use root::internal::secret        // Error: module 'internal' is private
```

### Duplicate Detection

Importing the same name twice — whether through single imports, glob imports, group imports, or any combination — is an error:

```zoya
use root::a::helper
use root::b::helper               // Error: 'helper' is already imported

use root::math::{add, add}        // Error: 'add' is already imported
```

### Re-exports

A `pub use` declaration re-exports imported names, making them available through the current module. All three import forms support `pub`:

```zoya
// Module: root::prelude
pub use root::option::Option           // re-export single item
pub use root::collections::*           // re-export all public items
pub use root::math::{add, subtract}    // re-export specific items
```

```zoya
// Module: root::app
use root::prelude::Option         // OK: Option is re-exported by prelude
use root::prelude::add            // OK: add is re-exported by prelude
```

A `pub use` can only re-export items that are themselves public. Re-exporting a private item is an error:

```zoya
// Module: root::a
fn secret() -> Int 42             // private

// Module: root::b
pub use root::a::secret           // Error: cannot re-export private item
```

Re-exporting a module with `pub use` is not yet supported:

```zoya
// Module: root::b
pub use root::a                   // Error: re-exporting modules is not yet supported
```

## Visibility

Every item and module declaration has a visibility: **private** (default) or **public** (`pub`).

See [Definitions](04-definitions.md#visibility) for the full rules, access table, and visibility consistency requirements.

### Summary

- **Private** items are visible from the defining module and all its descendants.
- **Public** items are visible from any module.
- Public items may only reference public types in their signatures.
- Enum variants inherit the visibility of their parent enum.

### Module Visibility

Module declarations also have visibility. To access an item through a path like `root::a::b::item`, every module along the path (`a` and `b`) must be visible to the accessor:

```zoya
// Module: root
pub mod api
mod internal
pub mod consumer

// Module: root::internal
pub mod deep

// Module: root::internal::deep
pub fn hidden() -> Int 42

// Module: root::consumer
use root::internal::deep::hidden  // Error: module 'internal' is private
```

Even though `deep` and `hidden` are public, the private `internal` module blocks access.
