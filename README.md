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
- **Module system** - Organize code into modules with public/private visibility
- **Standard library** - `Option<T>` and `Result<T, E>` included
- **Immutable by default** - All data structures are immutable
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
| [zoya-ir](crates/zoya-ir) | Typed IR and type definitions |
| [zoya-lexer](crates/zoya-lexer) | Tokenizer (logos) |
| [zoya-loader](crates/zoya-loader) | Package file loading |
| [zoya-package](crates/zoya-package) | Package data structures |
| [zoya-parser](crates/zoya-parser) | Parser (chumsky) |
| [zoya-run](crates/zoya-run) | Runtime execution (QuickJS) |
| [zoya-std](crates/zoya-std) | Standard library (Option, Result) |

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
    └── main.zoya      # Entry point
```

The `package.toml` file defines the package:

```toml
name = "my_project"
main = "src/main.zoya"
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
zoya run program.zoya         # Run a single file
zoya run                      # Run package in current directory
zoya run path/to/project      # Run package at path
```

### Type Check Only

Validate types without executing:

```bash
zoya check program.zoya       # Check a single file
zoya check                    # Check package in current directory
```

### Compile to JavaScript

```bash
zoya build program.zoya           # Output to stdout
zoya build program.zoya -o out.js # Output to file
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

### Modules

Zoya organizes code into modules using `mod` declarations. Each module maps to a file:

```zoya
// src/main.zoya
mod utils              // loads src/utils.zoya
mod math               // loads src/math.zoya

fn main() -> Int {
    utils::helper()
}
```

```zoya
// src/utils.zoya
pub fn helper() -> Int { 42 }
```

For nested modules:

```zoya
// src/math.zoya
mod geometry           // loads src/math/geometry.zoya

pub fn add(x: Int, y: Int) -> Int x + y
```

```zoya
// src/math/geometry.zoya
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

Use `pub use` to re-export imported items:

```zoya
// src/main.zoya
mod math

pub use root::math::add     // re-export add from root
```

This makes `add` available to anyone who can access the current module, even though it is defined in `math`.

### Standard Library

Zoya includes a standard library with common types:

```zoya
// Option<T> - represents an optional value
let some_val = Option::Some(42)
let no_val = Option::None::<Int>

// Result<T, E> - represents success or failure
let ok = Result::Ok::<Int, String>(42)
let err = Result::Err::<Int, String>("oops")
```

### Methods

Built-in methods on primitive types:

```zoya
// String
"hello".len()           // 5
"hello".is_empty()      // false
"hello".contains("ell") // true
"hello".to_uppercase()  // "HELLO"

// Int
(-5).abs()              // 5
42.to_string()          // "42"
3.min(5)                // 3

// Float
3.14.floor()            // 3.0
3.14.ceil()             // 4.0
4.0.sqrt()              // 2.0

// List (all operations return new lists)
[1, 2].push(3)          // [1, 2, 3]
[1, 2].concat([3, 4])   // [1, 2, 3, 4]
[1, 2, 3].reverse()     // [3, 2, 1]
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

See [ROADMAP.md](ROADMAP.md) for planned features including:

- impl blocks for user-defined methods
- Traits
- Expanded standard library

## Contributing

Contributions welcome! The compiler is written in Rust. See [CLAUDE.md](CLAUDE.md) for development guidelines.

```bash
cargo test      # Run tests
cargo clippy    # Lint
```

## License

[MIT](LICENSE)
