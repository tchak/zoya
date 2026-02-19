# Standard Library

Zoya's standard library provides common types and methods for everyday programming. It includes methods on primitive types (Int, Float, String, BigInt, List, Set, Dict) defined via `impl` blocks in dedicated modules (`std::int`, `std::float`, `std::string`, `std::bigint`, `std::list`, `std::set`, `std::dict`).

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

#### `Option<T>`

| Method | Description |
|--------|-------------|
| `map<U>(self, f: T -> U) -> Option<U>` | Transform the contained value |
| `and_then<U>(self, f: T -> Option<U>) -> Option<U>` | Chain operations that return `Option` |
| `is_some(self) -> Bool` | Return `true` if `Some` |
| `is_none(self) -> Bool` | Return `true` if `None` |
| `unwrap(self) -> T` | Extract the value or panic |
| `expect(self, message: String) -> T` | Extract the value or panic with message |
| `unwrap_or(self, default: T) -> T` | Extract the value or return default |
| `unwrap_or_else(self, f: () -> T) -> T` | Extract the value or compute default |
| `filter(self, predicate: T -> Bool) -> Option<T>` | Keep `Some` only if predicate is true |
| `map_or<U>(self, default: U, f: T -> U) -> U` | Transform or return default |
| `map_or_else<U>(self, default_fn: () -> U, f: T -> U) -> U` | Transform or compute default |
| `zip<U>(self, other: Option<U>) -> Option<(T, U)>` | Combine two options into a tuple |
| `or(self, other: Option<T>) -> Option<T>` | Return `self` if `Some`, otherwise `other` |
| `or_else(self, f: () -> Option<T>) -> Option<T>` | Return `self` if `Some`, otherwise compute |
| `and<U>(self, other: Option<U>) -> Option<U>` | Return `other` if `self` is `Some`, otherwise `None` |
| `ok_or<E>(self, err: E) -> Result<T, E>` | Convert to `Result`, mapping `None` to `Err` |
| `ok_or_else<E>(self, f: () -> E) -> Result<T, E>` | Convert to `Result`, computing error for `None` |

```zoya
Some(5).map(|x| x * 2)              // Some(10)
Some(5).and_then(|x| Some(x + 1))   // Some(6)
Some(5).filter(|x| x > 3)           // Some(5)
Some(5).unwrap_or(0)                 // 5
None.unwrap_or(0)                    // 0

// Chaining
Some(5).map(|x| x + 1).and_then(|x| Some(x * 2))  // Some(12)

// Boolean ops
Some(1).or(Some(2))         // Some(1)
None.or(Some(2))            // Some(2)
Some(1).and(Some("hello"))  // Some("hello")

// Conversions
Some(5).ok_or("missing")    // Ok(5)
None.ok_or("missing")       // Err("missing")
Some(1).zip(Some("a"))      // Some((1, "a"))
```

### Result Methods

#### `Result<T, E>`

| Method | Description |
|--------|-------------|
| `map<U>(self, f: T -> U) -> Result<U, E>` | Transform the success value |
| `and_then<U>(self, f: T -> Result<U, E>) -> Result<U, E>` | Chain operations that return `Result` |
| `is_ok(self) -> Bool` | Return `true` if `Ok` |
| `is_err(self) -> Bool` | Return `true` if `Err` |
| `unwrap(self) -> T` | Extract the success value or panic |
| `expect(self, message: String) -> T` | Extract the success value or panic with message |
| `unwrap_or(self, default: T) -> T` | Extract the success value or return default |
| `unwrap_or_else(self, f: E -> T) -> T` | Extract the success value or compute from error |
| `unwrap_err(self) -> E` | Extract the error value or panic |
| `expect_err(self, message: String) -> E` | Extract the error value or panic with message |
| `map_err<F>(self, f: E -> F) -> Result<T, F>` | Transform the error value |
| `or(self, other: Result<T, E>) -> Result<T, E>` | Return `self` if `Ok`, otherwise `other` |
| `or_else(self, f: E -> Result<T, E>) -> Result<T, E>` | Return `self` if `Ok`, otherwise compute from error |
| `and<U>(self, other: Result<U, E>) -> Result<U, E>` | Return `other` if `self` is `Ok`, otherwise propagate error |
| `ok(self) -> Option<T>` | Convert to `Option`, discarding error |
| `err(self) -> Option<E>` | Convert error to `Option`, discarding success |

```zoya
Ok(5).map(|x| x * 2)              // Ok(10)
Ok(5).and_then(|x| Ok(x + 1))     // Ok(6)
Ok(42).unwrap_or(0)                // 42
Err("fail").unwrap_or(0)           // 0

// Error handling
Err("fail").map_err(|e: String| e.len())  // Err(4)

// Boolean ops
Ok(1).or(Ok(2))            // Ok(1)
Err("fail").or(Ok(2))      // Ok(2)
Ok(1).and(Ok("hello"))     // Ok("hello")

// Conversions
Ok(42).ok()                // Some(42)
Err("fail").ok()           // None
Err("fail").err()          // Some("fail")
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

## `std::int`

Methods on the `Int` type.

### `Int`

| Method | Description |
|--------|-------------|
| `abs(self) -> Int` | Return the absolute value |
| `to_string(self) -> String` | Convert to string representation |
| `to_float(self) -> Float` | Convert to floating-point |
| `min(self, other: Int) -> Int` | Return the smaller of two values |
| `max(self, other: Int) -> Int` | Return the larger of two values |
| `pow(self, exp: Int) -> Int` | Raise to the power of `exp` |
| `clamp(self, min: Int, max: Int) -> Int` | Clamp value between `min` and `max` |
| `signum(self) -> Int` | Return -1, 0, or 1 based on sign |
| `is_positive(self) -> Bool` | Check if strictly positive |
| `is_negative(self) -> Bool` | Check if strictly negative |
| `is_zero(self) -> Bool` | Check if equal to zero |
| `to_bigint(self) -> BigInt` | Convert to BigInt |

```zoya
(-5).abs()              // 5
42.to_string()          // "42"
42.to_float()           // 42.0
3.min(5)                // 3
3.max(5)                // 5
2.pow(10)               // 1024
15.clamp(0, 10)         // 10
(-42).signum()          // -1
5.is_positive()         // true
(-3).is_negative()      // true
0.is_zero()             // true
42.to_bigint()          // 42n
```

## `std::float`

Methods on the `Float` type.

### `Float`

| Method | Description |
|--------|-------------|
| `abs(self) -> Float` | Return the absolute value |
| `to_string(self) -> String` | Convert to string representation |
| `to_int(self) -> Int` | Truncate to integer |
| `floor(self) -> Float` | Round down to nearest integer |
| `ceil(self) -> Float` | Round up to nearest integer |
| `round(self) -> Float` | Round to nearest integer |
| `sqrt(self) -> Float` | Return the square root |
| `min(self, other: Float) -> Float` | Return the smaller of two values |
| `max(self, other: Float) -> Float` | Return the larger of two values |
| `pow(self, exp: Float) -> Float` | Raise to the power of `exp` |
| `clamp(self, min: Float, max: Float) -> Float` | Clamp value between `min` and `max` |
| `signum(self) -> Float` | Return -1.0, 0.0, or 1.0 based on sign |
| `is_positive(self) -> Bool` | Check if strictly positive |
| `is_negative(self) -> Bool` | Check if strictly negative |
| `is_zero(self) -> Bool` | Check if equal to zero |

```zoya
3.14.floor()            // 3.0
3.14.ceil()             // 4.0
3.14.round()            // 3.0
4.0.sqrt()              // 2.0
(-3.14).abs()           // 3.14
3.14.to_string()        // "3.14"
3.7.to_int()            // 3
3.14.min(2.0)           // 2.0
3.14.max(5.0)           // 5.0
2.0.pow(3.0)            // 8.0
15.0.clamp(0.0, 10.0)   // 10.0
(-3.14).signum()        // -1.0
3.14.is_positive()      // true
(-3.14).is_negative()   // true
0.0.is_zero()           // true
```

## `std::bigint`

Methods on the `BigInt` type.

### `BigInt`

| Method | Description |
|--------|-------------|
| `abs(self) -> BigInt` | Return the absolute value |
| `to_string(self) -> String` | Convert to string representation |
| `min(self, other: BigInt) -> BigInt` | Return the smaller of two values |
| `max(self, other: BigInt) -> BigInt` | Return the larger of two values |
| `pow(self, exp: BigInt) -> BigInt` | Raise to the power of `exp` |
| `clamp(self, min: BigInt, max: BigInt) -> BigInt` | Clamp value between `min` and `max` |
| `signum(self) -> BigInt` | Return -1n, 0n, or 1n based on sign |
| `is_positive(self) -> Bool` | Check if strictly positive |
| `is_negative(self) -> Bool` | Check if strictly negative |
| `is_zero(self) -> Bool` | Check if equal to zero |
| `to_int(self) -> Int` | Convert to Int |

```zoya
(-5n).abs()             // 5n
42n.to_string()         // "42"
3n.min(5n)              // 3n
3n.max(5n)              // 5n
2n.pow(10n)             // 1024n
15n.clamp(0n, 10n)      // 10n
(-42n).signum()         // -1n
5n.is_positive()        // true
(-3n).is_negative()     // true
0n.is_zero()            // true
42n.to_int()            // 42
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

### Indexing

Lists support index access with bracket notation, returning `Option<T>`. Negative indices count from the end. Out-of-bounds access returns `None`.

```zoya
[10, 20, 30][0]         // Some(10)
[10, 20, 30][-1]        // Some(30)
[10, 20, 30][5]         // None
```

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

## `std::set`

Immutable set type backed by a persistent hash array mapped trie (HAMT). `Set` is re-exported in the prelude.

### `Set<T>`

| Method | Description |
|--------|-------------|
| `Set::new() -> Set<T>` | Create an empty set |
| `contains(self, value: T) -> Bool` | Check if a value exists in the set |
| `insert(self, value: T) -> Self` | Return a new set with the value added |
| `remove(self, value: T) -> Self` | Return a new set with the value removed |
| `len(self) -> Int` | Return the number of elements |
| `to_list(self) -> List<T>` | Return all elements as a list |
| `is_disjoint(self, other: Self) -> Bool` | Check if two sets have no elements in common |
| `is_subset(self, other: Self) -> Bool` | Check if all elements are in `other` |
| `is_superset(self, other: Self) -> Bool` | Check if `other`'s elements are all in `self` |
| `difference(self, other: Self) -> Self` | Elements in `self` but not in `other` |
| `intersection(self, other: Self) -> Self` | Elements in both `self` and `other` |
| `union(self, other: Self) -> Self` | Elements in either `self` or `other` |
| `Set::from(items: List<T>) -> Self` | Create a set from a list (duplicates removed) |
| `is_empty(self) -> Bool` | Check if the set is empty |

```zoya
let s = Set::new()
let s = s.insert(1).insert(2).insert(3)
s.contains(2)         // true
s.len()               // 3
s.remove(2).len()     // 2

// Create from list (duplicates are removed)
let s = Set::from([1, 2, 3, 2, 1])
s.len()               // 3

// Set operations
let a = Set::from([1, 2, 3])
let b = Set::from([2, 3, 4])
a.union(b).len()          // 5
a.intersection(b).len()   // 2
a.difference(b).len()     // 1
a.is_subset(b)            // false
a.is_disjoint(b)          // false
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
| `has(self, key: K) -> Bool` | Check if a key exists |
| `Dict::from(entries: List<(K, V)>) -> Self` | Create a dictionary from key-value pairs |
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

## `std::http`

Types for representing HTTP requests and responses. Must be imported explicitly.

### `Headers`

Type alias for `Dict<String, String>`.

### `Method`

Represents an HTTP method.

| Variant | Description |
|---------|-------------|
| `Get` | HTTP GET |
| `Post` | HTTP POST |
| `Put` | HTTP PUT |
| `Patch` | HTTP PATCH |
| `Delete` | HTTP DELETE |
| `Head` | HTTP HEAD |
| `Options` | HTTP OPTIONS |

### `Body`

Represents an HTTP body, either plain text or JSON.

| Variant | Description |
|---------|-------------|
| `Text(String)` | Plain text body |
| `Json(JSON)` | JSON body (uses `JSON` from `std::json`) |

### `Request`

Represents an HTTP request.

| Field | Type | Description |
|-------|------|-------------|
| `url` | `String` | The request URL |
| `method` | `Method` | The HTTP method |
| `body` | `Option<Body>` | Optional request body |
| `headers` | `Headers` | Request headers |

### `Response`

Represents an HTTP response.

| Field | Type | Description |
|-------|------|-------------|
| `body` | `Option<Body>` | Optional response body |
| `status` | `Int` | HTTP status code |
| `headers` | `Headers` | Response headers |

```zoya
use std::http::{Request, Response, Method, Body, Headers}
use std::json::JSON

let headers = Dict::from([("Content-Type", "application/json")])

let request = Request {
    url: "https://api.example.com/data",
    method: Method::Get,
    body: None,
    headers: headers,
}

let response = Response {
    body: Some(Body::Json(JSON::String("hello"))),
    status: 200,
    headers: Dict::new(),
}

match response.body {
    Some(Body::Json(json)) => json.to_string(),
    Some(Body::Text(text)) => text,
    None => "",
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
| `Object(Dict<String, JSON>)` | JSON object as key-value dictionary |

#### Methods

| Method | Description |
|--------|-------------|
| `to_string(self) -> String` | Serialize the JSON value to a string |

```zoya
use std::json::{JSON, Number, parse}

let data = JSON::Object(Dict::from([
    ("name", JSON::String("Alice")),
    ("age", JSON::Number(Number::Int(30))),
    ("active", JSON::Bool(true)),
    ("scores", JSON::Array([
        JSON::Number(Number::Float(9.5)),
        JSON::Number(Number::Float(8.0)),
    ])),
]))

match data {
    JSON::Object(dict) => dict.get("name"),
    _ => None,
}

// Serialize to JSON string
JSON::Number(Number::Int(42)).to_string()  // "42"
JSON::Bool(true).to_string()               // "true"
JSON::Null.to_string()                     // "null"

// Round-trip: parse and serialize
let json = parse("{\"key\": \"value\"}")
match json {
    Ok(value) => value.to_string(),  // "{\"key\":\"value\"}"
    Err(_) => "",
}
```
