# Impl Blocks

Zoya supports methods on types through `impl` blocks. You can define methods and associated functions on your own structs and enums.

## Basic Impl Block

An `impl` block groups functions that operate on a specific type. Functions with `self` as their first parameter are methods; those without `self` are associated functions.

```zoya
struct Point { x: Int, y: Int }

impl Point {
    fn sum(self) -> Int {
        self.x + self.y
    }

    fn distance(self, other: Point) -> Int {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }
}

pub fn main() -> Int {
    let p = Point { x: 3, y: 4 };
    p.sum()  // 7
}
```

## Associated Functions (Constructors)

Functions without `self` are called using path syntax (`Type::function()`). They are commonly used as constructors:

```zoya
struct Point { x: Int, y: Int }

impl Point {
    fn origin() -> Self {
        Point { x: 0, y: 0 }
    }

    fn new(x: Int, y: Int) -> Self {
        Point { x: x, y: y }
    }
}

pub fn main() -> Int {
    let p = Point::origin();
    let q = Point::new(3, 4);
    q.x  // 3
}
```

## The `Self` Type

Inside an `impl` block, `Self` refers to the type being implemented. Use it in return types and parameter types to avoid repeating the type name:

```zoya
struct Point { x: Int, y: Int }

impl Point {
    fn mirror(self) -> Self {
        Point { x: self.y, y: self.x }
    }
}

pub fn main() -> Int {
    let p = Point { x: 1, y: 2 };
    let m = p.mirror();
    m.x  // 2
}
```

## Generic Impl Blocks

For generic types, declare the type parameters after `impl`:

```zoya
struct Wrapper<T> { value: T }

impl<T> Wrapper<T> {
    fn new(v: T) -> Self {
        Wrapper { value: v }
    }

    fn unwrap(self) -> T {
        self.value
    }
}

pub fn main() -> Int {
    let w = Wrapper::new(42);
    w.unwrap()  // 42
}
```

## Impl on Enums

Enums can also have `impl` blocks. Use `match` inside methods to handle different variants:

```zoya
enum Shape {
    Circle(Int),
    Square(Int),
}

impl Shape {
    fn area(self) -> Int {
        match self {
            Shape::Circle(r) => r * r * 3,
            Shape::Square(s) => s * s,
        }
    }

    fn default() -> Self {
        Shape::Circle(1)
    }
}

pub fn main() -> Int {
    let c = Shape::Circle(5);
    c.area()  // 75
}
```

## Method Chaining

Methods that return `Self` enable chaining:

```zoya
struct Builder { value: Int }

impl Builder {
    fn new() -> Self {
        Builder { value: 0 }
    }

    fn add(self, n: Int) -> Self {
        Builder { value: self.value + n }
    }

    fn build(self) -> Int {
        self.value
    }
}

pub fn main() -> Int {
    Builder::new().add(10).add(20).add(12).build()  // 42
}
```

> The standard library also defines methods on primitive and collection types (Int, Float, String, List, Dict, etc.) via `impl` blocks. See the [Standard Library](standard-library.md) for the full reference.
