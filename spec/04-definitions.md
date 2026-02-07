# Definitions

A Zoya module consists of module declarations, use declarations, and item definitions. Items are the primary building blocks of a program.

```
module ::= (mod_decl | use_decl | item)*
item   ::= function_def | struct_def | enum_def | type_alias_def
```

Items are described using grammar productions that reference [type annotations](02-types.md), [expressions](03-expressions.md), and [lexical tokens](01-lexical-structure.md) defined in earlier sections.

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
function_def ::= visibility 'fn' identifier type_params? '(' params? ')' ('->' type)? body
type_params  ::= '<' identifier (',' identifier)* '>'
params       ::= param (',' param)* ','?
param        ::= pattern ':' type
body         ::= '{' (let_binding ';')* expr '}' | expr
```

The return type annotation is optional. When omitted, the return type is inferred from the body expression.

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

Function parameters support [pattern](03-expressions.md#patterns) matching:

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

## Struct Definitions

```
struct_def ::= visibility 'struct' identifier type_params? '{' fields? '}'
fields     ::= field (',' field)* ','?
field      ::= identifier ':' type
```

See [Struct Types](02-types.md#struct-types) for construction, field access, and usage.

```zoya
struct Config { debug: Bool, verbose: Bool }
pub struct Point { x: Float, y: Float }
```

### Generic Structs

```zoya
pub struct Pair<A, B> {
    first: A,
    second: B,
}
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

See [Enum Types](02-types.md#enum-types) for construction and usage.

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

See [Type Aliases](02-types.md#type-aliases) for usage.

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
mod_decl ::= 'mod' identifier
```

A module declaration declares a submodule. Module declarations do not have a visibility modifier. Each declaration corresponds to a source file:

- `mod foo` in the root module expects `foo.zoya` in the same directory
- `mod bar` in module `foo` expects `foo/bar.zoya`

```zoya
// root module (main.zoya)
mod utils
mod types
```

Modules form a tree rooted at the root module. Each module has a path (e.g., `root::utils::helpers`).

## Use Declarations

```
use_decl    ::= 'use' use_path
use_path    ::= path_prefix identifier ('::' identifier)*
path_prefix ::= 'root' '::' | 'self' '::' | 'super' '::'
```

A use declaration imports a name from another module into the current scope. A path prefix is required.

| Prefix | Meaning |
|--------|---------|
| `root::` | Absolute path from the root module |
| `self::` | Current module |
| `super::` | Parent module |

The imported name is the last segment of the path:

```zoya
use root::utils::helper        // imports 'helper'
use self::types::Config        // imports 'Config'
use super::shared              // imports 'shared'
```

Use declarations do not have a visibility modifier. Imports are always local to the declaring module and are not re-exported.

### Visibility and Imports

A use declaration can only import items that are visible to the importing module. Attempting to import a private item from a non-ancestor module is an error:

```zoya
// Module: root::a
fn secret() -> Int 42

// Module: root::b
use root::a::secret            // Error: 'root::a::secret' is private
```
