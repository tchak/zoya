# Standard Library

Zoya's standard library provides common types for everyday programming.

## Prelude

The following types and functions are automatically available in every module without explicit imports:

- **`Option<T>`** — Represents an optional value: `Some(T)` or `None`
- **`Result<T, E>`** — Represents success (`Ok(T)`) or failure (`Err(E)`)
- **`panic(message: String) -> T`** — Aborts execution with an error message. The generic return type `T` allows `panic` to be used in any type context.

```zoya
let x = Some(42)
let y: Option<Int> = None

let ok = Ok("success")
let err: Result<String, Int> = Err(404)
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
