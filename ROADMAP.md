# Roadmap

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
