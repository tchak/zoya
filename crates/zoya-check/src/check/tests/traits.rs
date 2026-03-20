use zoya_loader::{MemorySource, load_memory_package};

use crate::check::check;

/// Helper: parse source into a package, type-check it, return error message if any.
fn check_source(source: &str) -> Result<(), String> {
    let mem = MemorySource::new().with_module("root", source);
    let pkg = load_memory_package(&mem, zoya_loader::Mode::Dev).map_err(|e| format!("{}", e))?;
    check(&pkg, &[]).map(|_| ()).map_err(|e| e.to_string())
}

// -- Basic trait definition and impl ------------------------------------------

#[test]
fn test_trait_basic_impl() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }

        trait Describe {
            fn describe(self) -> String
        }

        impl Describe for Point {
            fn describe(self) -> String { "point" }
        }

        pub fn main() -> String {
            let p = Point { x: 1, y: 2 };
            p.describe()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_trait_multiple_methods() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }

        trait Shape {
            fn area(self) -> Int
            fn name(self) -> String
        }

        impl Shape for Point {
            fn area(self) -> Int { self.x * self.y }
            fn name(self) -> String { "point" }
        }

        pub fn main() -> String {
            let p = Point { x: 3, y: 4 };
            p.name()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Default methods ----------------------------------------------------------

#[test]
fn test_trait_default_method() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }

        trait Describe {
            fn describe(self) -> String
            fn verbose(self) -> String { self.describe() }
        }

        impl Describe for Point {
            fn describe(self) -> String { "point" }
        }

        pub fn main() -> String {
            let p = Point { x: 1, y: 2 };
            p.describe()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Error: missing required method -------------------------------------------

#[test]
fn test_trait_missing_required_method() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }

        trait Shape {
            fn area(self) -> Int
            fn name(self) -> String
        }

        impl Shape for Point {
            fn area(self) -> Int { self.x * self.y }
        }
    "#,
    );
    assert!(result.is_err(), "Expected error for missing method");
    assert!(
        result.unwrap_err().contains("missing trait method"),
        "Should mention missing trait method"
    );
}

// -- Error: impl for non-trait ------------------------------------------------

#[test]
fn test_impl_for_non_trait() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }

        impl Point for Point {
            fn describe(self) -> String { "point" }
        }
    "#,
    );
    assert!(result.is_err(), "Expected error: Point is not a trait");
    assert!(
        result.unwrap_err().contains("not a trait"),
        "Should mention not a trait"
    );
}

// -- Trait impl with inherent methods -----------------------------------------

#[test]
fn test_trait_and_inherent_coexist() {
    let result = check_source(
        r#"
        struct Point { x: Int, y: Int }

        trait Describe {
            fn describe(self) -> String
        }

        impl Point {
            fn sum(self) -> Int { self.x + self.y }
        }

        impl Describe for Point {
            fn describe(self) -> String { "point" }
        }

        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            p.sum()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// -- Trait impl for enum ------------------------------------------------------

#[test]
fn test_trait_impl_for_enum() {
    let result = check_source(
        r#"
        enum Color { Red, Green, Blue }

        trait Describe {
            fn describe(self) -> String
        }

        impl Describe for Color {
            fn describe(self) -> String {
                match self {
                    Color::Red => "red",
                    Color::Green => "green",
                    Color::Blue => "blue",
                }
            }
        }

        pub fn main() -> String {
            let c = Color::Red;
            c.describe()
        }
    "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}
