# Structs

Structs are product types that group related data together. Zoya supports three forms: named-field structs, tuple structs, and unit structs.

## Named-Field Structs

```zoya
struct Point { x: Int, y: Int }
struct Pair<T, U> { first: T, second: U }
```

### Creating Instances

```zoya
let p = Point { x: 1, y: 2 }
```

### Field Access

```zoya
let x_coord = p.x
```

### Field Shorthand

When variable names match field names:

```zoya
let x = 10
let y = 20
let p = Point { x, y }  // Same as Point { x: x, y: y }
```

### Destructuring

```zoya
let Point { x, y } = p
let Point { x, .. } = p  // Ignore other fields
```

## Tuple Structs

Tuple structs have positional fields. They are useful for simple wrappers and newtypes where named fields add noise.

```zoya
struct Wrapper(Int)
struct Pair(String, Int)
struct Triple<A, B, C>(A, B, C)
```

### Creating Instances

Tuple structs are constructed like function calls:

```zoya
let w = Wrapper(42)
let p = Pair("hello", 1)
```

### Field Access

Tuple struct fields can be accessed by index using dot notation:

```zoya
let w = Wrapper(42)
w.0                     // 42

let p = Pair("hello", 1)
p.0                     // "hello"
p.1                     // 1
```

### Destructuring

Tuple structs are destructured with parenthesized patterns:

```zoya
let Wrapper(value) = w
let Pair(name, id) = p
```

Spread patterns work like tuples:

```zoya
let Triple(first, ..) = triple        // Bind only first
let Triple(.., last) = triple          // Bind only last
let Triple(a, .., c) = triple          // Bind first and last
let Triple(a, rest @ ..) = triple      // Bind rest as a tuple
```

## Unit Structs

Structs without fields can be defined without braces:

```zoya
struct Token

let t = Token
match t {
    Token => "matched",
}
```
