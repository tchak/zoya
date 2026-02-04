# Enums

Enums are sum types (tagged unions) that can hold different variants.

## Defining Enums

```zoya
// Simple variants
enum Color { Red, Green, Blue }

// Generic enums with data
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Err(E) }

// Mixed variant styles
enum Message {
    Quit,
    Move { x: Int, y: Int },
    Write(String),
}
```

## Creating Variants

```zoya
let color = Color::Red
let maybe = Option::Some(42)
let msg = Message::Move { x: 10, y: 20 }
```

## Turbofish Syntax

For explicit type arguments when they can't be inferred:

```zoya
let none = Option::None::<Int>
```

## Matching on Enums

See [Pattern Matching](pattern-matching.md) for how to work with enum values.
