# Functions

## Basic Functions

```zoya
fn add(x: Int, y: Int) -> Int {
    x + y
}
```

## Single-Expression Bodies

Functions with a single expression can omit braces:

```zoya
fn square(x: Int) -> Int x * x
```

## Generic Functions

```zoya
fn identity<T>(x: T) -> T x
```

## Pattern Destructuring in Parameters

```zoya
fn swap((a, b): (Int, Int)) -> (Int, Int) (b, a)

fn get_x(Point { x, .. }: Point) -> Int x
```

## Let Bindings

```zoya
let x = 42                      // Type inferred as Int
let y: Float = 3.14             // Explicit type annotation
let (a, b) = (1, 2)             // Tuple destructuring
let Point { x, y } = point      // Struct destructuring
let (first, ..) = long_tuple    // Rest patterns
let pair @ (a, b) = (1, 2)      // As-patterns (bind whole and parts)
```

## Lambdas

```zoya
let inc = |x| x + 1
let add = |x, y| x + y
let typed = |x: Int| -> Int x * 2
let block = |x| { let y = x * 2; y + 1 }

// Pattern destructuring
let get_x = |Point { x, .. }| x
let sum_pair = |(a, b)| a + b
```

## Job Functions

The `#[job]` attribute marks a function as externally callable. Unlike `#[test]`, job functions can have parameters and any return type, and are included in all compilation modes.

```zoya
#[job]
fn deploy(name: String) -> String {
    $"deployed: {name}"
}
```
