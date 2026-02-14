# Zoya

A strongly-typed functional programming language that compiles to JavaScript.

Zoya combines Rust-inspired syntax with Hindley-Milner type inference, giving you the safety of static types without the verbosity of explicit annotations everywhere.

## Quick Example

```zoya
struct Point { x: Int, y: Int }

fn distance(Point { x, y }: Point) -> Float {
    let squared = x * x + y * y;
    squared.to_float().sqrt()
}

fn main() -> Float {
    let origin = Point { x: 3, y: 4 };
    distance(origin)
}
```

## Features

- **Type inference** - Types are inferred automatically; annotations optional
- **Algebraic data types** - Structs (products) and enums (sums) with generics
- **Type aliases** - Named type synonyms with generic support
- **Pattern matching** - Exhaustive matching with destructuring everywhere
- **First-class functions** - Lambdas, closures, and higher-order functions
- **Impl blocks** - Methods and associated functions on user-defined types
- **Module system** - Organize code into modules with public/private visibility
- **Standard library** - `Option<T>`, `Result<T, E>`, `Dict<K, V>`, JSON, and more
- **String interpolation** - `$"hello {name}!"` syntax with embedded expressions
- **Immutable by default** - All data structures are persistent and immutable
- **Compiles to JavaScript** - Run anywhere JS runs

## Installation

Requires [Rust](https://rustup.rs/) (1.85+).

```bash
git clone https://github.com/user/zoya-lang
cd zoya-lang
cargo build --release
```

The binary will be at `target/release/zoya`.

## Workspace Structure

Zoya is organized as a Cargo workspace with multiple crates:

| Crate | Description |
|-------|-------------|
| [zoya](crates/zoya) | Main compiler and CLI |
| [zoya-ast](crates/zoya-ast) | Abstract Syntax Tree types |
| [zoya-check](crates/zoya-check) | Type checker with Hindley-Milner inference |
| [zoya-codegen](crates/zoya-codegen) | JavaScript code generation |
| [zoya-fmt](crates/zoya-fmt) | Source code formatter |
| [zoya-ir](crates/zoya-ir) | Typed IR and type definitions |
| [zoya-lexer](crates/zoya-lexer) | Tokenizer (logos) |
| [zoya-loader](crates/zoya-loader) | Package file loading |
| [zoya-naming](crates/zoya-naming) | Naming conventions and validation |
| [zoya-package](crates/zoya-package) | Package data structures |
| [zoya-parser](crates/zoya-parser) | Parser (chumsky) |
| [zoya-run](crates/zoya-run) | Runtime execution (QuickJS) |
| [zoya-std](crates/zoya-std) | Standard library |

## Usage

### Create a New Project

```bash
zoya new my_project
cd my_project
```

This creates:

```
my_project/
├── package.toml       # Package configuration
└── src/
    └── main.zy      # Entry point
```

The `package.toml` file defines the package:

```toml
[package]
name = "my_project"
```

Optional fields:

```toml
[package]
name = "my_project"
main = "src/main.zy"   # Entry point (default: src/main.zy)
output = "build"        # Build output directory (default: build)
```

### REPL

Start an interactive session:

```bash
zoya repl
```

```
> let greeting = "Hello, Zoya!"
let greeting: String
> greeting.len()
12
> let add = |x, y| x + y
let add: (?0, ?0) -> ?0
> add(1, 2)
3
```

### Run a File

```bash
zoya run program.zy         # Run a single file
zoya run                      # Run package in current directory
zoya run path/to/project      # Run package at path
zoya run --mode test          # Run in test mode
```

### Type Check Only

Validate types without executing:

```bash
zoya check program.zy       # Check a single file
zoya check                    # Check package in current directory
```

### Compile to JavaScript

```bash
zoya build program.zy           # Output to stdout
zoya build program.zy -o out.js # Output to file
```

### Format Source Code

```bash
zoya fmt                     # Format all .zy files in current directory
zoya fmt program.zy          # Format a single file
zoya fmt --check             # Check formatting without writing
```

### Run Tests

```bash
zoya test                    # Run tests in current package
zoya test path/to/project    # Run tests at path
```

## Language Tour

### Comments

```zoya
// This is a line comment
fn main() -> Int {
    42 // inline comment
}
```

### Types

Zoya has the following built-in types:

| Type | Examples |
|------|----------|
| `Int` | `42`, `1_000`, `-5` |
| `BigInt` | `42n`, `9_000_000_000n` |
| `Float` | `3.14`, `0.5` |
| `Bool` | `true`, `false` |
| `String` | `"hello"`, `"line\nbreak"` |
| `List<T>` | `[1, 2, 3]`, `[]` |
| `Dict<K, V>` | persistent hash map |
| `(T, U, ...)` | `(1, "hello")`, `()`, `(42,)` |
| `T -> U` | `Int -> Bool`, `(Int, Int) -> Int` |

### Functions

```zoya
// Basic function
fn add(x: Int, y: Int) -> Int {
    x + y
}

// Single-expression bodies can omit braces
fn square(x: Int) -> Int x * x

// Return type annotation is optional
fn double(x: Int) x * 2

// Generic functions
fn identity<T>(x: T) -> T x

// Pattern destructuring in parameters
fn swap((a, b): (Int, Int)) -> (Int, Int) (b, a)
```

### Let Bindings

```zoya
let x = 42                      // Type inferred as Int
let y: Float = 3.14             // Explicit type annotation
let (a, b) = (1, 2)             // Tuple destructuring
let Point { x, y } = point      // Struct destructuring
let (first, ..) = long_tuple    // Rest patterns
let pair @ (a, b) = (1, 2)      // As-patterns (bind whole and parts)
```

### Lambdas

```zoya
let inc = |x| x + 1
let add = |x, y| x + y
let typed = |x: Int| -> Int x * 2
let block = |x| { let y = x * 2; y + 1 }

// Pattern destructuring
let get_x = |Point { x, .. }| x
let sum_pair = |(a, b)| a + b
```

### String Interpolation

Embed expressions in strings using `$"..."` syntax:

```zoya
let name = "world"
let greeting = $"hello {name}!"       // "hello world!"

let x = 42
let msg = $"the answer is {x}"        // "the answer is 42"
let calc = $"1 + 2 = {1 + 2}"         // "1 + 2 = 3"
```

Interpolated expressions must be `String`, `Int`, `Float`, or `BigInt`.

### Operators

```zoya
// Arithmetic
1 + 2       // addition
5 - 3       // subtraction
2 * 3       // multiplication
10 / 3      // integer division
10 % 3      // modulo
2 ** 10     // power (1024)

// Comparison
x == y      // equality
x != y      // inequality
x < y       x > y       x <= y       x >= y

// Logical
a && b      // and
a || b      // or
!a          // not

// String
"hello" ++ " world"   // concatenation
```

### Structs

```zoya
struct Point { x: Int, y: Int }
struct Pair<T, U> { first: T, second: U }

let p = Point { x: 1, y: 2 }
let x_coord = p.x

// Shorthand when variable names match fields
let x = 10
let y = 20
let p = Point { x, y }
```

### Enums

```zoya
enum Color { Red, Green, Blue }
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Err(E) }
enum Message {
    Quit,
    Move { x: Int, y: Int },
    Write(String),
}

let color = Color::Red
let maybe = Option::Some(42)
let msg = Message::Move { x: 10, y: 20 }

// Turbofish for explicit type arguments
let none = Option::None::<Int>
```

### Impl Blocks

Define methods and associated functions on types:

```zoya
struct Point { x: Int, y: Int }

impl Point {
    fn new(x: Int, y: Int) -> Point {
        Point { x, y }
    }

    fn distance(self) -> Float {
        let squared = self.x * self.x + self.y * self.y;
        squared.to_float().sqrt()
    }
}

let p = Point::new(3, 4)
p.distance()    // 5.0
```

Generic impl blocks:

```zoya
impl<T> Option<T> {
    fn map<U>(self, f: (T) -> U) -> Option<U> {
        match self {
            Option::Some(v) => Option::Some(f(v)),
            Option::None => Option::None::<U>,
        }
    }
}
```

### Type Aliases

Create named synonyms for types:

```zoya
type UserId = Int
type Callback = (Int) -> Bool
type Pair<A, B> = (A, B)
type StringList = List<String>

fn get_user(id: UserId) -> String { ... }
fn make_pair() -> Pair<Int, Bool> { (1, true) }
```

Type aliases are transparent - `UserId` and `Int` are interchangeable everywhere.

### Pattern Matching

```zoya
fn describe(opt: Option<Int>) -> String {
    match opt {
        Option::None => "nothing",
        Option::Some(0) => "zero",
        Option::Some(n) => n.to_string(),
    }
}

// List patterns
match list {
    [] => "empty",
    [x] => "single",
    [x, y] => "pair",
    [first, ..] => "has first",
    [.., last] => "has last",
    [first, .., last] => "has both",
}

// Tuple patterns
match tuple {
    (0, _) => "starts with zero",
    (_, 0) => "ends with zero",
    (a, b) => a + b,
}
```

Pattern matching is exhaustive - the compiler ensures all cases are covered.

### List Spread

Spread lists into other lists:

```zoya
let xs = [1, 2, 3]
let ys = [0, ..xs, 4]    // [0, 1, 2, 3, 4]
```

### Modules

Zoya organizes code into modules using `mod` declarations. Each module maps to a file:

```zoya
// src/main.zy
mod utils              // loads src/utils.zy
mod math               // loads src/math.zy

fn main() -> Int {
    utils::helper()
}
```

```zoya
// src/utils.zy
pub fn helper() -> Int { 42 }
```

For nested modules:

```zoya
// src/math.zy
mod geometry           // loads src/math/geometry.zy

pub fn add(x: Int, y: Int) -> Int x + y
```

```zoya
// src/math/geometry.zy
pub fn area(w: Int, h: Int) -> Int w * h
```

Module names must be `snake_case`.

### Visibility

Items are private by default. Use `pub` to make them accessible from other modules:

```zoya
pub fn public_function() -> Int { 42 }
fn private_function() -> Int { 10 }

pub struct Point { x: Int, y: Int }
struct Internal { data: Int }

pub enum Color { Red, Green, Blue }

pub type UserId = Int

pub mod submodule
```

Public items can reference only public types in their signatures:

```zoya
pub struct Pair { x: Int, y: Int }         // OK: Int is always visible
pub fn make_pair() -> Pair { ... }         // OK: Pair is pub
// pub fn get_internal() -> Internal { ... }  Error: Internal is private
```

### Imports

Use `use` to bring items from other modules into scope:

```zoya
// Import a specific item
use root::utils::helper

// Use it without qualification
fn main() -> Int {
    helper()
}
```

Import a module as a namespace:

```zoya
use root::math

fn main() -> Int {
    math::add(1, 2)         // access items through the module name
}
```

Glob imports bring all public items from a module, including child modules:

```zoya
use root::types::*           // imports all public items and modules from types

fn main() -> Int {
    let c = Color::Red;      // Color was imported via glob
    helper()                 // helper was imported via glob
    child_mod::something()   // public child modules are also imported
}
```

Glob and group imports can also target enums to import variants directly:

```zoya
use root::types::Color::*              // import all variants
use root::types::Option::{Some, None}  // import specific variants

fn main() -> Int {
    match Some(Red) {
        Some(Red) => 1,
        _ => 0,
    }
}
```

Group imports bring specific items:

```zoya
use root::math::{add, subtract}

fn main() -> Int {
    add(1, subtract(5, 3))
}
```

Path prefixes for navigation:

| Prefix | Meaning |
|--------|---------|
| `root::` | Absolute path from the package root |
| `self::` | Relative to the current module |
| `super::` | Relative to the parent module |

```zoya
use root::math::add          // absolute import
use self::helpers::format    // relative import
use super::shared::Config    // parent module import
```

### Re-exports

Use `pub use` to re-export imported items. All import forms support `pub`:

```zoya
pub use root::math::add              // re-export single item
pub use root::math                   // re-export a module
pub use root::collections::*         // re-export all public items and modules
pub use root::math::{add, subtract}  // re-export specific items
pub use root::types::Color::*        // re-export all enum variants
pub use root::types::Color::{Red}    // re-export specific variants
```

This makes the items available to anyone who can access the current module, even though they are defined elsewhere.

### Standard Library

Zoya includes a standard library with common types and methods:

```zoya
// Option<T> - represents an optional value
let some_val = Option::Some(42)
let no_val = Option::None::<Int>
some_val.map(|x| x + 1)       // Option::Some(43)
some_val.unwrap()               // 42

// Result<T, E> - represents success or failure
let ok = Result::Ok::<Int, String>(42)
let err = Result::Err::<Int, String>("oops")
ok.map(|x| x + 1)              // Result::Ok(43)
```

### Methods

Methods on primitive types (defined in the standard library via impl blocks):

```zoya
// String
"hello".len()                  // 5
"hello".is_empty()             // false
"hello".contains("ell")        // true
"hello".to_uppercase()         // "HELLO"
"hello world".split(" ")       // ["hello", "world"]
"  hello  ".trim()             // "hello"

// Int
(-5).abs()                     // 5
42.to_string()                 // "42"
3.min(5)                       // 3
2.pow(10)                      // 1024

// Float
3.14.floor()                   // 3.0
3.14.ceil()                    // 4.0
4.0.sqrt()                     // 2.0
1.5.round()                    // 2.0

// BigInt
42n.abs()                      // 42n
42n.to_string()                // "42"

// List (all operations return new lists)
[1, 2].push(3)                 // [1, 2, 3]
[1, 2].concat([3, 4])          // [1, 2, 3, 4]
[1, 2, 3].reverse()            // [3, 2, 1]
[1, 2, 3].map(|x| x * 2)      // [2, 4, 6]
[1, 2, 3].filter(|x| x > 1)   // [2, 3]

// Dict
Dict::new::<String, Int>()     // empty dict
dict.insert("key", 42)         // new dict with entry
dict.get("key")                // Option::Some(42)
dict.keys()                    // list of keys

// Option
Option::Some(42).map(|x| x + 1)      // Option::Some(43)
Option::Some(42).unwrap()              // 42
Option::None::<Int>.unwrap_or(0)       // 0
Option::Some(42).is_some()             // true

// Result
Result::Ok::<Int, String>(42).map(|x| x + 1)   // Result::Ok(43)
Result::Ok::<Int, String>(42).unwrap()           // 42
Result::Err::<Int, String>("no").unwrap_or(0)    // 0
```

## Naming Conventions

Zoya enforces naming conventions at compile time:

- **PascalCase**: struct names, enum names, variant names, type parameters
- **snake_case**: function names, variable names, parameters, module names

```zoya
struct MyStruct { }     // OK
struct myStruct { }     // Error!

fn my_function() { }    // OK
fn myFunction() { }     // Error!

mod my_module           // OK
mod MyModule            // Error!
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features.

## Contributing

Contributions welcome! The compiler is written in Rust. See [CLAUDE.md](CLAUDE.md) for development guidelines.

```bash
cargo test      # Run tests
cargo clippy    # Lint
```

## License

[MIT](LICENSE)
