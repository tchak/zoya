# Standard Library

Zoya's standard library provides common types for everyday programming.

## Prelude

The following types and functions are automatically available in every module without explicit imports:

- **`Option<T>`** — Represents an optional value: `Some(T)` or `None`
- **`Result<T, E>`** — Represents success (`Ok(T)`) or failure (`Err(E)`)
- **`panic(message: String) -> T`** — Aborts execution with an error message. The generic return type `T` allows `panic` to be used in any type context.
- **`assert(condition: Bool) -> T`** — Panics with "assertion failed" if the condition is `false`.
- **`assert_eq(left: V, right: V) -> T`** — Panics with "assertion failed: left != right" if `left` and `right` are not equal (deep equality).
- **`assert_ne(left: V, right: V) -> T`** — Panics with "assertion failed: left == right" if `left` and `right` are equal (deep equality).
- **`println(message: String) -> ()`** — Prints a string to stdout followed by a newline.

```zoya
let x = Some(42)
let y: Option<Int> = None

let ok = Ok("success")
let err: Result<String, Int> = Err(404)
```

### Option Methods

#### `map`

Transforms the contained value using a function. Returns `None` if the option is `None`.

```zoya
Some(5).map(|x| x * 2)    // Some(10)
None.map(|x: Int| x * 2)  // None
```

#### `and_then`

Calls a function that returns an `Option` on the contained value. Useful for chaining operations that may fail.

```zoya
Some(5).and_then(|x| Some(x + 1))  // Some(6)
Some(5).and_then(|x| None::<Int>)  // None
```

Methods can be chained:

```zoya
Some(5).map(|x| x + 1).and_then(|x| Some(x * 2))  // Some(12)
```

### Result Methods

#### `map`

Transforms the success value using a function. Returns the error unchanged if the result is `Err`.

```zoya
Ok(5).map(|x| x * 2)              // Ok(10)
Err("fail").map(|x: Int| x * 2)   // Err("fail")
```

#### `and_then`

Calls a function that returns a `Result` on the success value. Useful for chaining operations that may fail.

```zoya
Ok(5).and_then(|x| Ok(x + 1))    // Ok(6)
Ok(5).and_then(|x| Err("fail"))  // Err("fail")
```

### `panic`

`panic` terminates the program with an error message. Because its return type is generic (`T`), it can be used anywhere any type is expected:

```zoya
fn divide(a: Int, b: Int) -> Int {
    match b {
        0 => panic("division by zero"),
        _ => a / b,
    }
}
```

### `assert`

`assert` checks that a condition is `true`, and panics if it is `false`:

```zoya
assert(1 + 1 == 2)
assert(true)
```

### `assert_eq`

`assert_eq` checks that two values are equal using deep equality, and panics if they are not:

```zoya
assert_eq(1 + 1, 2)
assert_eq([1, 2, 3], [1, 2, 3])
```

### `assert_ne`

`assert_ne` checks that two values are not equal using deep equality, and panics if they are:

```zoya
assert_ne(1, 2)
assert_ne([1, 2], [3, 4])
```

### `println`

`println` prints a string to stdout followed by a newline:

```zoya
println("Hello, World!")

let name = "Alice"
println("Hello, " + name + "!")
```

## `std::io`

Basic I/O operations. `println` is re-exported in the prelude, so it can be used without an explicit import.

### `println`

```zoya
use std::io::println

println("Hello from std::io!")
```

## `std::json`

Types for representing arbitrary JSON data structures. Must be imported explicitly.

### `Number`

Represents a JSON number, either integer or floating-point.

```zoya
use std::json::Number

let i = Number::Int(42)
let f = Number::Float(3.14)
```

### `JSON`

Represents an arbitrary JSON value.

| Variant | Description |
|---------|-------------|
| `Null` | JSON null |
| `Bool(Bool)` | JSON boolean |
| `Number(Number)` | JSON number |
| `String(String)` | JSON string |
| `Array(List<JSON>)` | JSON array |
| `Object(List<(String, JSON)>)` | JSON object as key-value pairs |

```zoya
use std::json::{JSON, Number}

let data = JSON::Object([
    ("name", JSON::String("Alice")),
    ("age", JSON::Number(Number::Int(30))),
    ("active", JSON::Bool(true)),
    ("scores", JSON::Array([
        JSON::Number(Number::Float(9.5)),
        JSON::Number(Number::Float(8.0)),
    ])),
])

match data {
    JSON::Object(entries) => entries,
    _ => [],
}
```
