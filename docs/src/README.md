# Zoya

A strongly-typed functional programming language that compiles to JavaScript.

Zoya combines Rust-inspired syntax with Hindley-Milner type inference, giving you the safety of static types without the verbosity of explicit annotations everywhere.

## Quick Example

```zoya
struct Point { x: Int, y: Int }

fn distance(Point { x, y }: Point) -> Float {
    let squared = x * x + y * y;
    squared.to_float().sqrt()
}

fn main() -> Float {
    let origin = Point { x: 3, y: 4 };
    distance(origin)
}
```

## Features

- **Type inference** - Types are inferred automatically; annotations optional
- **Algebraic data types** - Structs (products) and enums (sums) with generics
- **Type aliases** - Named type synonyms with generic support
- **Pattern matching** - Exhaustive matching with destructuring everywhere
- **First-class functions** - Lambdas, closures, and higher-order functions
- **Immutable by default** - All data structures are immutable
- **Compiles to JavaScript** - Run anywhere JS runs

## Get Started

Ready to try Zoya? Head to the [Installation](getting-started/installation.md) guide.
