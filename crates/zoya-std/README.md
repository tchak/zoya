# zoya-std

Standard library for the Zoya programming language.

Provides built-in type definitions and methods as a pre-compiled and cached `CheckedPackage`. The standard library is written in Zoya itself and compiled at startup.

## Included Types and Modules

### Option\<T\>

```zoya
pub enum Option<T> { None, Some(T) }

impl<T> Option<T> {
    pub fn map<U>(self, f: (T) -> U) -> Option<U>
    pub fn and_then<U>(self, f: (T) -> Option<U>) -> Option<U>
    pub fn is_some(self) -> Bool
    pub fn is_none(self) -> Bool
    pub fn unwrap(self) -> T
    pub fn expect(self, msg: String) -> T
    pub fn unwrap_or(self, default: T) -> T
    pub fn unwrap_or_else(self, f: () -> T) -> T
    pub fn filter(self, f: (T) -> Bool) -> Option<T>
    pub fn map_or<U>(self, default: U, f: (T) -> U) -> U
    pub fn map_or_else<U>(self, default: () -> U, f: (T) -> U) -> U
    pub fn zip<U>(self, other: Option<U>) -> Option<(T, U)>
    pub fn or(self, other: Option<T>) -> Option<T>
    pub fn or_else(self, f: () -> Option<T>) -> Option<T>
    pub fn and<U>(self, other: Option<U>) -> Option<U>
    pub fn ok_or<E>(self, err: E) -> Result<T, E>
    pub fn ok_or_else<E>(self, f: () -> E) -> Result<T, E>
}
```

### Result\<T, E\>

```zoya
pub enum Result<T, E> { Ok(T), Err(E) }

impl<T, E> Result<T, E> {
    pub fn map<U>(self, f: (T) -> U) -> Result<U, E>
    pub fn and_then<U>(self, f: (T) -> Result<U, E>) -> Result<U, E>
    pub fn is_ok(self) -> Bool
    pub fn is_err(self) -> Bool
    pub fn unwrap(self) -> T
    pub fn expect(self, msg: String) -> T
    pub fn unwrap_or(self, default: T) -> T
    pub fn unwrap_or_else(self, f: (E) -> T) -> T
    pub fn unwrap_err(self) -> E
    pub fn expect_err(self, msg: String) -> E
    pub fn map_err<F>(self, f: (E) -> F) -> Result<T, F>
    pub fn or<F>(self, other: Result<T, F>) -> Result<T, F>
    pub fn or_else<F>(self, f: (E) -> Result<T, F>) -> Result<T, F>
    pub fn and<U>(self, other: Result<U, E>) -> Result<U, E>
    pub fn ok(self) -> Option<T>
    pub fn err(self) -> Option<E>
}
```

### Primitive Type Methods

```zoya
// Int methods: abs, to_string, to_float, min, max, pow, clamp, signum,
//              is_positive, is_negative, is_zero, to_bigint

// Float methods: abs, to_string, to_int, floor, ceil, round, sqrt, min, max,
//                pow, clamp, signum, is_positive, is_negative, is_zero

// BigInt methods: abs, to_string, min, max, pow, clamp, signum,
//                 is_positive, is_negative, is_zero, to_int

// String methods: len, is_empty, contains, starts_with, ends_with, to_uppercase,
//                 to_lowercase, trim, trim_start, trim_end, replace, repeat, split,
//                 chars, find, slice, reverse, replace_first, pad_start, pad_end,
//                 lines, to_int, to_float

// List methods: len, is_empty, reverse, push, concat, map, filter, fold,
//               filter_map, first, last, truncate, insert, remove

// Dict methods: new, get, insert, remove, keys, values, len, has, from, is_empty

// Set methods: new, contains, insert, remove, len, to_list, is_disjoint,
//              is_subset, is_superset, difference, intersection, union, from, is_empty
```

### Set\<T\>

```zoya
impl<T> Set<T> {
    pub fn new() -> Self
    pub fn contains(self, value: T) -> Bool
    pub fn insert(self, value: T) -> Self
    pub fn remove(self, value: T) -> Self
    pub fn len(self) -> Int
    pub fn to_list(self) -> List<T>
    pub fn is_disjoint(self, other: Self) -> Bool
    pub fn is_subset(self, other: Self) -> Bool
    pub fn is_superset(self, other: Self) -> Bool
    pub fn difference(self, other: Self) -> Self
    pub fn intersection(self, other: Self) -> Self
    pub fn union(self, other: Self) -> Self
    pub fn from(items: List<T>) -> Self
    pub fn is_empty(self) -> Bool
}
```

### HTTP Types

```zoya
pub type Headers = Dict<String, String>

pub enum Method { Get, Post, Put, Patch, Delete, Head, Options }
pub enum Body { Text(String), Json(JSON) }

pub struct Request { url: String, method: Method, body: Option<Body>, headers: Headers }
pub struct Response { body: Option<Body>, status: Int, headers: Headers }

impl Response {
    pub fn ok(body: Option<Body>) -> Response
}
```

### Other Modules

- **io** - IO operations (`println`)
- **json** - JSON type, `Number` enum, `parse` function, `ParseError` enum
- **http** - HTTP types (`Request`, `Response`, `Method`, `Body`, `Headers`)
- **set** - Set methods (`new`, `insert`, `remove`, `union`, `intersection`, etc.)
- **bytes** - `Bytes` type for binary data
- **task** - Task/job support utilities
- **prelude** - Re-exports all standard types and variants for automatic injection

### Built-in Functions

```zoya
pub fn panic<T>(message: String) -> T
pub fn assert(condition: Bool) -> ()
pub fn assert_eq<V>(left: V, right: V) -> ()
pub fn assert_ne<V>(left: V, right: V) -> ()
```

## Usage

```rust
use zoya_std::std;
use zoya_ir::Definition;
use zoya_package::QualifiedPath;

// Get the standard library (lazily compiled and cached)
let std_pkg = std();

// Access the Option enum definition
let option_path = QualifiedPath::root().child("option").child("Option");
let def = std_pkg.definitions.get(&option_path).unwrap();
assert!(matches!(def, Definition::Enum(_)));

// Pass as dependency to the type checker
use zoya_check::check;
let checked = check(&user_pkg, &[std_pkg])?;
```

The standard library is a `&'static CheckedPackage` - it is compiled once and cached for the lifetime of the process.

## Module Structure

```
root
├── bigint     # BigInt methods
├── dict       # Dict<K, V> type and methods
├── float      # Float methods
├── http       # HTTP Request/Response types
├── int        # Int methods
├── io         # IO operations (println)
├── json       # JSON type and parsing
├── list       # List<T> methods
├── option     # Option<T> enum and methods
├── prelude    # Re-exports for auto-injection
├── result     # Result<T, E> enum and methods
├── set        # Set<T> methods
├── string     # String methods
├── bytes      # Bytes type
└── task       # Task/job utilities
```

## Error Handling

```rust
use zoya_std::StdError;

/// Error when loading or compiling the standard library.
pub enum StdError {
    /// Failed to load std .zy source files
    Load(zoya_loader::LoaderError<String>),
    /// Failed to type-check std package
    Check(zoya_ir::TypeError),
}
```

`StdError` uses `#[from]` for automatic `?` propagation from both `LoaderError<String>` and `TypeError`.

## Dependencies

- [zoya-check](../zoya-check) - Type checker (compiles the .zy sources)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-loader](../zoya-loader) - Package loading (via `MemorySource`)
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
