# Methods

Zoya supports methods on types through `impl` blocks. The standard library provides methods on primitive types (Int, Float, String, etc.), and you can define methods on your own structs and enums.

## User-Defined Methods

Use `impl` blocks to define methods and associated functions on your own types.

### Basic Impl Block

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

### Associated Functions (Constructors)

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

### The `Self` Type

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

### Generic Impl Blocks

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

### Impl on Enums

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

### Method Chaining

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

## Primitive Type Methods

The standard library defines methods on primitive and collection types via `impl` blocks in the `std` package.

## String Methods

```zoya
"hello".len()              // 5
"hello".is_empty()         // false
"hello".contains("ell")    // true
"hello".starts_with("he")  // true
"hello".ends_with("lo")    // true
"hello".to_uppercase()     // "HELLO"
"HELLO".to_lowercase()     // "hello"
"  hi  ".trim()            // "hi"
```

## Int Methods

```zoya
(-5).abs()              // 5
42.to_string()          // "42"
42.to_float()           // 42.0
3.min(5)                // 3
3.max(5)                // 5
```

## BigInt Methods

```zoya
(-5n).abs()             // 5n
42n.to_string()         // "42"
3n.min(5n)              // 3n
3n.max(5n)              // 5n
```

## Float Methods

```zoya
3.14.floor()            // 3.0
3.14.ceil()             // 4.0
3.14.round()            // 3.0
4.0.sqrt()              // 2.0
3.14.abs()              // 3.14
3.14.to_string()        // "3.14"
3.7.to_int()            // 3
3.14.min(2.0)           // 2.0
3.14.max(5.0)           // 5.0
```

## List Methods

Lists support index access with bracket notation, returning `Option<T>`:

```zoya
[10, 20, 30][0]         // Some(10)
[10, 20, 30][-1]        // Some(30)
[10, 20, 30][5]         // None
```

All list operations return new lists (immutable):

```zoya
[1, 2].len()            // 2
[1, 2].is_empty()       // false
[1, 2].push(3)          // [1, 2, 3]
[1, 2, 3].reverse()     // [3, 2, 1]
```
