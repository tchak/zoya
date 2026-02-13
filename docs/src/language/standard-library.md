# Standard Library

Zoya's standard library provides common types and methods for everyday programming. It includes methods on primitive types (Int, Float, String, BigInt, List, Dict) defined via `impl` blocks in dedicated modules (`std::int`, `std::float`, `std::string`, `std::bigint`, `std::list`, `std::dict`). See [Methods](methods.md) for the full list of primitive type methods.

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

## `std::string`

Methods on the `String` type.

### `String`

| Method | Description |
|--------|-------------|
| `len(self) -> Int` | Return the number of characters |
| `is_empty(self) -> Bool` | Check if the string is empty |
| `contains(self, needle: String) -> Bool` | Check if the string contains `needle` |
| `starts_with(self, prefix: String) -> Bool` | Check if the string starts with `prefix` |
| `ends_with(self, suffix: String) -> Bool` | Check if the string ends with `suffix` |
| `to_uppercase(self) -> String` | Convert to uppercase |
| `to_lowercase(self) -> String` | Convert to lowercase |
| `trim(self) -> String` | Remove leading and trailing whitespace |
| `trim_start(self) -> String` | Remove leading whitespace |
| `trim_end(self) -> String` | Remove trailing whitespace |
| `replace(self, from: String, to: String) -> String` | Replace all occurrences of `from` with `to` |
| `repeat(self, n: Int) -> String` | Repeat the string `n` times |
| `split(self, sep: String) -> List<String>` | Split the string by `sep` |
| `chars(self) -> List<String>` | Split into individual characters |
| `find(self, needle: String) -> Option<Int>` | Find the index of `needle`, or `None` |
| `slice(self, start: Int, end: Int) -> String` | Extract a substring from `start` to `end` |
| `reverse(self) -> String` | Reverse the string |
| `replace_first(self, from: String, to: String) -> String` | Replace the first occurrence of `from` with `to` |
| `pad_start(self, len: Int, fill: String) -> String` | Pad the start to reach `len` using `fill` |
| `pad_end(self, len: Int, fill: String) -> String` | Pad the end to reach `len` using `fill` |
| `lines(self) -> List<String>` | Split by newlines |
| `to_int(self) -> Option<Int>` | Parse as integer, or `None` |
| `to_float(self) -> Option<Float>` | Parse as float, or `None` |

```zoya
let s = "Hello, World!"
s.len()                    // 13
s.contains("World")        // true
s.replace("World", "Zoya") // "Hello, Zoya!"
s.split(", ")              // ["Hello", "World!"]
"42".to_int()              // Some(42)
"abc".to_int()             // None
```

## `std::list`

Methods on the immutable `List<T>` type. All operations return new lists.

### `List<T>`

| Method | Description |
|--------|-------------|
| `len(self) -> Int` | Return the number of elements |
| `is_empty(self) -> Bool` | Check if the list is empty |
| `push(self, item: T) -> Self` | Return a new list with the item appended |
| `reverse(self) -> Self` | Return a new list in reverse order |
| `first(self) -> Option<T>` | Return the first element, or `None` if empty |
| `last(self) -> Option<T>` | Return the last element, or `None` if empty |
| `map<U>(self, f: T -> U) -> List<U>` | Transform each element with a function |
| `filter(self, f: T -> Bool) -> Self` | Keep only elements where the predicate is true |
| `fold<U>(self, init: U, f: (U, T) -> U) -> U` | Reduce the list to a single value |
| `filter_map<U>(self, f: T -> Option<U>) -> List<U>` | Filter and transform in a single pass |
| `truncate(self, len: Int) -> Self` | Return the first `len` elements |
| `insert(self, index: Int, value: T) -> Self` | Return a new list with `value` inserted at `index` |
| `remove(self, index: Int) -> Self` | Return a new list with the element at `index` removed |

```zoya
let xs = [1, 2, 3, 4, 5]
xs.map(|x| x * 2)                // [2, 4, 6, 8, 10]
xs.filter(|x| x > 3)             // [4, 5]
xs.fold(0, |acc, x| acc + x)     // 15
xs.first()                        // Some(1)
xs.last()                         // Some(5)
xs.truncate(3)                    // [1, 2, 3]
xs.insert(2, 99)                  // [1, 2, 99, 3, 4, 5]
xs.remove(0)                      // [2, 3, 4, 5]
```

## `std::dict`

Immutable dictionary type backed by a persistent hash array mapped trie (HAMT).

### `Dict<K, V>`

| Method | Description |
|--------|-------------|
| `Dict::new() -> Dict<K, V>` | Create an empty dictionary |
| `get(self, key: K) -> Option<V>` | Look up a key, returns `Some(value)` or `None` |
| `insert(self, key: K, value: V) -> Self` | Return a new dict with the key-value pair added (or replaced) |
| `remove(self, key: K) -> Self` | Return a new dict with the key removed |
| `keys(self) -> List<K>` | Return all keys as a list |
| `values(self) -> List<V>` | Return all values as a list |
| `len(self) -> Int` | Return the number of entries |
| `is_empty(self) -> Bool` | Check if the dictionary is empty |

```zoya
let d = Dict::new()
let d = d.insert("name", "Alice")
let d = d.insert("city", "Paris")
d.get("name")     // Some("Alice")
d.get("age")      // None
d.len()            // 2
d.keys()           // ["name", "city"] (order may vary)
let d = d.remove("city")
d.is_empty()       // false
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
