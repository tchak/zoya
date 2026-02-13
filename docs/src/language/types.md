# Types

Zoya has the following built-in types:

| Type | Examples |
|------|----------|
| `Int` | `42`, `1_000`, `-5` |
| `BigInt` | `42n`, `9_000_000_000n` |
| `Float` | `3.14`, `0.5` |
| `Bool` | `true`, `false` |
| `String` | `"hello"`, `"line\nbreak"` |
| `List<T>` | `[1, 2, 3]`, `[]`, `[1, ..rest]` |
| `Dict<K, V>` | `Dict::new()`, `d.insert(k, v)` — immutable dictionary |
| `(T, U, ...)` | `(1, "hello")`, `()`, `(42,)`, `(..a, ..b)` — access elements with `.0`, `.1` |
| `T -> U` | `Int -> Bool`, `(Int, Int) -> Int` |

## Type Inference

Zoya uses Hindley-Milner type inference. You rarely need to write type annotations:

```zoya
let x = 42              // Inferred as Int
let y = 3.14            // Inferred as Float
let double = |x| x * 2    // Inferred as Int -> Int
```

## Type Annotations

You can add explicit annotations when needed:

```zoya
let y: Float = 3.14
let numbers: List<Int> = [1, 2, 3]
```

## Type Aliases

Create named synonyms for types:

```zoya
type UserId = Int
type Callback = (Int) -> Bool
type Pair<A, B> = (A, B)
type StringList = List<String>

fn get_user(id: UserId) -> String "user"
fn make_pair() -> Pair<Int, Bool> (1, true)
```

Type aliases are transparent - `UserId` and `Int` are interchangeable everywhere.
