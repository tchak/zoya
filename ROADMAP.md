# Roadmap

Planned features in rough implementation order:

1. **Type aliases** - Named type synonyms
   - `type MyResult = Result<(Int, Int), String>`
   - `type Callback<T> = T -> Bool`
   - Generic aliases with type parameters

2. **Module/package system** - Code organization and reuse
   - Module definitions and imports
   - Public/private visibility
   - Package management

3. **impl blocks** - Methods on user-defined types
   - `impl Point { fn distance(self) -> Float { ... } }`
   - Generic methods: `impl<T> Option<T> { fn unwrap(self) -> T { ... } }`

4. **Traits** - Shared behavior definitions
   - `trait Display { fn to_string(self) -> String }`
   - `impl Display for Point { ... }`

5. **Trait-based operators** - Operators defined via traits
   - `+` requires `Add` trait, `==` requires `Eq` trait, etc.
   - Enables operator overloading for user types

6. **Standard library expansion** - Once trait infrastructure exists
   - `Option<T>`, `Result<T, E>` with full method sets
   - `map`, `filter`, `fold` on List via traits
   - Common traits: `Eq`, `Ord`, `Display`, `Default`
