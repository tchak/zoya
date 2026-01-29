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
| [zoya-lexer](crates/zoya-lexer) | Tokenizer (logos) |
| [zoya-parser](crates/zoya-parser) | Parser (chumsky) |

## Usage

### REPL

Start an interactive session:

```bash
zoya run
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
zoya run program.zoya
```

### Type Check Only

Validate types without executing:

```bash
zoya check program.zoya
```

### Compile to JavaScript

```bash
zoya build program.zoya           # Output to stdout
zoya build program.zoya -o out.js # Output to file
```

## Language Tour

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

Built-in generic types: `Option<T>` and `Result<T, E>`.

### Functions

```zoya
// Basic function
fn add(x: Int, y: Int) -> Int {
    x + y
}

// Single-expression bodies can omit braces
fn square(x: Int) -> Int x * x

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
- **snake_case**: function names, variable names, parameters

```zoya
struct MyStruct { }     // OK
struct myStruct { }     // Error!

fn my_function() { }    // OK
fn myFunction() { }     // Error!
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features including:

- Module system
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
