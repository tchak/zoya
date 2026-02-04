# Pattern Matching

Pattern matching in Zoya is exhaustive - the compiler ensures all cases are covered.

## Match Expressions

```zoya
fn describe(opt: Option<Int>) -> String {
    match opt {
        Option::None => "nothing",
        Option::Some(0) => "zero",
        Option::Some(n) => n.to_string(),
    }
}
```

## List Patterns

```zoya
match list {
    [] => "empty",
    [x] => "single",
    [x, y] => "pair",
    [first, ..] => "has first",
    [.., last] => "has last",
    [first, .., last] => "has both",
}
```

## Tuple Patterns

```zoya
match tuple {
    (0, _) => "starts with zero",
    (_, 0) => "ends with zero",
    (a, b) => a + b,
}
```

## Struct Patterns

```zoya
match point {
    Point { x: 0, y: 0 } => "origin",
    Point { x: 0, y } => "on y-axis",
    Point { x, y: 0 } => "on x-axis",
    Point { x, y } => "somewhere else",
}
```

## Wildcard Pattern

Use `_` to match anything without binding:

```zoya
match value {
    Some(x) => x,
    _ => 0,  // Matches anything else
}
```

## As-Patterns

Bind the whole value while also destructuring:

```zoya
let pair @ (a, b) = (1, 2)
// pair = (1, 2), a = 1, b = 2
```
