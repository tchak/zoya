# Structs

Structs are product types with named fields.

## Defining Structs

```zoya
struct Point { x: Int, y: Int }
struct Pair<T, U> { first: T, second: U }
struct Empty
```

## Creating Instances

```zoya
let p = Point { x: 1, y: 2 }
```

## Field Access

```zoya
let x_coord = p.x
```

## Field Shorthand

When variable names match field names:

```zoya
let x = 10
let y = 20
let p = Point { x, y }  // Same as Point { x: x, y: y }
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

## Destructuring

```zoya
let Point { x, y } = p
let Point { x, .. } = p  // Ignore other fields
```
