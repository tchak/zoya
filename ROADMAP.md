# Roadmap

## Completed

- **Impl blocks** - Methods and associated functions on user-defined and primitive types
- **Standard library expansion** - `Option<T>`, `Result<T, E>` with full method sets, methods on `Int`, `Float`, `BigInt`, `String`, `List<T>`, `Dict<K, V>`
- **String interpolation** - `$"hello {name}!"` syntax
- **Dict type** - Persistent `Dict<K, V>` backed by HAMT
- **List spread** - `[0, ..xs, 4]` syntax
- **Modulo and power operators** - `%` and `**`
- **Source formatter** - `zoya fmt` command
- **Test runner** - `zoya test` command

## Planned

Planned features in rough implementation order:

1. **Traits** - Shared behavior definitions
   - `trait Display { fn to_string(self) -> String }`
   - `impl Display for Point { ... }`

2. **Trait-based operators** - Operators defined via traits
   - `+` requires `Add` trait, `==` requires `Eq` trait, etc.
   - Enables operator overloading for user types

3. **Common traits** - Standard trait library
   - `Eq`, `Ord`, `Display`, `Default`, `Hash`
   - Trait bounds on generic functions: `fn sort<T: Ord>(list: List<T>) -> List<T>`
