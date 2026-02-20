# zoya-value

Runtime value types and serialization for the Zoya programming language.

This crate defines `Value`, the Rust representation of Zoya runtime values, and `JSValue`, an intermediate representation for bridging between JavaScript and Zoya. Extracted from `zoya-run` to allow reuse without depending on the QuickJS runtime.

## Features

- **Value enum** - Typed representation of all Zoya runtime values (Int, Float, String, List, Dict, Set, Struct, Enum, etc.)
- **Display formatting** - Human-readable output for all value types
- **JSON serialization** - `serde::Serialize` implementation with `to_json()` convenience method
- **Hash and Eq** - Full `Hash`/`Eq` support for use in `HashSet` and `HashMap` (including `Dict` and `Set` values)
- **JSValue bridge** - Runtime-agnostic intermediate representation for JS interop
- **QuickJS integration** - Optional `quickjs` feature for `IntoJs`/`FromJs` implementations
- **Type-guided conversion** - Convert `JSValue` to `Value` using Zoya type information

## Usage

### Creating values

```rust
use zoya_value::{Value, ValueData};
use zoya_ir::QualifiedPath;
use std::collections::{HashMap, HashSet};

// Primitive values
let int_val = Value::Int(42);
let float_val = Value::Float(3.14);
let bool_val = Value::Bool(true);
let str_val = Value::String("hello".into());
let bigint_val = Value::BigInt(999);

// Collections
let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
let tuple = Value::Tuple(vec![Value::Int(1), Value::String("hello".into())]);
let set = Value::Set(HashSet::from([Value::Int(1), Value::Int(2)]));
let dict = Value::Dict(HashMap::from([
    (Value::String("key".into()), Value::Int(42)),
]));

// Struct values
let point = Value::Struct {
    name: "Point".into(),
    module: QualifiedPath::new(vec!["root".into()]),
    data: ValueData::Struct(HashMap::from([
        ("x".into(), Value::Int(10)),
        ("y".into(), Value::Int(20)),
    ])),
};

// Enum variant values
let some_val = Value::EnumVariant {
    enum_name: "Option".into(),
    variant_name: "Some".into(),
    module: QualifiedPath::new(vec!["root".into(), "option".into()]),
    data: ValueData::Tuple(vec![Value::Int(42)]),
};

let none_val = Value::EnumVariant {
    enum_name: "Option".into(),
    variant_name: "None".into(),
    module: QualifiedPath::new(vec!["root".into(), "option".into()]),
    data: ValueData::Unit,
};
```

### Display formatting

```rust
use zoya_value::Value;

assert_eq!(format!("{}", Value::Int(42)), "42");
assert_eq!(format!("{}", Value::String("hello".into())), "\"hello\"");
assert_eq!(
    format!("{}", Value::List(vec![Value::Int(1), Value::Int(2)])),
    "[1, 2]"
);
assert_eq!(
    format!("{}", Value::Tuple(vec![Value::Int(1), Value::Bool(true)])),
    "(1, true)"
);
```

### JSON serialization

```rust
use zoya_value::{Value, ValueData};
use zoya_ir::QualifiedPath;

// Primitives serialize naturally
assert_eq!(Value::Int(42).to_json(), "42");
assert_eq!(Value::Bool(true).to_json(), "true");
assert_eq!(Value::String("hello".into()).to_json(), r#""hello""#);

// BigInts serialize as strings
assert_eq!(Value::BigInt(123).to_json(), r#""123""#);

// Collections serialize as JSON arrays
assert_eq!(
    Value::List(vec![Value::Int(1), Value::Int(2)]).to_json(),
    "[1,2]"
);

// Structs serialize with type information
let point = Value::Struct {
    name: "Point".into(),
    module: QualifiedPath::new(vec!["root".into()]),
    data: ValueData::Struct(std::collections::HashMap::from([
        ("x".into(), Value::Int(10)),
    ])),
};
// => {"type":"Point","data":{"x":10}}

// Enum variants include the qualified variant name
let color = Value::EnumVariant {
    enum_name: "Color".into(),
    variant_name: "Red".into(),
    module: QualifiedPath::new(vec!["root".into()]),
    data: ValueData::Unit,
};
assert_eq!(color.to_json(), r#"{"type":"Color::Red"}"#);
```

### JSValue bridge

`JSValue` is an intermediate representation decoupled from any specific JS runtime:

```rust
use zoya_value::JSValue;
use std::collections::HashMap;

// Create JSValues directly
let int_js = JSValue::Int(42);
let obj_js = JSValue::Object {
    tag: Some("Red".into()),
    fields: HashMap::new(),
};

// Convert from Value to JSValue
use zoya_value::Value;
let val = Value::Int(42);
let js: JSValue = val.into();
```

### Type-guided JS-to-Value conversion

```rust
use zoya_value::{Value, JSValue};
use zoya_ir::Type;

// Convert JSValue back to Value using type information
let js = JSValue::Int(42);
let val = Value::from_js_value(js, &Type::Int, &type_lookup)?;
assert_eq!(val, Value::Int(42));
```

## Error Handling

```rust
use zoya_value::Error;

/// Runtime value conversion errors.
pub enum Error {
    Panic(String),
    TypeMismatch { from: String, to: String },
    MissingField(String),
    UnknownVariant(String),
    UnsupportedConversion(String),
    ParseError(String),
}
```

These errors arise during type-guided conversion from `JSValue` to `Value`. `zoya-run` converts them to `EvalError` via a `From` impl.

## Feature Flags

| Feature | Description |
|---------|-------------|
| `quickjs` | Enables `IntoJs` and `FromJs` implementations for `JSValue` via `rquickjs` |

## Dependencies

- [zoya-ir](../zoya-ir) - Type definitions for type-guided conversion
- [serde](https://github.com/serde-rs/serde) - Serialization framework
- [serde_json](https://github.com/serde-rs/json) - JSON serialization
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
- [rquickjs](https://github.com/aspect-build/rquickjs) - QuickJS bindings (optional, `quickjs` feature)
