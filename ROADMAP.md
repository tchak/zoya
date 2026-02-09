# Roadmap

Planned features in rough implementation order:

1. **impl blocks** - Methods on user-defined types
   - `impl Point { fn distance(self) -> Float { ... } }`
   - Generic methods: `impl<T> Option<T> { fn unwrap(self) -> T { ... } }`

2. **Traits** - Shared behavior definitions
   - `trait Display { fn to_string(self) -> String }`
   - `impl Display for Point { ... }`

3. **Trait-based operators** - Operators defined via traits
   - `+` requires `Add` trait, `==` requires `Eq` trait, etc.
   - Enables operator overloading for user types

4. **Standard library expansion** - Once trait infrastructure exists
   - `Option<T>`, `Result<T, E>` with full method sets (requires impl blocks)
   - `map`, `filter`, `fold` on List via traits
   - Common traits: `Eq`, `Ord`, `Display`, `Default`
