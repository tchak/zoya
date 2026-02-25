# Types

Zoya has the following built-in types:

| Type | Examples |
|------|----------|
| `Int` | `42`, `1_000`, `-5` |
| `BigInt` | `42n`, `9_000_000_000n` |
| `Float` | `3.14`, `0.5` |
| `Bool` | `true`, `false` |
| `String` | `"hello"`, `"line\nbreak"`, `$"hello {name}!"` |
| `List<T>` | `[1, 2, 3]`, `[]`, `[1, ..rest]` |
| `Set<T>` | `Set::new()`, `s.insert(v)` — immutable set |
| `Dict<K, V>` | `Dict::new()`, `d.insert(k, v)` — immutable dictionary |
| `Task<T>` | `Task::of(42)`, `t.map(f)` — lazy async computation |
| `Bytes` | `Bytes::from_string("hi")`, `Bytes::from_list([72, 105])` — raw binary data |
| `(T, U, ...)` | `(1, "hello")`, `()`, `(42,)`, `(..a, ..b)` — access elements with `.0`, `.1` |
| `T -> U` | `Int -> Bool`, `(Int, Int) -> Int` |

### String Interpolation

Interpolated strings use `$"..."` syntax to embed expressions:

```zoya
let name = "world";
$"hello {name}!"           // "hello world!"
$"1 + 2 = {1 + 2}"        // "1 + 2 = 3"
$"pi is {3.14}"            // "pi is 3.14"
$"big: {42n}"              // "big: 42"
$"literal \{ braces \}"    // "literal { braces }"
```

Only `String`, `Int`, `Float`, and `BigInt` types can be interpolated.

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
