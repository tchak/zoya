# Modules

Zoya programs are organized into a tree of modules. Each module has a path, a set of items, and zero or more child modules. Modules control namespace organization and visibility boundaries.

## Module Tree

Every program has a single root module. All other modules are descendants of the root, forming a tree:

```
root                    (main.zy)
├── utils               (utils.zy)
│   └── helpers         (utils/helpers.zy)
├── types               (types.zy)
└── app                 (app.zy)
```

The root module has the path `root`. Child modules extend their parent's path: `root::utils`, `root::utils::helpers`, etc.

## File-to-Module Mapping

Each module corresponds to a single source file. The root module is the entry point file (typically `main.zy`). Submodule files are located relative to their parent:

| Module path | Parent file | Expected file |
|-------------|-------------|---------------|
| `root` | — | `main.zy` |
| `root::utils` | `main.zy` | `utils.zy` |
| `root::utils::helpers` | `utils.zy` | `utils/helpers.zy` |
| `root::app` | `main.zy` | `app.zy` |

A `mod` declaration in a parent module causes the compiler to load the corresponding file:

```zoya
// main.zy (root module)
pub mod utils       // loads utils.zy
mod internal        // loads internal.zy
```

```zoya
// utils.zy (root::utils)
pub mod helpers     // loads utils/helpers.zy
```

A module name must not be declared more than once in the same parent module. Module names must not be reserved names (`root`, `self`, `super`, `std`, `zoya`), as these conflict with path prefixes or the standard library.

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

A glob import brings all public items from a module into scope, including public child modules:

```zoya
use root::math::*                 // imports all public items and modules from root::math
```

Private items are silently skipped. Enum variants are not imported directly — only the enum type itself is imported. Public child modules are imported as namespaces:

```zoya
// Module: root::types
pub enum Color { Red, Green, Blue }
pub fn helper() -> Int 42
pub mod extras
fn secret() -> Int 0              // private, not imported

// Module: root
use root::types::*                // imports Color, helper, and extras

fn main() -> String {
    let c = Color::Red;           // Color imported, variants accessed through it
    helper()                      // helper imported
    extras::something()           // extras imported as namespace
}
```

A glob import can also target an enum to bring its variants directly into scope:

```zoya
use root::types::Color::*        // imports Red, Green, Blue as bare names

fn main() -> Int {
    match Red {
        Red => 1,
        Green => 2,
        Blue => 3,
    }
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

A group import can also target an enum to import specific variants:

```zoya
use root::types::Color::{Red, Green}  // imports Red and Green as bare names

fn main() -> Int {
    match Red {
        Red => 1,
        Green => 2,
        Blue => 3,
    }
}
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
pub use root::collections::*           // re-export all public items and modules
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

### Module Re-exports

A `pub use` targeting a module re-exports the entire module as a namespace. Other modules can then access the re-exported module through the re-exporting module's path:

```zoya
// Module: root::a
pub fn helper() -> Int 42
pub fn add(x: Int, y: Int) -> Int x + y

// Module: root::b
pub use root::a                   // re-exports module 'a' through 'b'

// Module: root::consumer
use root::b::a                    // imports module 'a' via the re-export in 'b'

fn main() -> Int a::helper()      // resolves to root::a::helper
```

All import forms work through a re-exported module:

```zoya
use root::b::a::helper            // single item import through re-exported module
use root::b::a::*                 // glob import through re-exported module
use root::b::a::{add, helper}     // group import through re-exported module
```

Glob re-exports also include public child modules. Group re-exports can mix modules and items:

```zoya
// Module: root::lib
pub mod utils
pub fn helper() -> Int 42

// Module: root::facade
pub use root::lib::*              // re-exports helper and utils module
pub use root::lib::{utils, helper}  // equivalent explicit form
```

### Enum Variant Re-exports

Glob and group re-exports can target an enum to re-export its variants:

```zoya
// Module: root::types
pub enum Color { Red, Green, Blue }

// Module: root::prelude
pub use root::types::Color::*             // re-exports Red, Green, Blue
pub use root::types::Color::{Red, Green}  // re-exports specific variants
```

```zoya
// Module: root::app
use root::prelude::Red            // imports the re-exported variant

fn main() -> Int {
    match Red {
        Red => 1,
        Green => 2,
        Blue => 3,
    }
}
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
