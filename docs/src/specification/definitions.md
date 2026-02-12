# Definitions

A Zoya module consists of module declarations, use declarations, and item definitions. Items are the primary building blocks of a program.

```
module ::= (mod_decl | use_decl | item)*
item   ::= function_def | struct_def | enum_def | type_alias_def
```

Items are described using grammar productions that reference [type annotations](types.md), [expressions](expressions.md), and [lexical tokens](lexical-structure.md) defined in earlier sections.

## Visibility

Every item has a visibility that controls which modules can access it.

```
visibility ::= 'pub' | ε
```

### Default Visibility

All items are private by default. A private item is accessible from the module that defines it and from all descendant modules.

```zoya
// In module utils
fn helper() -> Int 42            // private
struct Config { debug: Bool }    // private
```

### Public Visibility

The `pub` keyword makes an item accessible from any module.

```zoya
pub fn add(x: Int, y: Int) -> Int x + y
pub struct Point { x: Float, y: Float }
pub enum Color { Red, Green, Blue }
pub type UserId = Int
```

### Access Rules

| Accessor relationship to defining module | Private | Public |
|------------------------------------------|---------|--------|
| Same module                              | Yes     | Yes    |
| Descendant module                        | Yes     | Yes    |
| Parent module                            | No      | Yes    |
| Sibling module                           | No      | Yes    |
| Unrelated module                         | No      | Yes    |

```zoya
// Module: root
mod utils
mod other

// Module: root::utils
fn secret() -> Int 42       // private to utils
mod sub

// Module: root::utils::sub
use super::secret            // OK: descendant of utils

// Module: root::other
use root::utils::secret      // Error: 'root::utils::secret' is private
```

### Visibility Consistency

All types referenced in a public item's signature must themselves be public. This prevents exposing private types through public interfaces.

The rule applies to:

- Function parameter types and return type
- Struct field types
- Enum variant data types
- Type alias underlying type

Type references are checked recursively through generic type arguments. Primitive types (`Int`, `BigInt`, `Float`, `Bool`, `String`), built-in generic types (`List`), tuple types, and function types are always considered public.

```zoya
struct Secret { value: Int }

// Error: private type 'Secret' in public function signature
pub fn reveal() -> Secret {
    Secret { value: 42 }
}

// Error: private type 'Secret' in public struct
pub struct Wrapper {
    inner: Secret,
}

// Error: private type 'Secret' in public enum variant
pub enum Container {
    Value(Secret),
}

// Error: private type 'Secret' in public type alias
pub type Alias = Secret

// Error: private type 'Secret' in public function signature (nested in List)
pub fn get_secrets() -> List<Secret> {
    [Secret { value: 1 }]
}
```

Making the referenced type public resolves the error:

```zoya
pub struct Secret { value: Int }
pub fn reveal() -> Secret {      // OK
    Secret { value: 42 }
}
```

Private items may freely reference other private items or public items:

```zoya
pub struct Point { x: Float, y: Float }
struct Internal { p: Point }     // OK: private item referencing public type
fn helper() -> Internal {        // OK: private function returning private type
    Internal { p: Point { x: 0.0, y: 0.0 } }
}
```

## Function Definitions

```
function_def ::= attribute* visibility 'fn' identifier type_params? '(' params? ')' ('->' type)? body
attribute      ::= '#[' identifier attribute_args? ']'
attribute_args ::= '(' (identifier (',' identifier)* ','?)? ')'
type_params  ::= '<' identifier (',' identifier)* '>'
params       ::= param (',' param)* ','?
param        ::= pattern ':' type
body         ::= '{' (let_binding ';')* expr '}' | expr
```

The return type annotation is optional. When omitted, the return type is inferred from the body expression:

```zoya
fn square(x: Int) x * x         // return type inferred as Int
fn add(x: Int, y: Int) x + y    // return type inferred as Int
```

### Function Body

A function body is either a single expression or a block expression:

```zoya
fn square(x: Int) -> Int x * x

fn distance(x: Float, y: Float) -> Float {
    let squared = x * x + y * y;
    squared.sqrt()
}
```

### Generic Functions

Type parameters are declared in angle brackets after the function name:

```zoya
fn identity<T>(x: T) -> T x
pub fn apply<T, U>(f: T -> U, x: T) -> U f(x)
```

### Parameter Destructuring

Function parameters support [pattern](expressions.md#patterns) matching:

```zoya
fn sum_pair((a, b): (Int, Int)) -> Int a + b
fn get_x(Point { x, .. }: Point) -> Float x
```

### Naming Conventions

Function names use `snake_case`. Type parameter names use `PascalCase`.

### Examples

```zoya
fn answer() -> Int 42
fn negate(x: Int) -> Int -x
pub fn add(x: Int, y: Int) -> Int x + y
fn curry_add(x: Int) -> Int -> Int |y| x + y
```

### Builtin Functions

The `#[builtin]` attribute declares a function whose implementation is provided by the compiler rather than written in Zoya. The function signature is type-checked normally, but the body must be the unit expression `()` and the return type must be explicitly annotated.

```
builtin_function_def ::= '#[builtin]' visibility 'fn' identifier type_params? '(' params? ')' '->' type '()'
```

Builtin functions are restricted to the standard library. Attempting to use `#[builtin]` outside the `std` package is an error.

```zoya
// In std::json
#[builtin]
pub fn parse(value: String) -> Result<JSON, ParseError> ()
```

The compiler substitutes a JavaScript implementation at code generation time. Every `#[builtin]` function must have a corresponding implementation registered in the code generator.

### Test Functions

The `#[test]` attribute marks a function as test-only. Test functions are excluded from the program in `dev` and `release` modes, and included only in `test` mode.

```zoya
#[test]
fn test_addition() {
    let result = 1 + 1;
    ()
}
```

Test functions have the following constraints:

- **No parameters** — test functions must be parameterless.
- **Return type** — test functions must return `()` (unit) or `Result`. No other return types are allowed.
- **No `#[builtin]`** — a function cannot have both `#[builtin]` and `#[test]` attributes.

The `#[test]` attribute is only valid on function definitions. Using it on structs, enums, type aliases, or use declarations is a type error. Using it on module declarations is a loader error — use `#[mode(test)]` instead.

### Conditional Compilation

The `#[mode(test)]` attribute marks an item or module as test-only. Like `#[test]`, items with `#[mode(test)]` are excluded in `dev` and `release` modes and included in `test` mode.

```zoya
#[mode(test)]
fn test_helper() -> Int { 42 }

#[mode(test)] mod tests
```

Unlike `#[test]`, `#[mode(test)]` can be used on any item or module declaration. When applied to a module declaration, the entire module (and its submodules) is not loaded in non-test modes.

## Struct Definitions

Structs come in three forms: named-field structs, tuple structs, and unit structs.

```
struct_def   ::= visibility 'struct' identifier type_params? struct_body?
struct_body  ::= '{' fields '}' | '(' tuple_fields ')'
fields       ::= field (',' field)* ','?
field        ::= identifier ':' type
tuple_fields ::= type (',' type)* ','?
```

See [Struct Types](types.md#struct-types) for construction, field access, and usage.

### Named-Field Structs

```zoya
struct Config { debug: Bool, verbose: Bool }
pub struct Point { x: Float, y: Float }
```

### Tuple Structs

Tuple structs have positional fields identified by type rather than name. They are constructed and destructured like function calls.

```zoya
struct Wrapper(Int)
pub struct Pair(String, Int)
```

Tuple structs require at least one field. Use a unit struct for the zero-field case.

### Unit Structs

```zoya
struct Empty
```

### Generic Structs

```zoya
pub struct Pair<A, B> {
    first: A,
    second: B,
}

pub struct Box<T>(T)
```

### Naming Conventions

Struct names and type parameter names use `PascalCase`. Field names use `snake_case`.

## Enum Definitions

```
enum_def ::= visibility 'enum' identifier type_params? '{' variants '}'
variants ::= variant (',' variant)* ','?
variant  ::= identifier
           | identifier '(' type (',' type)* ','? ')'
           | identifier '{' fields? '}'
```

See [Enum Types](types.md#enum-types) for construction and usage.

### Variant Forms

Variants come in three forms:

```zoya
enum Message {
    Quit,                          // unit variant
    Write(String),                 // tuple variant
    Move { x: Int, y: Int },       // struct variant
}
```

### Generic Enums

```zoya
pub enum Option<T> {
    None,
    Some(T),
}

pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

### Enum Variant Visibility

Enum variants inherit the visibility of their parent enum. There is no per-variant visibility modifier.

```zoya
pub enum Color {    // public enum
    Red,            // public (inherited)
    Green,          // public (inherited)
    Blue,           // public (inherited)
}

enum Internal {     // private enum
    A,              // private (inherited)
    B(Int),         // private (inherited)
}
```

### Naming Conventions

Enum names, variant names, and type parameter names use `PascalCase`.

## Type Alias Definitions

```
type_alias_def ::= visibility 'type' identifier type_params? '=' type
```

A type alias introduces a synonym for an existing type. Aliases are transparent: the alias and the underlying type are fully interchangeable.

See [Type Aliases](types.md#type-aliases) for usage.

```zoya
pub type UserId = Int
type Callback<T> = T -> ()
pub type Pair<A, B> = (A, B)
type StringList = List<String>
```

### Naming Conventions

Type alias names and type parameter names use `PascalCase`.

## Module Declarations

```
mod_decl ::= attribute* visibility 'mod' identifier
```

A module declaration declares a submodule. Each declaration corresponds to a source file:

- `mod foo` in the root module expects `foo.zy` in the same directory
- `mod bar` in module `foo` expects `foo/bar.zy`

```zoya
// root module (main.zy)
pub mod utils
mod internal
```

Modules form a tree rooted at the root module. Each module has a path (e.g., `root::utils::helpers`).

### Naming Conventions

Module names use `snake_case`: must start with a lowercase letter, followed by lowercase letters, digits, or underscores. Module names must not be reserved names (`root`, `self`, `super`, `std`, `zoya`).

```zoya
mod utils          // OK
mod my_helpers     // OK
mod v2             // OK
mod MyModule       // Error: should be 'my_module'
mod _private       // Error: must start with a letter
mod std            // Error: 'std' is a reserved name
```

A private module is only accessible from the declaring module and its descendants. A public module is accessible from any module. To access an item in a nested module, all modules along the path must be visible to the accessor:

```zoya
// Module: root
pub mod api
mod internal

// Module: root::api
pub fn endpoint() -> Int 42

// Module: root::internal
pub fn helper() -> Int 1

// Module: root::other
use root::api::endpoint          // OK: api is public
use root::internal::helper       // Error: module 'internal' is private
```

## Use Declarations

```
use_decl    ::= visibility 'use' use_path
use_path    ::= path_prefix identifier ('::' identifier)* use_suffix?
use_suffix  ::= '::' '*' | '::' '{' use_group '}'
use_group   ::= identifier (',' identifier)* ','?
path_prefix ::= 'root' '::' | 'self' '::' | 'super' '::'
```

A use declaration imports names from another module into the current scope. A path prefix is required. There are three import forms.

### Single Import

A single import (no suffix) brings one item or module into scope. The local name is the last path segment:

```zoya
use root::utils::helper        // imports item 'helper'
use self::types::Config        // imports item 'Config'
use super::shared              // imports 'shared' (item or module)
```

If the path resolves to a module rather than an item, the module is imported as a namespace. Items within it are accessed with qualified paths (e.g., `math::add()`).

### Glob Import

A glob import (`::*`) brings all public items from a module into scope:

```zoya
use root::math::*              // imports all public items from root::math
```

Private items are silently skipped. Enum variants are not directly imported — only the enum type itself.

### Group Import

A group import (`::{...}`) brings specific named items from a module into scope:

```zoya
use root::math::{add, subtract}    // imports add and subtract
use root::math::{add, subtract,}   // trailing comma is permitted
```

An empty group `::{}` is a parse error. Each named item must exist and be visible.

### Path Prefixes

| Prefix | Meaning |
|--------|---------|
| `root::` | Absolute path from the root module |
| `self::` | Current module |
| `super::` | Parent module |

### Re-exports

A private use declaration (the default) imports the name locally. A `pub use` declaration re-exports the imported names, making them accessible through the current module. All three forms support `pub`:

```zoya
// Module: root::prelude
pub use root::option::Option         // re-export single item
pub use root::collections::*         // re-export all public items
pub use root::math::{add, subtract}  // re-export specific items
```

The re-exported names follow the same visibility rules as any other public item. The visibility consistency rule applies: a `pub use` can only re-export items that are themselves public.

Re-exporting a module with `pub use` is not yet supported.

### Visibility and Imports

A use declaration can only import items that are visible to the importing module. Attempting to import a private item from a non-ancestor module is an error:

```zoya
// Module: root::a
fn secret() -> Int 42

// Module: root::b
use root::a::secret            // Error: 'root::a::secret' is private
```
