# Roadmap

Planned features in rough implementation order:

1. **Module/package system** - Code organization and reuse
   - Module definitions and imports
   - Public/private visibility
   - Package management

2. **impl blocks** - Methods on user-defined types
   - `impl Point { fn distance(self) -> Float { ... } }`
   - Generic methods: `impl<T> Option<T> { fn unwrap(self) -> T { ... } }`

3. **Traits** - Shared behavior definitions
   - `trait Display { fn to_string(self) -> String }`
   - `impl Display for Point { ... }`

4. **Trait-based operators** - Operators defined via traits
   - `+` requires `Add` trait, `==` requires `Eq` trait, etc.
   - Enables operator overloading for user types

5. **Standard library expansion** - Once trait infrastructure exists
   - `Option<T>`, `Result<T, E>` with full method sets
   - `map`, `filter`, `fold` on List via traits
   - Common traits: `Eq`, `Ord`, `Display`, `Default`
