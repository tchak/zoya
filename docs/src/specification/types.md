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

## Dict Types

Immutable dictionaries mapping keys to values, backed by a persistent hash array mapped trie (HAMT).

### Syntax

```
Dict<K, V>
```

### Usage

Dict has no literal syntax. Create and manipulate dictionaries using methods:

```zoya
let d = Dict::new()
let d = d.insert("a", 1)
let d = d.insert("b", 2)
d.get("a")    // Some(1)
d.get("c")    // None
d.len()       // 2
d.keys()      // List of keys
d.values()    // List of values
d.remove("a") // New dict without "a"
d.is_empty()  // false
```

Keys can be any type that supports structural equality. Both keys and values must be homogeneous within a single dict.

## Set Types

Immutable sets backed by a persistent hash array mapped trie (HAMT). Single type parameter for element type.

### Syntax

```
Set<T>
```

### Usage

Set has no literal syntax. Create and manipulate sets using methods:

```zoya
let s = Set::new()
let s = s.insert(1)
let s = s.insert(2)
s.contains(1)         // true
s.contains(3)         // false
s.len()               // 2
s.remove(1)           // New set without 1
s.is_empty()          // false
```

### Set Operations

```zoya
let a = Set::from([1, 2, 3])
let b = Set::from([2, 3, 4])
a.union(b)            // {1, 2, 3, 4}
a.intersection(b)     // {2, 3}
a.difference(b)       // {1}
a.is_subset(b)        // false
a.is_superset(b)      // false
a.is_disjoint(b)      // false
```

Elements can be any type that supports structural equality. Elements must be homogeneous within a single set.

## Task Types

Lazy asynchronous computations. A `Task<T>` represents a computation that, when executed, produces a value of type `T`. Tasks are lazy — they describe work to be done but do not execute until the runtime drives them.

### Syntax

```
Task<T>
```

### Usage

Task has no literal syntax. Create tasks using methods:

```zoya
let t = Task::of(42)
let mapped = t.map(|x| x + 1)
let chained = t.and_then(|x| Task::of(x * 2))
let all = Task::all([Task::of(1), Task::of(2)])
let zipped = Task::zip(Task::of(1), Task::of("a"))
```

When a `main` function returns `Task<T>`, the runtime executes the task and produces the inner value.

## Bytes Type

Raw binary data, backed by JavaScript's `Uint8Array`. Non-generic, immutable.

### Syntax

```
Bytes
```

### Usage

Bytes has no literal syntax. Create byte sequences using methods:

```zoya
let b = Bytes::from_string("hello")
let b = Bytes::from_list([72, 101, 108, 108, 111])
b.len()                    // 5
b.get(0)                   // Option::Some(72)
b.slice(0, 2)              // First 2 bytes
b.concat(other)            // Concatenate two byte sequences
b.to_list()                // Convert to List<Int>
b.to_string()              // Decode as UTF-8 string
```

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

### Unit Structs

Structs without fields can be defined without braces. They are constructed and pattern-matched using a bare path:

```zoya
struct Token

let t = Token
match t {
  Token => "matched",
}
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
let left = Either::Left::<Int, String>(42)
```

## Self Type

Inside an [impl block](definitions.md#impl-blocks), the identifier `Self` refers to the target type of that block. It can be used in parameter types and return types of methods and associated functions.

For a non-generic impl block, `Self` resolves directly to the named type:

```zoya
struct Point { x: Int, y: Int }

impl Point {
    fn origin() -> Self { ... }    // Self = Point
    fn mirror(self) -> Self { ... } // Self = Point
}
```

For a generic impl block, `Self` includes the type parameters:

```zoya
struct Wrapper<T> { value: T }

impl<T> Wrapper<T> {
    fn new(v: T) -> Self { ... }   // Self = Wrapper<T>
}
```

Using `Self` outside of an impl block is an error:

```zoya
fn bad() -> Self { ... }   // Error: `Self` can only be used inside an impl block
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
