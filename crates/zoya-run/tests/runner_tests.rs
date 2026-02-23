use zoya_check::check;
use zoya_loader::{MemorySource, load_memory_package};
use zoya_package::QualifiedPath;
use zoya_run::{EvalError, Runner, Value, run_source};
use zoya_std::std as zoya_std;

#[test]
fn test_run_simple_main() {
    let source = "pub fn main() -> Int { 42 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_main_with_expression() {
    let source = "pub fn main() -> Int { 1 + 2 * 3 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_run_main_with_float() {
    let source = "pub fn main() -> Float { 3.15 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Float(3.15));
}

#[test]
fn test_run_no_main_error() {
    let source = "fn foo() -> Int { 42 }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("no pub fn main()"))
    );
}

#[test]
fn test_run_private_main_error() {
    let source = "fn main() -> Int { 42 }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("no pub fn main()"))
    );
}

#[test]
fn test_run_main_with_params_error() {
    let source = "pub fn main(x: Int) -> Int { x }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("expects 1 argument(s), got 0"))
    );
}

#[test]
fn test_run_division_by_zero() {
    let source = "pub fn main() -> Int { 1 / 0 }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::Panic(ref msg)) if msg == "division by zero"),
        "expected Panic(\"division by zero\"), got: {:?}",
        result
    );
}

// List tests

#[test]
fn test_run_list_exhaustiveness_error() {
    let source = r#"
        fn bad(xs: List<Int>) -> Int {
            match xs {
                [x, ..] => x,
            }
        }
        pub fn main() -> Int { bad([1]) }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::TypeError(e)) if e.to_string().contains("non-exhaustive")
    ));
}

// Lambda tests

#[test]
fn test_run_function_type_mismatch_error() {
    let source = r#"
        pub fn main() -> Int {
            let f: Int -> Int = |x| true;
            0
        }
    "#;
    let result = run_source(source);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("type mismatch") || err.contains("Bool") || err.contains("Int"),
        "error should mention type mismatch: {}",
        err
    );
}

// Tuple struct tests

#[test]
fn test_run_tuple_struct_display() {
    let source = r#"
        struct Wrapper(Int)
        pub fn main() -> Wrapper {
            Wrapper(42)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Wrapper(42)");
}

// ============================================================================

/// Helper function to run multi-module code expecting a type check error containing substring.
fn expect_check_error(modules: Vec<(&str, &str)>, expected_substring: &str) {
    let mut source = MemorySource::new();
    for (path, content) in modules {
        source.add_module(path, content);
    }
    let package = load_memory_package(&source, zoya_loader::Mode::Dev);
    match package {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains(expected_substring),
                "expected error containing '{}', got: {}",
                expected_substring,
                msg
            );
        }
        Ok(pkg) => {
            let result = check(&pkg, &[]);
            assert!(
                result.is_err(),
                "expected error containing '{}', but check succeeded",
                expected_substring
            );
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains(expected_substring),
                "expected error containing '{}', got: {}",
                expected_substring,
                msg
            );
        }
    }
}

// ===== Use Path Prefixes =====

#[test]
fn test_use_super_from_root_fails() {
    expect_check_error(
        vec![(
            "root",
            r#"
            use super::something
            pub fn main() -> Int { 0 }
        "#,
        )],
        "super::",
    );
}

// ===== Visibility Tests =====

#[test]
fn test_visibility_private_fn_not_accessible_from_sibling() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod a
            mod b
            pub fn main() -> Int { b::try_access() }
        "#,
            ),
            (
                "a",
                r#"
            fn secret() -> Int { 42 }
        "#,
            ),
            (
                "b",
                r#"
            use root::a::secret
            pub fn try_access() -> Int { secret() }
        "#,
            ),
        ],
        "private",
    );
}

#[test]
fn test_visibility_private_fn_not_accessible_from_parent() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod child
            pub fn main() -> Int { child::secret() }
        "#,
            ),
            (
                "child",
                r#"
            fn secret() -> Int { 42 }
        "#,
            ),
        ],
        "private",
    );
}

// ===== Error Cases =====

#[test]
fn test_error_import_nonexistent_item() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::nonexistent
            pub fn main() -> Int { 0 }
        "#,
            ),
            ("utils", "pub fn helper() -> Int { 42 }"),
        ],
        "cannot find",
    );
}

#[test]
fn test_error_import_private_from_sibling() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod a
            mod b
            use root::b::secret
            pub fn main() -> Int { secret() }
        "#,
            ),
            ("a", "pub fn helper() -> Int { 1 }"),
            ("b", "fn secret() -> Int { 42 }"),
        ],
        "private",
    );
}

#[test]
fn test_error_duplicate_import_names() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod a
            mod b
            use root::a::foo
            use root::b::foo
            pub fn main() -> Int { foo() }
        "#,
            ),
            ("a", "pub fn foo() -> Int { 1 }"),
            ("b", "pub fn foo() -> Int { 2 }"),
        ],
        "already imported",
    );
}

#[test]
fn test_error_use_without_prefix() {
    let mut source = MemorySource::new();
    source.add_module(
        "root",
        r#"
        mod utils
        use utils::helper
        pub fn main() -> Int { helper() }
    "#,
    );
    source.add_module("utils", "pub fn helper() -> Int { 42 }");
    let result = load_memory_package(&source, zoya_loader::Mode::Dev);
    assert!(
        result.is_err() || {
            let pkg = result.unwrap();
            check(&pkg, &[]).is_err()
        }
    );
}

#[test]
fn test_error_module_not_found() {
    let mut source = MemorySource::new();
    source.add_module(
        "root",
        r#"
        mod missing_module
        pub fn main() -> Int { 0 }
    "#,
    );
    let result = load_memory_package(&source, zoya_loader::Mode::Dev);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("missing"),
        "expected 'not found' error, got: {}",
        err
    );
}

#[test]
fn test_error_call_private_via_qualified_path() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod utils
            pub fn main() -> Int { utils::secret() }
        "#,
            ),
            ("utils", "fn secret() -> Int { 42 }"),
        ],
        "private",
    );
}

// ===== Module Visibility (pub mod) Tests =====

#[test]
fn test_visibility_private_mod_not_accessible_from_non_descendant() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub mod b
            pub fn main() -> Int { 0 }
        "#,
            ),
            ("a", "mod internal"),
            ("a/internal", "pub fn helper() -> Int { 42 }"),
            (
                "b",
                r#"
            use root::a::internal::helper
            pub fn test() -> Int { helper() }
        "#,
            ),
        ],
        "private",
    );
}

#[test]
fn test_visibility_private_mod_blocks_nested_pub_item() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub mod b
            pub fn main() -> Int { 0 }
        "#,
            ),
            ("a", "mod internal"),
            ("a/internal", "pub fn secret() -> Int { 99 }"),
            (
                "b",
                r#"
            use root::a::internal::secret
            pub fn test() -> Int { secret() }
        "#,
            ),
        ],
        "private",
    );
}

#[test]
fn test_visibility_all_modules_in_path_must_be_visible() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub mod outside
            pub fn main() -> Int { 0 }
        "#,
            ),
            ("a", "mod b"),
            ("a/b", "pub fn f() -> Int { 42 }"),
            (
                "outside",
                r#"
            use root::a::b::f
            pub fn test() -> Int { f() }
        "#,
            ),
        ],
        "private",
    );
}

#[test]
fn test_visibility_struct_through_private_mod_error() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub mod b
            pub fn main() -> Int { 0 }
        "#,
            ),
            (
                "a",
                r#"
            mod types
        "#,
            ),
            (
                "a/types",
                r#"
            pub struct Point { x: Int, y: Int }
        "#,
            ),
            (
                "b",
                r#"
            use root::a::types::Point
            pub fn test() -> Int { 0 }
        "#,
            ),
        ],
        "private",
    );
}

#[test]
fn test_visibility_private_mod_qualified_path_error() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub mod b
            pub fn main() -> Int { 0 }
        "#,
            ),
            ("a", "mod internal"),
            ("a/internal", "pub fn helper() -> Int { 42 }"),
            (
                "b",
                r#"
            pub fn test() -> Int { root::a::internal::helper() }
        "#,
            ),
        ],
        "private",
    );
}

// ===== pub use Re-export Tests =====

#[test]
fn test_pub_use_visibility_error_e2e() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
            mod a
            mod b
            pub fn main() -> Int { 0 }
        "#,
            ),
            ("a", "fn secret() -> Int { 42 }"),
            ("b", "pub use root::a::secret"),
        ],
        "pub use cannot re-export private",
    );
}

// ============================================================================

#[test]
fn test_glob_import_skips_private() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math::*
                pub fn main() -> Int { secret() }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                fn secret() -> Int { 42 }
            "#,
            ),
        ],
        "unknown identifier: secret",
    );
}

// ============================================================================

#[test]
fn test_group_import_not_found() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math::{add, nonexistent}
                pub fn main() -> Int { add(1, 2) }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
            ),
        ],
        "cannot find",
    );
}

#[test]
fn test_group_import_private_error() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math::{add, secret}
                pub fn main() -> Int { add(1, 2) }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                fn secret() -> Int { 42 }
            "#,
            ),
        ],
        "private",
    );
}

// ============================================================================

#[test]
fn test_duplicate_group_import_error() {
    expect_check_error(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math::{add, add}
                pub fn main() -> Int { add(1, 2) }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
            ),
        ],
        "already imported",
    );
}

// ── std library integration tests ──────────────────────────────────

#[test]
fn test_std_option_use_type_and_variants() {
    let source = r#"
        use std::option::Option

        pub fn main() -> Option<Int> { Option::Some(42) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(42)");
}

#[test]
fn test_std_option_use_none() {
    let source = r#"
        use std::option::Option

        pub fn main() -> Option<Int> { Option::None::<Int> }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::None");
}

#[test]
fn test_std_option_use_group_some() {
    let source = r#"
        use std::option::{Option, Some}

        pub fn main() -> Option<Int> { Some(42) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(42)");
}

#[test]
fn test_std_option_use_group_none() {
    let source = r#"
        use std::option::{Option, None}

        pub fn main() -> Option<Int> { None::<Int> }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::None");
}

#[test]
fn test_std_option_use_variant_directly() {
    let source = r#"
        use std::option::Some
        use std::option::None
        use std::option::Option

        pub fn main() -> Option<Int> { Some(42) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(42)");
}

#[test]
fn test_std_option_nested() {
    let source = r#"
        use std::option::{Option, Some}

        pub fn main() -> Option<Option<Int>> {
            Some(Some(42))
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(Option::Some(42))");
}

#[test]
fn test_std_option_use_group() {
    let source = r#"
        use std::option::{Option, Some, None}

        pub fn main() -> Option<Int> { Some(99) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(99)");
}

// ── std library direct package path tests (no use imports) ─────────

#[test]
fn test_std_option_direct_some_expr() {
    let source = r#"
        pub fn main() -> std::option::Option<Int> { std::option::Some(42) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(42)");
}

#[test]
fn test_std_option_direct_none_expr() {
    let source = r#"
        pub fn main() -> std::option::Option<Int> { std::option::Option::None::<Int> }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::None");
}

// ===== Prelude (auto-imported) =====

#[test]
fn test_prelude_option_some_without_import() {
    let source = r#"
        pub fn main() -> Option<Int> { Some(42) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(42)");
}

#[test]
fn test_prelude_option_none_without_import() {
    let source = r#"
        pub fn main() -> Option<Int> { None::<Int> }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::None");
}

#[test]
fn test_prelude_result_ok_without_import() {
    let source = r#"
        pub fn main() -> Result<Int, String> { Ok(42) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Result::Ok(42)");
}

#[test]
fn test_prelude_result_err_without_import() {
    let source = r#"
        pub fn main() -> Result<Int, String> { Err("oops") }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), r#"Result::Err("oops")"#);
}

#[test]
fn test_prelude_explicit_import_takes_priority() {
    let source = r#"
        use std::option::{Option, Some, None}

        pub fn main() -> Option<Int> { Some(99) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(99)");
}

// ==================== Recursive Type Tests ====================

#[test]
fn test_recursive_enum_non_exhaustive_inner() {
    expect_check_error(
        vec![(
            "root",
            r#"
                    pub enum Tree { Leaf(Int), Branch(List<Tree>) }

                    fn check_tree(t: Tree) -> Int {
                        match t {
                            Tree::Leaf(n) => n,
                        }
                    }

                    pub fn main() -> Int { check_tree(Tree::Leaf(1)) }
                "#,
        )],
        "non-exhaustive",
    );
}

#[test]
fn test_recursive_enum_non_exhaustive_inner_multi_variant() {
    expect_check_error(
        vec![(
            "root",
            r#"
                    pub enum Value {
                        Null,
                        Num(Int),
                        Arr(List<Value>),
                    }

                    fn partial_check(v: Value) -> Int {
                        match v {
                            Value::Null => 0,
                            Value::Num(_) => 1,
                        }
                    }

                    pub fn main() -> Int { partial_check(Value::Null) }
                "#,
        )],
        "non-exhaustive",
    );
}

// Issue 1: Arithmetic on non-numeric types rejected
#[test]
fn test_arithmetic_on_string_error() {
    let source = r#"
        pub fn main() -> String {
            "hello" + "world"
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("arithmetic operators only work on numeric types"))
    );
}

#[test]
fn test_ordering_on_string_error() {
    let source = r#"
        pub fn main() -> Bool {
            "hello" < "world"
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("ordering operators only work on numeric types"))
    );
}

#[test]
fn test_ordering_on_bool_error() {
    let source = r#"
        pub fn main() -> Bool {
            true < false
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("ordering operators only work on numeric types"))
    );
}

// Issue 2: Turbofish on named-field struct construction
#[test]
fn test_turbofish_named_struct_wrong_type_error() {
    let source = r#"
        pub struct Pair<A, B> { first: A, second: B }
        pub fn main() -> Int {
            let p = Pair::<Int, Int> { first: "hello", second: 2 };
            p.second
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("type mismatch"))
    );
}

#[test]
fn test_turbofish_named_struct_wrong_arity_error() {
    let source = r#"
        pub struct Pair<A, B> { first: A, second: B }
        pub fn main() -> Int {
            let p = Pair::<Int> { first: 1, second: 2 };
            p.first
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("type argument"))
    );
}

// Issue 2: Turbofish on named-field enum struct variant construction
#[test]
fn test_turbofish_enum_struct_variant_wrong_type_error() {
    let source = r#"
        pub enum Container<T> {
            Named { value: T },
        }
        pub fn main() -> Int {
            let c = Container::Named::<Int> { value: "hello" };
            match c {
                Container::Named { value } => value,
            }
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("type mismatch"))
    );
}

// Issue 5: Duplicate pattern binding detection
#[test]
fn test_duplicate_binding_tuple_error() {
    let source = r#"
        pub fn main() -> Int {
            let t = (1, 2);
            match t {
                (x, x) => x,
            }
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("duplicate binding"))
    );
}

#[test]
fn test_duplicate_binding_list_error() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [1, 2];
            match xs {
                [x, x] => x,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::TypeError(ref e)) if e.to_string().contains("duplicate binding"))
    );
}

// ===== Panic Tests =====

#[test]
fn test_run_panic_produces_error() {
    let source = r#"
        pub fn main() -> Int {
            panic("something went wrong")
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "something went wrong"
    ));
}

#[test]
fn test_run_panic_with_string_return_type() {
    let source = r#"
        pub fn main() -> String {
            panic("not implemented")
        }
    "#;
    let result = run_source(source);
    assert!(matches!(result, Err(EvalError::Panic(_))));
}

#[test]
fn test_run_panic_with_bool_return_type() {
    let source = r#"
        pub fn main() -> Bool {
            panic("unreachable")
        }
    "#;
    let result = run_source(source);
    assert!(matches!(result, Err(EvalError::Panic(_))));
}

#[test]
fn test_run_panic_in_let_wildcard() {
    let source = r#"
        pub fn main() -> Int {
            let _ = panic("oops");
            42
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "oops"
    ));
}

#[test]
fn test_run_panic_in_match_arm() {
    let source = r#"
        pub fn main() -> Int {
            let x = 5;
            match x {
                0 => 0,
                _ => panic("unexpected value"),
            }
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "unexpected value"
    ));
}

#[test]
fn test_run_panic_in_called_function() {
    let source = r#"
        fn todo(message: String) -> Int {
            panic(message)
        }
        pub fn main() -> Int {
            todo("not yet")
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "not yet"
    ));
}

// ===== Assert Tests =====

#[test]
fn test_run_assert_false_panics() {
    let source = r#"
        pub fn main() -> () {
            assert(false)
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "assertion failed"
    ));
}

#[test]
fn test_run_assert_eq_different_values_panics() {
    let source = r#"
        pub fn main() -> () {
            assert_eq(1, 2)
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "assertion failed: left != right"
    ));
}

#[test]
fn test_run_assert_ne_same_values_panics() {
    let source = r#"
        pub fn main() -> () {
            assert_ne(1, 1)
        }
    "#;
    let result = run_source(source);
    assert!(matches!(
        result,
        Err(EvalError::Panic(msg)) if msg == "assertion failed: left == right"
    ));
}

#[test]
fn test_run_println_returns_unit() {
    let source = r#"
        pub fn main() -> () { println("hello") }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Tuple(vec![]));
}

#[test]
fn test_run_println_in_let_binding() {
    let source = r#"
        pub fn main() -> Int {
            let _ = println("hello");
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_println_with_string_variable() {
    let source = r#"
        pub fn main() -> () {
            let msg = "Hello, World!";
            println(msg)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Tuple(vec![]));
}

// ===== Mode tests =====

#[test]
fn test_run_dev_mode_strips_test_fn() {
    let source = r#"
        #[test]
        fn test_helper() -> Int { 99 }
        pub fn main() -> Int { 42 }
    "#;
    let result = Runner::new()
        .source(source)
        .mode(zoya_loader::Mode::Dev)
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_test_mode_retains_test_fn() {
    let source = r#"
        #[test]
        fn test_helper() { () }
        pub fn main() -> () { test_helper() }
    "#;
    let result = Runner::new()
        .source(source)
        .mode(zoya_loader::Mode::Test)
        .run()
        .unwrap();
    assert_eq!(result, Value::Tuple(vec![]));
}

// ============================================================================

#[test]
fn test_entry_runs_specific_function() {
    let source = r#"
        pub fn main() -> Int { 0 }
        pub fn answer() -> Int { 42 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .entry(QualifiedPath::root().child("answer"), vec![])
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_entry_runs_function_in_submodule() {
    let mut source = MemorySource::new();
    source.add_module("root", "pub mod utils");
    source.add_module("utils", "pub fn compute() -> Int { 99 }");
    let package = load_memory_package(&source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .entry(
            QualifiedPath::root().child("utils").child("compute"),
            vec![],
        )
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(99));
}

#[test]
fn test_entry_error_on_nonexistent_function() {
    let source = r#"
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let path = QualifiedPath::root().child("nonexistent");
    let result = Runner::new()
        .package(&checked, [])
        .entry(path, vec![])
        .run();
    assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("not found")));
}

#[test]
fn test_main_module_runs_main_in_submodule() {
    let mut source = MemorySource::new();
    source.add_module("root", "pub mod sub");
    source.add_module("sub", "pub fn main() -> Int { 77 }");
    let package = load_memory_package(&source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .main_module("sub")
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(77));
}

#[test]
fn test_entry_error_on_arg_count_mismatch() {
    let source = r#"
        pub fn add(x: Int, y: Int) -> Int { x + y }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .entry(QualifiedPath::root().child("add"), vec![Value::Int(1)])
        .run();
    assert!(
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("expects 2 argument(s), got 1"))
    );
    let result = Runner::new()
        .package(&checked, [])
        .entry(QualifiedPath::root().child("add"), vec![])
        .run();
    assert!(
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("expects 2 argument(s), got 0"))
    );
}

#[test]
fn test_entry_with_args() {
    let source = r#"
        pub fn add(x: Int, y: Int) -> Int { x + y }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .entry(
            QualifiedPath::root().child("add"),
            vec![Value::Int(3), Value::Int(4)],
        )
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_entry_with_string_arg() {
    let source = r#"
        pub fn len(s: String) -> Int { s.len() }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let std = zoya_std();
    let checked = check(&package, &[std]).unwrap();
    let result = Runner::new()
        .package(&checked, [std])
        .entry(
            QualifiedPath::root().child("len"),
            vec![Value::String("hello".into())],
        )
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(5));
}

#[test]
fn test_entry_arg_type_mismatch() {
    let source = r#"
        pub fn double(x: Int) -> Int { x * 2 }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .entry(
            QualifiedPath::root().child("double"),
            vec![Value::String("not an int".into())],
        )
        .run();
    assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("type mismatch")));
}

#[test]
fn test_entry_with_struct_arg() {
    let source = r#"
        pub struct Point { x: Int, y: Int }
        pub fn sum_point(p: Point) -> Int { p.x + p.y }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let mut fields = std::collections::HashMap::new();
    fields.insert("x".into(), Value::Int(10));
    fields.insert("y".into(), Value::Int(20));
    let point = Value::Struct {
        name: "Point".into(),
        module: QualifiedPath::root(),
        data: zoya_run::ValueData::Struct(fields),
    };
    let result = Runner::new()
        .package(&checked, [])
        .entry(QualifiedPath::root().child("sum_point"), vec![point])
        .run()
        .unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_entry_with_enum_arg() {
    let source = r#"
        pub enum Shape {
            Circle(Float),
            Square(Float),
        }
        pub fn area(s: Shape) -> Float {
            match s {
                Shape::Circle(r) => 3.14 * r * r,
                Shape::Square(side) => side * side,
            }
        }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let std = zoya_std();
    let checked = check(&package, &[std]).unwrap();
    let circle = Value::EnumVariant {
        enum_name: "Shape".into(),
        variant_name: "Circle".into(),
        module: QualifiedPath::root(),
        data: zoya_run::ValueData::Tuple(vec![Value::Float(10.0)]),
    };
    let result = Runner::new()
        .package(&checked, [std])
        .entry(QualifiedPath::root().child("area"), vec![circle])
        .run()
        .unwrap();
    assert_eq!(result, Value::Float(314.0));
}

// ===== Modulo operator tests =====

#[test]
fn test_modulo_by_zero() {
    let source = "pub fn main() -> Int { 10 % 0 }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::Panic(ref msg)) if msg == "modulo by zero"),
        "expected Panic(\"modulo by zero\"), got: {:?}",
        result
    );
}

#[test]
fn test_modulo_bigint_by_zero() {
    let source = "pub fn main() -> BigInt { 10n % 0n }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::Panic(ref msg)) if msg == "modulo by zero"),
        "expected Panic(\"modulo by zero\"), got: {:?}",
        result
    );
}

// ===== Power operator tests =====

#[test]
fn test_power_negative_exponent() {
    let source = "pub fn main() -> Int { 2 ** -1 }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::Panic(ref msg)) if msg == "negative exponent"),
        "expected Panic(\"negative exponent\"), got: {:?}",
        result
    );
}

#[test]
fn test_power_bigint_negative_exponent() {
    let source = "pub fn main() -> BigInt { 2n ** -1n }";
    let result = run_source(source);
    assert!(
        matches!(result, Err(EvalError::Panic(ref msg)) if msg == "negative exponent"),
        "expected Panic(\"negative exponent\"), got: {:?}",
        result
    );
}

// ==================== Dict Tests ====================

#[test]
fn test_dict_repl_display() {
    let source = r#"
        pub fn main() -> Dict<String, Int> {
            Dict::new().insert("a", 1)
        }
    "#;
    let result = run_source(source).unwrap();
    match &result {
        Value::Dict(entries) => {
            assert_eq!(entries.len(), 1);
            assert_eq!(
                entries.get(&Value::String("a".to_string())),
                Some(&Value::Int(1))
            );
        }
        other => panic!("expected Value::Dict, got {:?}", other),
    }
    assert_eq!(result.to_string(), "{\"a\": 1}");
}

// Interpolated string tests

#[test]
fn test_set_repl_display() {
    let source = r#"
        pub fn main() -> Set<Int> {
            Set::from([1, 2, 3])
        }
    "#;
    let result = run_source(source).unwrap();
    match &result {
        Value::Set(elements) => {
            assert_eq!(elements.len(), 3);
            let mut sorted: Vec<_> = elements
                .iter()
                .map(|v| match v {
                    Value::Int(n) => *n,
                    other => panic!("expected Value::Int, got {:?}", other),
                })
                .collect();
            sorted.sort();
            assert_eq!(sorted, vec![1, 2, 3]);
        }
        other => panic!("expected Value::Set, got {:?}", other),
    }
}

// ── Task attribute tests ─────────────────────────────────────────────

#[test]
fn test_task_fn_compiles_and_tasks_method_works() {
    let source = r#"
        #[task]
        pub fn my_task() -> Int { 42 }

        pub fn main() -> Int { my_task() }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let std = zoya_std();
    let checked = check(&package, &[std]).unwrap();

    let tasks = checked.tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0], QualifiedPath::root().child("my_task"));

    let result = Runner::new().package(&checked, [std]).run().unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_task_fn_with_params_compiles() {
    let source = r#"
        #[task]
        pub fn my_task(x: Int) -> Int { x + 1 }

        pub fn main() -> Int { my_task(41) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_private_task_fn_is_discoverable() {
    let source = r#"
        #[task]
        fn my_task() -> Int { 42 }

        pub fn main() { () }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let std = zoya_std();
    let checked = check(&package, &[std]).unwrap();

    let tasks = checked.tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0], QualifiedPath::root().child("my_task"));
}

#[test]
fn test_fns_returns_pub_non_test_non_task_functions() {
    let source = r#"
        pub fn hello() -> Int { 1 }
        pub fn world() -> Int { 2 }

        fn private_fn() -> Int { 3 }

        #[test]
        fn my_test() { () }

        #[task]
        pub fn my_task() -> Int { 4 }

        pub fn main() { () }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let std = zoya_std();
    let checked = check(&package, &[std]).unwrap();

    let fns = checked.fns();
    let fn_names: Vec<String> = fns.iter().map(|p| p.to_string()).collect();

    assert!(fn_names.contains(&"root::hello".to_string()));
    assert!(fn_names.contains(&"root::main".to_string()));
    assert!(fn_names.contains(&"root::world".to_string()));

    assert!(!fn_names.contains(&"root::my_test".to_string()));

    assert!(!fn_names.contains(&"root::my_task".to_string()));

    assert!(!fn_names.contains(&"root::private_fn".to_string()));
}
