# Types

Zoya is a statically typed language with Hindley-Milner type inference. Types are checked at compile time and erased at runtime.

## Primitive Types

### Int

64-bit signed integer.

```zoya
let x = 42
let y = -17
let max = 9223372036854775807
```

### BigInt

Arbitrary precision integer, denoted with `n` suffix.

```zoya
let big = 9_000_000_000_000_000n
let negative = -42n
```

### Float

64-bit IEEE 754 floating-point number.

```zoya
let pi = 3.14159
let negative = -2.5
```

### Bool

Boolean type with two values.

```zoya
let yes = true
let no = false
```

### String

Unicode string, delimited by double quotes.

```zoya
let greeting = "Hello, world!"
let empty = ""
let escaped = "line1\nline2"
```

## List Types

Homogeneous, immutable sequences.

### Syntax

```
List<T>
```

### Literals

```zoya
let numbers = [1, 2, 3]
let strings = ["a", "b", "c"]
let empty: List<Int> = []
let nested = [[1, 2], [3, 4]]
```

All elements must have the same type. Empty lists require type annotation or inference from context.

## Tuple Types

Fixed-size, heterogeneous product types.

### Syntax

```
(T1, T2, ...)
(T,)
()
```

### Unit Type

The empty tuple `()` is the unit type, used when no meaningful value is returned.

```zoya
let unit = ()
```

### Single-Element Tuples

Single-element tuples require a trailing comma to distinguish from parenthesized expressions.

```zoya
let single = (42,)
let not_tuple = (42)  // This is just Int
```

### Multi-Element Tuples

```zoya
let pair = (1, "hello")
let triple = (true, 42, 3.14)
```

## Function Types

### Syntax

```
T -> U
(T1, T2) -> R
() -> R
```

### Right Associativity

Function types are right-associative:

```zoya
// A -> B -> C is parsed as A -> (B -> C)
fn add(x: Int) -> Int -> Int |y| x + y
```

### Examples

```zoya
// Single argument
fn negate(x: Int) -> Int -x

// Multiple arguments
fn add(x: Int, y: Int) -> Int { x + y }

// No arguments
fn answer() -> Int 42

// Higher-order functions
fn apply(f: Int -> Int, x: Int) -> Int f(x)
```

## Struct Types

Named product types with labeled fields.

### Definition

```zoya
struct Point {
  x: Float,
  y: Float,
}
```

### Construction

```zoya
let p = Point { x: 1.0, y: 2.0 }
```

### Field Access

```zoya
let x_coord = p.x
```

### Generic Structs

```zoya
struct Pair<A, B> {
  first: A,
  second: B,
}

let p = Pair { first: 1, second: "one" }
```

## Enum Types

Sum types with variants. Each variant can be unit, tuple, or struct style.

### Definition

```zoya
enum Color {
  Red,
  Green,
  Blue,
}

enum Shape {
  Circle(Float),
  Rectangle(Float, Float),
  Point { x: Float, y: Float },
}
```

### Construction

```zoya
let c = Color::Red
let circle = Shape::Circle(5.0)
let rect = Shape::Rectangle(3.0, 4.0)
let point = Shape::Point { x: 1.0, y: 2.0 }
```

### Generic Enums

```zoya
enum Option<T> {
  Some(T),
  None,
}

enum Result<T, E> {
  Ok(T),
  Err(E),
}

let maybe = Option::Some(42)
let nothing: Option<Int> = Option::None
```

## Type Aliases

Transparent type synonyms that create no new type.

### Simple Aliases

```zoya
type UserId = Int
type Name = String

let id: UserId = 42
let name: Name = "Alice"
```

### Generic Aliases

```zoya
type Pair<A, B> = (A, B)
type Callback<T> = T -> ()

let p: Pair<Int, String> = (1, "one")
```

## Type Parameters

Generic type parameters use PascalCase names.

### Declaration

```zoya
fn identity<T>(x: T) -> T x

struct Box<T> {
  value: T,
}

enum Either<L, R> {
  Left(L),
  Right(R),
}
```

### Instantiation with Turbofish

When type inference is insufficient, use turbofish syntax to specify type arguments:

```zoya
let x = identity::<Int>(42)
let box = Box::<String> { value: "hello" }
let left = Either::<Int, String>::Left(42)
```

## Type Annotations

### In Let Bindings

```zoya
let x: Int = 42
let name: String = "Alice"
let numbers: List<Int> = [1, 2, 3]
```

### In Function Signatures

```zoya
fn double(x: Int) -> Int x * 2
fn add(x: Int, y: Int) -> Int { x + y }
```

### In Lambda Expressions

```zoya
let f = |x: Int| -> Int x * 2
let pred = |x: Int| -> Bool x > 0
```

### Inference

Type annotations are often optional when types can be inferred:

```zoya
let x = 42           // Inferred as Int
let f = |x| x + 1    // Inferred as Int -> Int from usage
```
