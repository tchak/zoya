use zoya_loader::{MemorySource, load_memory_package};

use crate::check::check;

/// Helper: parse source into a package, type-check it, return error message if any.
fn check_source(source: &str) -> Result<(), String> {
    let mem = MemorySource::new().with_module("root", source);
    let pkg = load_memory_package(&mem, zoya_loader::Mode::Dev).map_err(|e| format!("{}", e))?;
    check(&pkg, &[]).map(|_| ()).map_err(|e| e.to_string())
}

// -- Basic impl methods -------------------------------------------------------

#[test]
fn test_impl_method_basic() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn sum(self) -> Int {
                self.x + self.y
            }
        }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            p.sum()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_impl_associated_function() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn origin() -> Self {
                Point { x: 0, y: 0 }
            }
        }
        pub fn main() -> Int {
            Point::origin().x
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_impl_self_type_in_return() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn mirror(self) -> Self {
                Point { x: self.y, y: self.x }
            }
        }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            p.mirror().x
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Generic impls ------------------------------------------------------------

#[test]
fn test_impl_generic() {
    let result = check_source(
        r#"
        struct Wrapper<T> { value: T }
        impl<T> Wrapper<T> {
            fn unwrap(self) -> T {
                self.value
            }
        }
        pub fn main() -> Int {
            let w = Wrapper { value: 42 };
            w.unwrap()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_impl_generic_associated_function() {
    let result = check_source(
        r#"
        struct Wrapper<T> { value: T }
        impl<T> Wrapper<T> {
            fn new(v: T) -> Self {
                Wrapper { value: v }
            }
        }
        pub fn main() -> Int {
            Wrapper::new(42).value
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Enum impls ---------------------------------------------------------------

#[test]
fn test_impl_on_enum() {
    let result = check_source(
        r#"
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
        }
        pub fn main() -> Int {
            Shape::Circle(5).area()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_impl_enum_associated_function() {
    let result = check_source(
        r#"
        enum Color { Red, Green, Blue }
        impl Color {
            fn default() -> Self {
                Color::Red
            }
        }
        pub fn main() -> Bool {
            match Color::default() {
                Color::Red => true,
                _ => false,
            }
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Multiple impl blocks -----------------------------------------------------

#[test]
fn test_impl_multiple_blocks() {
    let result = check_source(
        r#"
        struct Foo { x: Int }
        impl Foo {
            fn get(self) -> Int { self.x }
        }
        impl Foo {
            fn new() -> Self { Foo { x: 0 } }
        }
        pub fn main() -> Int {
            Foo::new().get()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Error cases --------------------------------------------------------------

#[test]
fn test_impl_on_undefined_type() {
    let result = check_source(
        r#"
        impl Nonexistent {
            fn foo(self) -> Int { 42 }
        }
        pub fn main() -> Int { 0 }
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_impl_duplicate_method() {
    let result = check_source(
        r#"
        struct Foo { x: Int }
        impl Foo {
            fn get(self) -> Int { self.x }
        }
        impl Foo {
            fn get(self) -> Int { self.x }
        }
        pub fn main() -> Int { 0 }
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_impl_method_wrong_return_type() {
    let result = check_source(
        r#"
        struct Foo { x: Int }
        impl Foo {
            fn get(self) -> String {
                self.x
            }
        }
        pub fn main() -> Int { 0 }
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_impl_call_associated_as_method_error() {
    let result = check_source(
        r#"
        struct Foo { x: Int }
        impl Foo {
            fn new() -> Self { Foo { x: 0 } }
        }
        pub fn main() -> Int {
            let f = Foo { x: 1 };
            f.new().x
        }
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_impl_method_chaining() {
    let result = check_source(
        r#"
        struct Builder { value: Int }
        impl Builder {
            fn new() -> Self { Builder { value: 0 } }
            fn add(self, n: Int) -> Self { Builder { value: self.value + n } }
            fn build(self) -> Int { self.value }
        }
        pub fn main() -> Int {
            Builder::new().add(10).add(20).build()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_impl_method_with_let_block() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn distance_squared(self, other: Point) -> Int {
                let dx = self.x - other.x;
                let dy = self.y - other.y;
                dx * dx + dy * dy
            }
        }
        pub fn main() -> Int {
            let a = Point { x: 1, y: 2 };
            let b = Point { x: 4, y: 6 };
            a.distance_squared(b)
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_impl_self_not_outside_impl() {
    let result = check_source(
        r#"
        fn foo() -> Self { 42 }
        pub fn main() -> Int { 0 }
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_impl_on_primitive_error() {
    let result = check_source(
        r#"
        impl Int {
            fn double(self) -> Int { self + self }
        }
        pub fn main() -> Int { 0 }
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_impl_visibility() {
    let result = check_source(
        r#"
        struct Foo { x: Int }
        impl Foo {
            pub fn get(self) -> Int { self.x }
            fn secret(self) -> Int { self.x }
        }
        pub fn main() -> Int {
            let f = Foo { x: 1 };
            f.get()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Concrete generic impl blocks -----------------------------------------------

#[test]
fn test_concrete_impl_basic() {
    let result = check_source(
        r#"
        struct Wrapper<T> { value: T }
        impl Wrapper<Int> {
            fn double(self) -> Int {
                self.value * 2
            }
        }
        pub fn main() -> Int {
            let w = Wrapper { value: 5 };
            w.double()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_concrete_impl_type_mismatch() {
    let result = check_source(
        r#"
        struct Wrapper<T> { value: T }
        impl Wrapper<Int> {
            fn double(self) -> Int {
                self.value * 2
            }
        }
        pub fn main() -> Int {
            let w = Wrapper { value: "hello" };
            w.double()
        }
    "#,
    );
    assert!(
        result.is_err(),
        "Expected error for String wrapper calling Int method"
    );
}

#[test]
fn test_concrete_impl_with_generic_fallback() {
    let result = check_source(
        r#"
        struct Box<T> { value: T }
        impl<T> Box<T> {
            fn get(self) -> T {
                self.value
            }
        }
        impl Box<Int> {
            fn double(self) -> Int {
                self.value * 2
            }
        }
        pub fn main() -> Int {
            let b = Box { value: 10 };
            b.get() + b.double()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_concrete_impl_overlap_concrete_wins() {
    let result = check_source(
        r#"
        struct Box<T> { value: T }
        impl<T> Box<T> {
            fn describe(self) -> String {
                "generic"
            }
        }
        impl Box<Int> {
            fn describe(self) -> String {
                "int"
            }
        }
        pub fn main() -> String {
            let b = Box { value: 42 };
            b.describe()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_concrete_impl_generic_used_for_non_matching_type() {
    let result = check_source(
        r#"
        struct Box<T> { value: T }
        impl<T> Box<T> {
            fn get(self) -> T {
                self.value
            }
        }
        impl Box<Int> {
            fn double(self) -> Int {
                self.value * 2
            }
        }
        pub fn main() -> String {
            let b = Box { value: "hello" };
            b.get()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_concrete_impl_duplicate_error() {
    let result = check_source(
        r#"
        struct Wrapper<T> { value: T }
        impl Wrapper<Int> {
            fn foo(self) -> Int { self.value }
        }
        impl Wrapper<Int> {
            fn foo(self) -> Int { self.value * 2 }
        }
    "#,
    );
    assert!(result.is_err(), "Expected duplicate error");
    assert!(
        result.unwrap_err().contains("duplicate"),
        "Should mention duplicate"
    );
}

#[test]
fn test_partial_specialization_rejected() {
    let result = check_source(
        r#"
        struct Pair<A, B> { first: A, second: B }
        impl<A> Pair<A, Int> {
            fn get_second(self) -> Int { self.second }
        }
    "#,
    );
    assert!(result.is_err(), "Expected partial specialization error");
    assert!(
        result.unwrap_err().contains("partial specialization"),
        "Should mention partial specialization"
    );
}

#[test]
fn test_concrete_impl_associated_function() {
    let result = check_source(
        r#"
        struct Wrapper<T> { value: T }
        impl Wrapper<Int> {
            fn zero() -> Self {
                Wrapper { value: 0 }
            }
        }
        pub fn main() -> Int {
            let w = Wrapper::zero();
            w.value
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}
