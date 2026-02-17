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
fn test_run_main_calling_function() {
    let source = r#"
        fn add(x: Int, y: Int) -> Int { x + y }
        pub fn main() -> Int { add(10, 20) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
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
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("must not take any parameters"))
    );
}

#[test]
fn test_run_multiple_functions() {
    let source = r#"
        fn square(x: Int) -> Int { x * x }
        fn double(x: Int) -> Int { x + x }
        pub fn main() -> Int { square(double(3)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(36)); // double(3) = 6, square(6) = 36
}

#[test]
fn test_run_function_no_braces() {
    // Functions with simple expression bodies can omit braces
    let source = r#"
        fn square(x: Int) -> Int x * x
        pub fn main() -> Int { square(5) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(25));
}

#[test]
fn test_run_function_no_braces_multiple() {
    // Multiple functions without braces
    let source = r#"
        fn add(x: Int, y: Int) -> Int x + y
        fn double(x: Int) -> Int x * 2
        pub fn main() -> Int add(double(3), 4)
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10)); // double(3) = 6, add(6, 4) = 10
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

#[test]
fn test_run_bool_true() {
    let source = "pub fn main() -> Bool { true }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_bool_false() {
    let source = "pub fn main() -> Bool { false }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_equality_true() {
    let source = "pub fn main() -> Bool { 1 == 1 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_equality_false() {
    let source = "pub fn main() -> Bool { 1 == 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_inequality() {
    let source = "pub fn main() -> Bool { 1 != 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_less_than() {
    let source = "pub fn main() -> Bool { 1 < 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_greater_than() {
    let source = "pub fn main() -> Bool { 2 > 1 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_less_equal() {
    let source = "pub fn main() -> Bool { 2 <= 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_greater_equal() {
    let source = "pub fn main() -> Bool { 2 >= 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_bool_equality() {
    let source = "pub fn main() -> Bool { true == false }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_float_comparison() {
    let source = "pub fn main() -> Bool { 1.5 < 2.5 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_comparison_with_arithmetic() {
    let source = "pub fn main() -> Bool { 1 + 2 == 3 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_chained_method_calls() {
    let source = r#"pub fn main() -> Int { "hello".to_uppercase().len() }"#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5));
}

#[test]
fn test_run_method_call_in_function() {
    let source = r#"
        fn get_length(s: String) -> Int { s.len() }
        pub fn main() -> Int { get_length("hello") }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5));
}

// List tests
#[test]
fn test_run_list_literal() {
    let source = "pub fn main() -> List<Int> { [1, 2, 3] }";
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
}

#[test]
fn test_run_empty_list() {
    let source = "pub fn main() -> List<Int> { [] }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::List(vec![]));
}

#[test]
fn test_run_list_equality_true() {
    let source = "pub fn main() -> Bool { [1, 2, 3] == [1, 2, 3] }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_list_equality_false_different_elements() {
    let source = "pub fn main() -> Bool { [1, 2, 3] == [1, 2, 4] }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_list_equality_false_different_length() {
    let source = "pub fn main() -> Bool { [1, 2] == [1, 2, 3] }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_list_inequality() {
    let source = "pub fn main() -> Bool { [1, 2] != [1, 3] }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_empty_list_equality() {
    let source = "pub fn main() -> Bool { [] == [] }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_tuple_equality_true() {
    let source = "pub fn main() -> Bool { (1, 2) == (1, 2) }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_tuple_equality_false() {
    let source = "pub fn main() -> Bool { (1, 2) == (1, 3) }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_tuple_inequality() {
    let source = "pub fn main() -> Bool { (1, 2) != (1, 3) }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_list_match_empty() {
    let source = r#"
        fn is_empty<T>(xs: List<T>) -> Bool {
            match xs {
                [] => true,
                [_, ..] => false,
            }
        }
        pub fn main() -> Bool { is_empty([]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_list_match_nonempty() {
    let source = r#"
        fn is_empty<T>(xs: List<T>) -> Bool {
            match xs {
                [] => true,
                [_, ..] => false,
            }
        }
        pub fn main() -> Bool { is_empty([1, 2, 3]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_list_match_head() {
    let source = r#"
        fn head_or_zero(xs: List<Int>) -> Int {
            match xs {
                [] => 0,
                [x, ..] => x,
            }
        }
        pub fn main() -> Int { head_or_zero([42, 1, 2]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_list_match_head_empty() {
    let source = r#"
        fn head_or_zero(xs: List<Int>) -> Int {
            match xs {
                [] => 0,
                [x, ..] => x,
            }
        }
        pub fn main() -> Int { head_or_zero([]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_run_list_match_exact() {
    let source = r#"
        fn sum_pair(xs: List<Int>) -> Int {
            match xs {
                [a, b] => a + b,
                _ => 0,
            }
        }
        pub fn main() -> Int { sum_pair([10, 20]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_run_list_match_exact_wrong_length() {
    let source = r#"
        fn sum_pair(xs: List<Int>) -> Int {
            match xs {
                [a, b] => a + b,
                _ => 0,
            }
        }
        pub fn main() -> Int { sum_pair([1, 2, 3]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_run_list_match_literal_pattern() {
    let source = r#"
        fn starts_with_one(xs: List<Int>) -> Bool {
            match xs {
                [1, ..] => true,
                [_, ..] => false,
                [] => false,
            }
        }
        pub fn main() -> Bool { starts_with_one([1, 2, 3]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_list_match_literal_pattern_not_matching() {
    let source = r#"
        fn starts_with_one(xs: List<Int>) -> Bool {
            match xs {
                [1, ..] => true,
                [_, ..] => false,
                [] => false,
            }
        }
        pub fn main() -> Bool { starts_with_one([2, 3, 4]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_run_list_exhaustiveness_error() {
    // Missing empty list pattern should cause compile error
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
        Err(EvalError::RuntimeError(msg)) if msg.contains("non-exhaustive")
    ));
}

#[test]
fn test_run_list_string() {
    let source = r#"pub fn main() -> List<String> { ["hello", "world"] }"#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::List(vec![
            Value::String("hello".to_string()),
            Value::String("world".to_string())
        ])
    );
}

#[test]
fn test_run_list_function_param() {
    let source = r#"
        fn len_check(xs: List<Int>) -> Bool {
            match xs {
                [] => true,
                [_] => true,
                [_, _] => true,
                _ => false,
            }
        }
        pub fn main() -> Bool { len_check([1, 2]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

// Suffix pattern tests
#[test]
fn test_run_list_match_suffix_pattern() {
    let source = r#"
        fn last_elem(xs: List<Int>) -> Int {
            match xs {
                [.., x] => x,
                [] => 0,
            }
        }
        pub fn main() -> Int { last_elem([1, 2, 3]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_list_match_suffix_pattern_single_elem() {
    let source = r#"
        fn last_elem(xs: List<Int>) -> Int {
            match xs {
                [.., x] => x,
                [] => 0,
            }
        }
        pub fn main() -> Int { last_elem([42]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_list_match_suffix_two_elements() {
    let source = r#"
        fn last_two(xs: List<Int>) -> Int {
            match xs {
                [.., a, b] => a + b,
                [x] => x,
                [] => 0,
            }
        }
        pub fn main() -> Int { last_two([1, 2, 3, 4]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(7)); // 3 + 4
}

#[test]
fn test_run_list_match_suffix_literal_pattern() {
    let source = r#"
        fn ends_with_zero(xs: List<Int>) -> Bool {
            match xs {
                [.., 0] => true,
                [_, ..] => false,
                [] => false,
            }
        }
        pub fn main() -> Bool { ends_with_zero([1, 2, 0]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

// Prefix+Suffix pattern tests
#[test]
fn test_run_list_match_prefix_suffix_pattern() {
    let source = r#"
        fn first_and_last(xs: List<Int>) -> Int {
            match xs {
                [a, .., b] => a + b,
                [a] => a,
                [] => 0,
            }
        }
        pub fn main() -> Int { first_and_last([1, 2, 3, 4]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5)); // 1 + 4
}

#[test]
fn test_run_list_match_prefix_suffix_min_length() {
    // [a, .., b] requires at least 2 elements
    let source = r#"
        fn first_and_last(xs: List<Int>) -> Int {
            match xs {
                [a, .., b] => a + b,
                [a] => a,
                [] => 0,
            }
        }
        pub fn main() -> Int { first_and_last([10, 20]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30)); // 10 + 20
}

#[test]
fn test_run_list_match_prefix_suffix_literals() {
    let source = r#"
        fn bookended_by_ones(xs: List<Int>) -> Bool {
            match xs {
                [1, .., 1] => true,
                [_, ..] => false,
                [] => false,
            }
        }
        pub fn main() -> Bool { bookended_by_ones([1, 2, 3, 1]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_list_match_prefix_suffix_multiple() {
    let source = r#"
        fn middle_free(xs: List<Int>) -> Int {
            match xs {
                [a, b, .., y, z] => a + b + y + z,
                [a, b, c] => a + b + c,
                [a, b] => a + b,
                [a] => a,
                [] => 0,
            }
        }
        pub fn main() -> Int { middle_free([1, 2, 3, 4, 5, 6]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(14)); // 1 + 2 + 5 + 6
}

// Tuple tests
#[test]
fn test_run_tuple_literal() {
    let source = r#"pub fn main() -> (Int, String) { (42, "hello") }"#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::Tuple(vec![Value::Int(42), Value::String("hello".to_string())])
    );
}

#[test]
fn test_run_empty_tuple() {
    let source = "pub fn main() -> () { () }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Tuple(vec![]));
}

#[test]
fn test_run_single_element_tuple() {
    let source = "pub fn main() -> (Int,) { (42,) }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Tuple(vec![Value::Int(42)]));
}

#[test]
fn test_run_tuple_match_exact() {
    let source = r#"
        fn first(t: (Int, String)) -> Int {
            match t {
                (x, _) => x,
            }
        }
        pub fn main() -> Int { first((10, "hello")) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_run_tuple_match_prefix() {
    let source = r#"
        fn get_first(t: (Int, Int, Int)) -> Int {
            match t {
                (x, ..) => x,
            }
        }
        pub fn main() -> Int { get_first((1, 2, 3)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_run_tuple_match_suffix() {
    let source = r#"
        fn get_last(t: (Int, Int, Int)) -> Int {
            match t {
                (.., z) => z,
            }
        }
        pub fn main() -> Int { get_last((1, 2, 3)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_tuple_match_prefix_suffix() {
    let source = r#"
        fn first_and_last(t: (Int, Int, Int)) -> Int {
            match t {
                (a, .., c) => a + c,
            }
        }
        pub fn main() -> Int { first_and_last((1, 2, 3)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(4)); // 1 + 3
}

#[test]
fn test_run_tuple_heterogeneous() {
    let source = r#"
        fn get_int(t: (Int, String, Bool)) -> Int {
            match t {
                (x, _, _) => x,
            }
        }
        pub fn main() -> Int { get_int((42, "hello", true)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_tuple_with_list() {
    let source = r#"
        pub fn main() -> (Int, List<Int>) { (1, [2, 3]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::Tuple(vec![
            Value::Int(1),
            Value::List(vec![Value::Int(2), Value::Int(3)])
        ])
    );
}

// Match arm block expression tests
#[test]
fn test_run_match_with_commas() {
    let source = r#"
        pub fn main() -> Int {
            match 1 { 0 => 0, 1 => 10, _ => 100 }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_run_match_braced_simple() {
    let source = r#"
        pub fn main() -> Int {
            match 1 { 0 => { 0 }, 1 => { 10 }, _ => { 100 } }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_run_match_braced_block() {
    let source = r#"
        pub fn main() -> Int {
            match 5 {
                n => {
                    let doubled = n * 2;
                    doubled + 1
                }
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(11)); // 5 * 2 + 1
}

#[test]
fn test_run_match_block_multiple_bindings() {
    let source = r#"
        pub fn main() -> Int {
            match 3 {
                n => {
                    let a = n * 2;
                    let b = a + 1;
                    let c = b * 2;
                    c
                }
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(14)); // ((3 * 2) + 1) * 2
}

#[test]
fn test_run_match_block_pattern_binding_visible() {
    let source = r#"
        pub fn main() -> Int {
            match 10 {
                x => {
                    let y = x + 5;
                    x + y
                }
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(25)); // 10 + 15
}

#[test]
fn test_run_match_mixed_arms() {
    let source = r#"
        pub fn main() -> Int {
            match 2 {
                0 => 100,
                1 => { let x = 1; x * 10 },
                n => {
                    let base = n * 10;
                    base + n
                }
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(22)); // 2 * 10 + 2
}

#[test]
fn test_run_match_block_with_list_pattern() {
    let source = r#"
        fn sum_first_two(xs: List<Int>) -> Int {
            match xs {
                [a, b, ..] => {
                    let sum = a + b;
                    sum
                },
                [a] => a,
                [] => 0,
            }
        }
        pub fn main() -> Int { sum_first_two([5, 7, 9]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(12));
}

#[test]
fn test_run_match_block_with_tuple_pattern() {
    let source = r#"
        fn process(t: (Int, Int)) -> Int {
            match t {
                (a, b) => {
                    let sum = a + b;
                    let product = a * b;
                    sum + product
                }
            }
        }
        pub fn main() -> Int { process((3, 4)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(19)); // (3 + 4) + (3 * 4) = 7 + 12
}

// Forward reference and mutual recursion tests
#[test]
fn test_run_forward_reference() {
    // caller is defined before callee but calls it
    let source = r#"
        fn caller() -> Int { callee() }
        fn callee() -> Int { 42 }
        pub fn main() -> Int { caller() }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_mutual_recursion() {
    // is_even and is_odd call each other
    let source = r#"
        fn is_even(n: Int) -> Bool {
            match n {
                0 => true,
                _ => is_odd(n - 1),
            }
        }
        fn is_odd(n: Int) -> Bool {
            match n {
                0 => false,
                _ => is_even(n - 1),
            }
        }
        pub fn main() -> Bool { is_even(4) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_mutual_recursion_odd() {
    let source = r#"
        fn is_even(n: Int) -> Bool {
            match n {
                0 => true,
                _ => is_odd(n - 1),
            }
        }
        fn is_odd(n: Int) -> Bool {
            match n {
                0 => false,
                _ => is_even(n - 1),
            }
        }
        pub fn main() -> Bool { is_odd(3) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

// Lambda tests
#[test]
fn test_run_simple_lambda() {
    let source = r#"
        pub fn main() -> Int {
            let f = |x| x + 1;
            f(41)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_multi_param() {
    let source = r#"
        pub fn main() -> Int {
            let add = |x, y| x + y;
            add(10, 32)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_with_type_annotation() {
    let source = r#"
        pub fn main() -> Int {
            let f = |x: Int| -> Int x * 2;
            f(21)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_block_body() {
    let source = r#"
        pub fn main() -> Int {
            let f = |x| {
                let y = x * 2;
                y + 1
            };
            f(20)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(41));
}

#[test]
fn test_run_lambda_nested() {
    let source = r#"
        pub fn main() -> Int {
            let add = |x| |y| x + y;
            let add10 = add(10);
            add10(32)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_polymorphic_identity() {
    // Test let polymorphism: id can be used at different types
    let source = r#"
        pub fn main() -> Int {
            let id = |x| x;
            let a = id(42);
            let b = id(true);
            a
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_polymorphic_const() {
    // Test polymorphic const function
    let source = r#"
        pub fn main() -> Int {
            let const_ = |x| |y| x;
            let always42 = const_(42);
            always42(true)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_no_params() {
    let source = r#"
        pub fn main() -> Int {
            let f = || 42;
            f()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_captures_outer_var() {
    // Lambda captures variable from outer scope
    let source = r#"
        pub fn main() -> Int {
            let x = 10;
            let f = |y| x + y;
            f(32)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_with_function_type_annotation() {
    let source = r#"
        pub fn main() -> Int {
            let f: Int -> Int = |x| x * 2;
            f(21)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_multi_param_function_type() {
    let source = r#"
        pub fn main() -> Int {
            let add: (Int, Int) -> Int = |x, y| x + y;
            add(10, 32)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_no_param_function_type() {
    let source = r#"
        pub fn main() -> Int {
            let f: () -> Int = || 42;
            f()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_lambda_passed_to_function() {
    let source = r#"
        fn apply(f: Int -> Int, x: Int) -> Int f(x)

        pub fn main() -> Int {
            apply(|x| x * 2, 21)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_higher_order_function_returns_lambda() {
    let source = r#"
        fn make_adder(n: Int) -> Int -> Int |x| x + n

        pub fn main() -> Int {
            let add5 = make_adder(5);
            add5(37)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

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

// Struct tests
#[test]
fn test_run_struct_simple() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 10, y: 20 };
            p.x + p.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_run_struct_field_access() {
    let source = r#"
        struct Person { name: String, age: Int }
        pub fn main() -> String {
            let p = Person { name: "Alice", age: 30 };
            p.name
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("Alice".to_string()));
}

#[test]
fn test_run_struct_empty() {
    let source = r#"
        struct Empty {}
        pub fn main() -> Int {
            let e = Empty {};
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_unit_struct() {
    let source = r#"
        struct Empty
        pub fn main() -> Int {
            let e = Empty;
            match e {
                Empty => 42,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_struct_generic() {
    let source = r#"
        struct Pair<T, U> { first: T, second: U }
        pub fn main() -> Int {
            let p = Pair { first: 1, second: 2 };
            p.first + p.second
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_struct_match_exact() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 10, y: 20 };
            match p {
                Point { x, y } => x + y,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_run_struct_match_partial() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 10, y: 20 };
            match p {
                Point { x, .. } => x,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_run_struct_match_with_binding() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 10, y: 20 };
            match p {
                Point { x: a, y: b } => a * b,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(200));
}

#[test]
fn test_run_struct_nested() {
    let source = r#"
        struct Point { x: Int, y: Int }
        struct Line { start: Point, end: Point }
        pub fn main() -> Int {
            let l = Line {
                start: Point { x: 0, y: 0 },
                end: Point { x: 10, y: 20 }
            };
            l.end.x + l.end.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_run_struct_field_shorthand() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let x = 10;
            let y = 20;
            let p = Point { x, y };
            p.x + p.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_run_struct_equality() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Bool {
            let p1 = Point { x: 10, y: 20 };
            let p2 = Point { x: 10, y: 20 };
            p1 == p2
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_struct_inequality() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Bool {
            let p1 = Point { x: 10, y: 20 };
            let p2 = Point { x: 10, y: 30 };
            p1 != p2
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

// Struct spread tests

#[test]
fn test_run_struct_spread_override_field() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            let q = Point { x: 10, ..p };
            q.x + q.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(12));
}

#[test]
fn test_run_struct_spread_copy_all() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            let q = Point { ..p };
            q.x + q.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_struct_spread_generic() {
    let source = r#"
        struct Pair<T> { first: T, second: T }
        pub fn main() -> Int {
            let p = Pair { first: 10, second: 20 };
            let q = Pair { first: 99, ..p };
            q.first + q.second
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(119));
}

#[test]
fn test_run_struct_spread_from_function() {
    let source = r#"
        struct Point { x: Int, y: Int }
        fn origin() -> Point {
            Point { x: 0, y: 0 }
        }
        pub fn main() -> Int {
            let p = Point { x: 5, ..origin() };
            p.x + p.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5));
}

#[test]
fn test_run_struct_spread_all_fields_overridden() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            let q = Point { x: 10, y: 20, ..p };
            q.x + q.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

// Tuple struct tests

#[test]
fn test_run_tuple_struct_construct() {
    let source = r#"
        struct Wrapper(Int)
        pub fn main() -> Int {
            let w = Wrapper(42);
            match w {
                Wrapper(n) => n,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_tuple_struct_two_fields() {
    let source = r#"
        struct Pair(String, Int)
        pub fn main() -> Int {
            let p = Pair("hello", 5);
            match p {
                Pair(_, n) => n,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5));
}

#[test]
fn test_run_tuple_struct_generic() {
    let source = r#"
        struct Box<T>(T)
        pub fn main() -> String {
            let b = Box("hello");
            match b {
                Box(s) => s,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("hello".to_string()));
}

#[test]
fn test_run_tuple_struct_match_prefix() {
    let source = r#"
        struct Triple(Int, Int, Int)
        pub fn main() -> Int {
            let t = Triple(1, 2, 3);
            match t {
                Triple(a, ..) => a,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_run_tuple_struct_match_suffix() {
    let source = r#"
        struct Triple(Int, Int, Int)
        pub fn main() -> Int {
            let t = Triple(1, 2, 3);
            match t {
                Triple(.., c) => c,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_tuple_struct_match_prefix_suffix() {
    let source = r#"
        struct Triple(Int, Int, Int)
        pub fn main() -> Int {
            let t = Triple(1, 2, 3);
            match t {
                Triple(a, .., c) => a + c,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(4));
}

#[test]
fn test_run_tuple_struct_match_destructure() {
    let source = r#"
        struct Pair(Int, String)
        pub fn main() -> Int {
            let p = Pair(42, "hello");
            match p {
                Pair(n, _) => n,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

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

#[test]
fn test_run_tuple_struct_equality() {
    let source = r#"
        struct Pair(Int, Int)
        pub fn main() -> Bool {
            Pair(1, 2) == Pair(1, 2)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_tuple_struct_as_function_arg() {
    let source = r#"
        struct Wrapper(Int)
        fn unwrap(w: Wrapper) -> Int {
            match w {
                Wrapper(n) => n,
            }
        }
        pub fn main() -> Int {
            unwrap(Wrapper(99))
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(99));
}

// Enum tests

#[test]
fn test_run_enum_unit_variant() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let x = Option::None;
            match x {
                Option::None => 0,
                Option::Some(v) => v
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_run_enum_tuple_variant() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let x = Option::Some(42);
            match x {
                Option::None => 0,
                Option::Some(v) => v
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_enum_struct_variant() {
    let source = r#"
        enum Message { Quit, Move { x: Int, y: Int }, Write(String) }
        pub fn main() -> Int {
            let msg = Message::Move { x: 10, y: 20 };
            match msg {
                Message::Quit => 0,
                Message::Move { x, y } => x + y,
                Message::Write(s) => s.len()
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_run_enum_all_variant_types() {
    let source = r#"
        enum Message { Quit, Move { x: Int, y: Int }, Write(String) }
        fn handle_quit() -> Int {
            let msg = Message::Quit;
            match msg {
                Message::Quit => 1,
                Message::Move { x, y } => x + y,
                Message::Write(s) => s.len()
            }
        }
        fn handle_write() -> Int {
            let msg = Message::Write("hello");
            match msg {
                Message::Quit => 0,
                Message::Move { x, y } => x + y,
                Message::Write(s) => s.len()
            }
        }
        pub fn main() -> Int {
            handle_quit() + handle_write()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(6)); // 1 + 5
}

#[test]
fn test_run_enum_generic_multiple_types() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let x = Option::Some(10);
            let y = Option::Some("hello");
            let a = match x {
                Option::Some(v) => v,
                Option::None => 0
            };
            let b = match y {
                Option::Some(s) => s.len(),
                Option::None => 0
            };
            a + b
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(15)); // 10 + 5
}

#[test]
fn test_run_enum_nested_pattern() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let x = Option::Some((1, 2));
            match x {
                Option::None => 0,
                Option::Some((a, b)) => a + b
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_enum_partial_struct_pattern() {
    let source = r#"
        enum Message { Quit, Move { x: Int, y: Int, z: Int } }
        pub fn main() -> Int {
            let msg = Message::Move { x: 1, y: 2, z: 3 };
            match msg {
                Message::Quit => 0,
                Message::Move { x, .. } => x
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_run_match_multiple_unit_variants() {
    let source = r#"
        enum Color { Red, Green, Blue }
        pub fn main() -> Int {
            let x = Color::Blue;
            match x {
                Color::Red => 1,
                Color::Green => 2,
                Color::Blue => 3
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_run_enum_wildcard_pattern() {
    let source = r#"
        enum Message { Quit, Move { x: Int, y: Int }, Write(String) }
        pub fn main() -> Int {
            let msg = Message::Write("hello");
            match msg {
                Message::Quit => 0,
                _ => 42
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_enum_equality() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Bool {
            let x = Option::Some(42);
            let y = Option::Some(42);
            x == y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_enum_inequality() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Bool {
            let x = Option::Some(42);
            let y = Option::None;
            x != y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_run_enum_multi_field_tuple() {
    let source = r#"
        enum Result<T, E> { Ok(T), Err(E) }
        pub fn main() -> Int {
            let x = Result::Ok(42);
            match x {
                Result::Ok(v) => v,
                Result::Err(e) => e
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

// Turbofish syntax tests

#[test]
fn test_turbofish_unit_variant() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let x = Option::None::<Int>;
            match x {
                Option::None => 0,
                Option::Some(v) => v
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_turbofish_tuple_variant() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let x = Option::Some::<Int>(42);
            match x {
                Option::None => 0,
                Option::Some(v) => v
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_turbofish_function_call() {
    let source = r#"
        fn identity<T>(x: T) -> T x
        pub fn main() -> Int {
            identity::<Int>(42)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_turbofish_multiple_type_args() {
    let source = r#"
        enum Result<T, E> { Ok(T), Err(E) }
        pub fn main() -> Int {
            let x = Result::Ok::<Int, String>(42);
            match x {
                Result::Ok(v) => v,
                Result::Err(_) => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

// As pattern (@) tests
#[test]
fn test_as_pattern_literal() {
    let source = r#"
        pub fn main() -> Int {
            let x = 42;
            match x {
                n @ 42 => n,
                _ => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_as_pattern_with_enum() {
    let source = r#"
        enum Option<T> { None, Some(T) }
        pub fn main() -> Int {
            let opt = Option::Some(10);
            match opt {
                whole @ Option::Some(x) => x,
                Option::None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_list_rest_binding() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [1, 2, 3, 4];
            match xs {
                [first, rest @ ..] => {
                    match rest {
                        [a, ..] => a,
                        [] => 0
                    }
                },
                [] => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(2));
}

#[test]
fn test_list_rest_binding_suffix() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [1, 2, 3, 4];
            match xs {
                [rest @ .., last] => {
                    match rest {
                        [.., x] => x,
                        [] => 0
                    }
                },
                [] => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_list_rest_binding_middle() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [1, 2, 3, 4, 5];
            match xs {
                [_, middle @ .., _] => {
                    match middle {
                        [a, ..] => a,
                        [] => 0
                    }
                },
                _ => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(2));
}

#[test]
fn test_tuple_rest_binding() {
    // rest @ .. on (1, 2, 3) with (first, rest @ ..) gives rest: (Int, Int)
    let source = r#"
        pub fn main() -> Int {
            let t = (1, 2, 3);
            match t {
                (first, rest @ ..) => {
                    match rest {
                        (a, _) => a
                    }
                }
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(2));
}

#[test]
fn test_tuple_rest_binding_suffix() {
    // rest @ .. on (1, 2, 3, 4) with (rest @ .., last) gives rest: (Int, Int, Int)
    let source = r#"
        pub fn main() -> Int {
            let t = (1, 2, 3, 4);
            match t {
                (rest @ .., last) => {
                    match rest {
                        (_, b, _) => b
                    }
                }
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(2));
}

// ===== Let Pattern Destructuring Tests =====

#[test]
fn test_let_tuple_destructuring() {
    let source = r#"
        pub fn main() -> Int {
            let (a, b) = (1, 2);
            a + b
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_let_tuple_nested_destructuring() {
    let source = r#"
        pub fn main() -> Int {
            let (a, (b, c)) = (1, (2, 3));
            a + b + c
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(6));
}

#[test]
fn test_let_struct_destructuring() {
    let source = r#"
        struct Point { x: Int, y: Int }
        pub fn main() -> Int {
            let p = Point { x: 10, y: 20 };
            let Point { x, y } = p;
            x + y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_let_struct_partial_destructuring() {
    let source = r#"
        struct Point { x: Int, y: Int, z: Int }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2, z: 3 };
            let Point { x, .. } = p;
            x
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_let_tuple_rest_prefix() {
    let source = r#"
        pub fn main() -> Int {
            let (first, ..) = (1, 2, 3, 4);
            first
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_let_tuple_rest_suffix() {
    let source = r#"
        pub fn main() -> Int {
            let (.., last) = (1, 2, 3, 4);
            last
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(4));
}

#[test]
fn test_let_wildcard() {
    let source = r#"
        pub fn main() -> Int {
            let _ = 42;
            0
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_let_as_pattern() {
    let source = r#"
        pub fn main() -> Int {
            let pair @ (a, b) = (1, 2);
            match pair {
                (x, y) => a + b + x + y,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(6));
}

#[test]
fn test_let_empty_tuple() {
    let source = r#"
        pub fn main() -> Int {
            let () = ();
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_function_tuple_param_destructuring() {
    let source = r#"
        fn swap((a, b): (Int, Int)) -> (Int, Int) (b, a)
        pub fn main() -> Int {
            let (x, y) = swap((1, 2));
            x * 10 + y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(21)); // x=2, y=1, so 2*10+1=21
}

#[test]
fn test_function_nested_tuple_param() {
    let source = r#"
        fn nested(((a, b), c): ((Int, Int), Int)) -> Int a + b + c
        pub fn main() -> Int { nested(((1, 2), 3)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(6));
}

#[test]
fn test_function_struct_param_destructuring() {
    let source = r#"
        struct Point { x: Int, y: Int }
        fn get_x(Point { x, .. }: Point) -> Int x
        pub fn main() -> Int { get_x(Point { x: 42, y: 0 }) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_lambda_tuple_param_destructuring() {
    let source = r#"
        pub fn main() -> Int {
            let add = |(a, b): (Int, Int)| a + b;
            add((3, 4))
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_lambda_tuple_param_type_inference() {
    let source = r#"
        pub fn main() -> Int {
            let add = |(a, b)| a + b;
            add((3, 4))
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_function_wildcard_param() {
    let source = r#"
        fn first((a, _): (Int, Int)) -> Int a
        pub fn main() -> Int { first((5, 10)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5));
}

#[test]
fn test_function_as_pattern_param() {
    let source = r#"
        fn with_as(pair @ (a, b): (Int, Int)) -> Int {
            let (x, y) = pair;
            a + b + x + y
        }
        pub fn main() -> Int { with_as((1, 2)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(6)); // 1+2+1+2=6
}

#[test]
fn test_type_alias_simple() {
    let source = r#"
        type UserId = Int
        fn get_id() -> UserId { 42 }
        pub fn main() -> Int { get_id() }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_type_alias_generic() {
    let source = r#"
        type Pair<A, B> = (A, B)
        fn make_pair() -> Pair<Int, Bool> { (1, true) }
        pub fn main() -> Int {
            let (x, _) = make_pair();
            x
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_type_alias_function_type() {
    // Test that type alias works for function types as parameter types
    let source = r#"
        type IntOp = (Int) -> Int
        fn apply(f: IntOp, x: Int) -> Int { f(x) }
        pub fn main() -> Int { apply(|x| x * 2, 21) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_type_alias_to_list() {
    let source = r#"
        type Numbers = List<Int>
        fn sum(ns: Numbers) -> Int {
            match ns {
                [] => 0,
                [x, rest @ ..] => x + sum(rest),
            }
        }
        pub fn main() -> Int { sum([1, 2, 3, 4]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

// ============================================================================
// Module Integration Tests
// ============================================================================

/// Helper function to run a multi-module package and assert the result.
/// Modules are specified as (path, source) tuples.
/// The first module should be "root" containing `pub fn main()`.
/// The expected value is compared using Display representation.
fn run_multi_module(modules: Vec<(&str, &str)>, expected: &str) {
    let mut source = MemorySource::new();
    for (path, content) in modules {
        source.add_module(path, content);
    }
    let package = load_memory_package(&source, zoya_loader::Mode::Dev)
        .unwrap_or_else(|e| panic!("failed to load package: {}", e));
    let checked =
        check(&package, &[]).unwrap_or_else(|e| panic!("failed to type check package: {}", e));
    let result = Runner::new()
        .package(&checked, [])
        .run()
        .unwrap_or_else(|e| panic!("failed to run package: {}", e));
    assert_eq!(result.to_string(), expected, "unexpected result");
}

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

/// Helper function to run multi-module code with std library available
fn run_multi_module_with_std(modules: Vec<(&str, &str)>, expected: &str) {
    let std = zoya_std();
    let mut source = MemorySource::new();
    for (path, content) in modules {
        source.add_module(path, content);
    }
    let package = load_memory_package(&source, zoya_loader::Mode::Dev)
        .unwrap_or_else(|e| panic!("failed to load package: {}", e));
    let checked =
        check(&package, &[std]).unwrap_or_else(|e| panic!("failed to type check package: {}", e));
    let result = Runner::new()
        .package(&checked, [std])
        .run()
        .unwrap_or_else(|e| panic!("failed to run package: {}", e));
    assert_eq!(result.to_string(), expected, "unexpected result");
}

// ===== Basic Module Imports =====

#[test]
fn test_module_pub_fn_qualified_call() {
    run_multi_module(
        vec![
            (
                "root",
                "mod utils\npub fn main() -> Int { utils::add(1, 2) }",
            ),
            ("utils", "pub fn add(x: Int, y: Int) -> Int { x + y }"),
        ],
        "3",
    );
}

#[test]
fn test_module_pub_fn_with_use() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::add
            pub fn main() -> Int { add(1, 2) }
        "#,
            ),
            ("utils", "pub fn add(x: Int, y: Int) -> Int { x + y }"),
        ],
        "3",
    );
}

#[test]
fn test_module_call_same_module_no_import() {
    run_multi_module(
        vec![(
            "root",
            r#"
            fn helper() -> Int { 42 }
            pub fn main() -> Int { helper() }
        "#,
        )],
        "42",
    );
}

#[test]
fn test_module_private_fn_same_module() {
    run_multi_module(
        vec![(
            "root",
            r#"
            fn secret() -> Int { 42 }
            pub fn main() -> Int { secret() }
        "#,
        )],
        "42",
    );
}

// ===== Use Path Prefixes =====

#[test]
fn test_use_root_prefix() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::helper
            pub fn main() -> Int { helper() }
        "#,
            ),
            ("utils", "pub fn helper() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_use_self_prefix() {
    run_multi_module(
        vec![(
            "root",
            r#"
            use self::helper
            fn helper() -> Int { 42 }
            pub fn main() -> Int { helper() }
        "#,
        )],
        "42",
    );
}

#[test]
fn test_use_super_prefix() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod child
            pub fn parent_fn() -> Int { 42 }
            pub fn main() -> Int { child::test() }
        "#,
            ),
            (
                "child",
                r#"
            use super::parent_fn
            pub fn test() -> Int { parent_fn() }
        "#,
            ),
        ],
        "42",
    );
}

#[test]
fn test_use_super_nested() {
    // Use root:: prefix to access root-level functions from nested modules
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod level1
            pub fn root_fn() -> Int { 100 }
            pub fn main() -> Int { level1::level2::test() }
        "#,
            ),
            ("level1", "pub mod level2"),
            (
                "level1/level2",
                r#"
            use root::root_fn
            pub fn test() -> Int { root_fn() }
        "#,
            ),
        ],
        "100",
    );
}

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
fn test_visibility_pub_fn_accessible_everywhere() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            pub fn main() -> Int { utils::helper() }
        "#,
            ),
            ("utils", "pub fn helper() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_visibility_private_fn_accessible_in_same_module() {
    run_multi_module(
        vec![(
            "root",
            r#"
            fn private_helper() -> Int { 42 }
            pub fn main() -> Int { private_helper() }
        "#,
        )],
        "42",
    );
}

#[test]
fn test_visibility_private_fn_accessible_in_child_module() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod child
            fn private_helper() -> Int { 42 }
            pub fn main() -> Int { child::test() }
        "#,
            ),
            (
                "child",
                r#"
            use super::private_helper
            pub fn test() -> Int { private_helper() }
        "#,
            ),
        ],
        "42",
    );
}

#[test]
fn test_visibility_private_fn_accessible_in_deep_descendant() {
    // Deep descendants can access root's private functions via root:: prefix
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod level1
            fn root_secret() -> Int { 99 }
            pub fn main() -> Int { level1::level2::level3::test() }
        "#,
            ),
            ("level1", "pub mod level2"),
            ("level1/level2", "pub mod level3"),
            (
                "level1/level2/level3",
                r#"
            use root::root_secret
            pub fn test() -> Int { root_secret() }
        "#,
            ),
        ],
        "99",
    );
}

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

#[test]
fn test_visibility_struct_always_public() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            pub fn main() -> Int {
                let p = types::Point { x: 10, y: 20 };
                p.x + p.y
            }
        "#,
            ),
            ("types", "pub struct Point { x: Int, y: Int }"),
        ],
        "30",
    );
}

#[test]
fn test_visibility_enum_always_public() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            pub fn main() -> Int {
                let opt = types::Option::Some(42);
                match opt {
                    types::Option::Some(x) => x,
                    types::Option::None => 0,
                }
            }
        "#,
            ),
            ("types", "pub enum Option<T> { None, Some(T) }"),
        ],
        "42",
    );
}

// ===== Complex Module Hierarchies =====

#[test]
fn test_module_three_level_hierarchy() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod utils
            pub fn main() -> Int { utils::helpers::deep_fn() }
        "#,
            ),
            ("utils", "pub mod helpers"),
            ("utils/helpers", "pub fn deep_fn() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_module_sibling_imports() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod a
            mod b
            pub fn main() -> Int { b::use_a() }
        "#,
            ),
            ("a", "pub fn get_val() -> Int { 10 }"),
            (
                "b",
                r#"
            use root::a::get_val
            pub fn use_a() -> Int { get_val() * 2 }
        "#,
            ),
        ],
        "20",
    );
}

#[test]
fn test_module_diamond_imports() {
    // root imports from a and b, both of which import from common
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod common
            mod a
            mod b
            pub fn main() -> Int { a::from_a() + b::from_b() }
        "#,
            ),
            ("common", "pub fn base() -> Int { 5 }"),
            (
                "a",
                r#"
            use root::common::base
            pub fn from_a() -> Int { base() * 2 }
        "#,
            ),
            (
                "b",
                r#"
            use root::common::base
            pub fn from_b() -> Int { base() * 3 }
        "#,
            ),
        ],
        "25", // 5*2 + 5*3
    );
}

#[test]
fn test_module_grandchild_to_root_access() {
    // Grandchild can access root's public function via root:: prefix
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod parent
            pub fn grandparent_fn() -> Int { 77 }
            pub fn main() -> Int { parent::child::test() }
        "#,
            ),
            ("parent", "pub mod child"),
            (
                "parent/child",
                r#"
            use root::grandparent_fn
            pub fn test() -> Int { grandparent_fn() }
        "#,
            ),
        ],
        "77",
    );
}

// ===== Type Imports =====

#[test]
fn test_import_struct_and_use() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            use root::types::Point
            pub fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                p.x + p.y
            }
        "#,
            ),
            ("types", "pub struct Point { x: Int, y: Int }"),
        ],
        "30",
    );
}

#[test]
fn test_import_struct_pattern_match() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            use root::types::Point
            pub fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                match p {
                    Point { x, y } => x * y,
                }
            }
        "#,
            ),
            ("types", "pub struct Point { x: Int, y: Int }"),
        ],
        "200",
    );
}

#[test]
fn test_import_enum_type_and_variants() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            use root::types::Option::Some
            use root::types::Option::None
            pub fn main() -> Int {
                let opt = Some(42);
                match opt {
                    Some(x) => x,
                    None => 0,
                }
            }
        "#,
            ),
            ("types", "pub enum Option<T> { None, Some(T) }"),
        ],
        "42",
    );
}

#[test]
fn test_import_enum_variant_in_pattern() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            use root::types::Result::Ok
            use root::types::Result::Err
            pub fn main() -> Int {
                let r = Ok(100);
                match r {
                    Ok(v) => v,
                    Err(e) => e,
                }
            }
        "#,
            ),
            ("types", "pub enum Result<T, E> { Ok(T), Err(E) }"),
        ],
        "100",
    );
}

#[test]
fn test_import_type_alias() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            use root::types::IntPair
            pub fn main() -> Int {
                let p: IntPair = (10, 20);
                match p {
                    (a, b) => a + b,
                }
            }
        "#,
            ),
            ("types", "pub type IntPair = (Int, Int)"),
        ],
        "30",
    );
}

#[test]
fn test_imported_generic_struct() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            use root::types::Pair
            pub fn main() -> Int {
                let p = Pair::<Int, Bool> { first: 42, second: true };
                p.first
            }
        "#,
            ),
            ("types", "pub struct Pair<A, B> { first: A, second: B }"),
        ],
        "42",
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
    // Parser treats prefix-free use paths as package paths; fails because no "utils" package exists
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
    // Parsing succeeds but check fails because there's no "utils" package dependency
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

// ===== Shadowing and Resolution =====

#[test]
fn test_shadowing_local_shadows_import() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::x
            pub fn main() -> Bool {
                let x = true;
                x
            }
        "#,
            ),
            ("utils", "pub fn x() -> Int { 42 }"),
        ],
        "true",
    );
}

#[test]
fn test_shadowing_import_shadows_module_level() {
    // Import takes priority over module-level definition
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::foo
            fn foo() -> Bool { true }
            pub fn main() -> Int { foo() }
        "#,
            ),
            ("utils", "pub fn foo() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_resolution_qualified_path_bypasses_import() {
    // helper() uses import (100), self::helper() uses local definition (1)
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::helper
            fn helper() -> Int { 1 }
            pub fn main() -> Int {
                let a = helper();
                let b = self::helper();
                a + b
            }
        "#,
            ),
            ("utils", "pub fn helper() -> Int { 100 }"),
        ],
        "101",
    );
}

#[test]
fn test_multiple_paths_same_function() {
    // 3 + 7 + 11 = 21
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod utils
            use root::utils::add
            pub fn main() -> Int {
                let a = add(1, 2);
                let b = utils::add(3, 4);
                let c = root::utils::add(5, 6);
                a + b + c
            }
        "#,
            ),
            ("utils", "pub fn add(x: Int, y: Int) -> Int { x + y }"),
        ],
        "21",
    );
}

// ===== Module Visibility (pub mod) Tests =====

#[test]
fn test_visibility_pub_mod_accessible_from_sibling() {
    // root declares pub mod utils; sibling module accesses items through it
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod utils
            pub mod other
            pub fn main() -> Int { other::call_utils() }
        "#,
            ),
            ("utils", "pub fn helper() -> Int { 42 }"),
            (
                "other",
                r#"
            use root::utils::helper
            pub fn call_utils() -> Int { helper() }
        "#,
            ),
        ],
        "42",
    );
}

#[test]
fn test_visibility_pub_mod_accessible_from_unrelated() {
    // Chain of pub mods accessible end-to-end from unrelated module
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub mod b
            pub fn main() -> Int { b::test() }
        "#,
            ),
            ("a", "pub mod inner\npub fn top() -> Int { 1 }"),
            ("a/inner", "pub fn deep() -> Int { 2 }"),
            (
                "b",
                r#"
            use root::a::inner::deep
            use root::a::top
            pub fn test() -> Int { deep() + top() }
        "#,
            ),
        ],
        "3",
    );
}

#[test]
fn test_visibility_private_mod_not_accessible_from_non_descendant() {
    // module `a` declares mod internal (private); module `b` (not a descendant of `a`) cannot access it
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
    // Private module inside `a` contains pub fn, but `b` (non-descendant) can't reach through it
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
fn test_visibility_private_mod_accessible_from_declaring_module() {
    // Module that declares a private submodule can access its items
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod internal
            pub fn main() -> Int { internal::helper() }
        "#,
            ),
            ("internal", "pub fn helper() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_visibility_private_mod_accessible_from_descendant() {
    // Descendant of declaring module can access the private submodule
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod internal
            pub mod child
            pub fn main() -> Int { child::test() }
        "#,
            ),
            ("internal", "pub fn helper() -> Int { 42 }"),
            (
                "child",
                r#"
            use root::internal::helper
            pub fn test() -> Int { helper() }
        "#,
            ),
        ],
        "42",
    );
}

#[test]
fn test_visibility_all_modules_in_path_must_be_visible() {
    // pub mod a -> mod b (private) -> pub fn f; accessing root::a::b::f from outside fails
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
fn test_visibility_nested_pub_mods_chain() {
    // Chain of pub mod all accessible end-to-end
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod a
            pub fn main() -> Int { a::b::c::deep() }
        "#,
            ),
            ("a", "pub mod b"),
            ("a/b", "pub mod c"),
            ("a/b/c", "pub fn deep() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_visibility_pub_use_parses() {
    // pub use parses without error (used locally like a normal import)
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod utils
            pub use root::utils::helper
            pub fn main() -> Int { helper() }
        "#,
            ),
            ("utils", "pub fn helper() -> Int { 42 }"),
        ],
        "42",
    );
}

#[test]
fn test_visibility_struct_through_private_mod_error() {
    // Private mod inside `a` contains pub struct; `b` (non-descendant of `a`) can't reach it
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
fn test_visibility_enum_through_pub_mod() {
    // pub mod contains pub enum; accessible from sibling via qualified path
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod types
            use root::types::Color::Red
            pub fn main() -> Int {
                match Red {
                    _ => 1,
                }
            }
        "#,
            ),
            (
                "types",
                r#"
            pub enum Color { Red, Blue }
        "#,
            ),
        ],
        "1",
    );
}

#[test]
fn test_visibility_private_mod_qualified_path_error() {
    // Accessing through private mod using qualified path (no import) also fails
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

#[test]
fn test_visibility_pub_mod_parsing() {
    // Verify pub mod and mod both parse correctly in same file
    run_multi_module(
        vec![
            (
                "root",
                r#"
            pub mod public_mod
            mod private_mod
            pub fn main() -> Int { public_mod::value() + private_mod::value() }
        "#,
            ),
            ("public_mod", "pub fn value() -> Int { 10 }"),
            ("private_mod", "pub fn value() -> Int { 20 }"),
        ],
        "30",
    );
}

// ===== pub use Re-export Tests =====

#[test]
fn test_pub_use_reexport_function_e2e() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod a
            mod b
            use root::b::helper
            pub fn main() -> Int { helper() }
        "#,
            ),
            ("a", "pub fn helper() -> Int { 42 }"),
            ("b", "pub use root::a::helper"),
        ],
        "42",
    );
}

#[test]
fn test_pub_use_reexport_enum_e2e() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            mod reexporter
            use root::reexporter::Color::Red
            pub fn main() -> Int {
                let x = Red;
                match x { Red => 1, _ => 2 }
            }
        "#,
            ),
            ("types", "pub enum Color { Red, Blue }"),
            ("reexporter", "pub use root::types::Color"),
        ],
        "1",
    );
}

#[test]
fn test_pub_use_reexport_enum_via_qualified_path_e2e() {
    // Also verify accessing via qualified path through the re-exporting module
    run_multi_module(
        vec![
            (
                "root",
                r#"
            mod types
            mod reexporter
            pub fn main() -> Int {
                let x = reexporter::Color::Red;
                match x { reexporter::Color::Red => 10, _ => 20 }
            }
        "#,
            ),
            ("types", "pub enum Color { Red, Blue }"),
            ("reexporter", "pub use root::types::Color"),
        ],
        "10",
    );
}

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
// Module Import Tests
// ============================================================================

#[test]
fn test_module_import_basic() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math
                pub fn main() -> Int { math::add(1, 2) }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
            ),
        ],
        "3",
    );
}

#[test]
fn test_module_import_enum_variant() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                use root::types
                pub fn main() -> Int {
                    match types::Color::Red {
                        types::Color::Red => 1,
                        types::Color::Green => 2,
                        types::Color::Blue => 3,
                    }
                }
            "#,
            ),
            (
                "types",
                r#"
                pub enum Color { Red, Green, Blue }
            "#,
            ),
        ],
        "1",
    );
}

// ============================================================================
// Glob Import Tests
// ============================================================================

#[test]
fn test_glob_import_functions() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math::*
                pub fn main() -> Int { add(1, mul(2, 3)) }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                pub fn mul(x: Int, y: Int) -> Int { x * y }
            "#,
            ),
        ],
        "7",
    );
}

#[test]
fn test_glob_import_skips_private() {
    // Private items in the module should NOT be imported by glob
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

#[test]
fn test_glob_import_enum() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                use root::types::*
                pub fn main() -> Int {
                    match Color::Red {
                        Color::Red => 1,
                        Color::Green => 2,
                        Color::Blue => 3,
                    }
                }
            "#,
            ),
            (
                "types",
                r#"
                pub enum Color { Red, Green, Blue }
            "#,
            ),
        ],
        "1",
    );
}

#[test]
fn test_glob_import_struct() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod geom
                use root::geom::*
                pub fn main() -> Int {
                    let p = Point { x: 3, y: 4 };
                    p.x + p.y
                }
            "#,
            ),
            (
                "geom",
                r#"
                pub struct Point { x: Int, y: Int }
            "#,
            ),
        ],
        "7",
    );
}

// ============================================================================
// Group Import Tests
// ============================================================================

#[test]
fn test_group_import_basic() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod math
                use root::math::{add, mul}
                pub fn main() -> Int { add(1, mul(2, 3)) }
            "#,
            ),
            (
                "math",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                pub fn mul(x: Int, y: Int) -> Int { x * y }
                pub fn sub(x: Int, y: Int) -> Int { x - y }
            "#,
            ),
        ],
        "7",
    );
}

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

#[test]
fn test_group_import_enum_and_function() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                use root::types::{Color, helper}
                pub fn main() -> Int {
                    match Color::Red {
                        Color::Red => helper(),
                        Color::Green => 2,
                        Color::Blue => 3,
                    }
                }
            "#,
            ),
            (
                "types",
                r#"
                pub enum Color { Red, Green, Blue }
                pub fn helper() -> Int { 42 }
            "#,
            ),
        ],
        "42",
    );
}

// ============================================================================
// Pub Use Glob/Group Re-export Tests
// ============================================================================

#[test]
fn test_pub_use_glob_reexport() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod a
                mod b
                use root::b::add
                pub fn main() -> Int { add(1, 2) }
            "#,
            ),
            (
                "a",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                pub fn mul(x: Int, y: Int) -> Int { x * y }
            "#,
            ),
            ("b", "pub use root::a::*"),
        ],
        "3",
    );
}

#[test]
fn test_pub_use_group_reexport() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod a
                mod b
                use root::b::add
                pub fn main() -> Int { add(10, 20) }
            "#,
            ),
            (
                "a",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                pub fn mul(x: Int, y: Int) -> Int { x * y }
            "#,
            ),
            ("b", "pub use root::a::{add}"),
        ],
        "30",
    );
}

#[test]
fn test_pub_use_module_reexport_namespace() {
    // Module b re-exports module a; root uses it as a namespace
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod a
                mod b
                use root::b::a
                pub fn main() -> Int { a::helper() }
            "#,
            ),
            ("a", "pub fn helper() -> Int { 42 }"),
            ("b", "pub use root::a"),
        ],
        "42",
    );
}

#[test]
fn test_pub_use_module_reexport_item_import() {
    // Module b re-exports module a; root imports a specific item through b::a
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod a
                mod b
                use root::b::a::helper
                pub fn main() -> Int { helper() }
            "#,
            ),
            ("a", "pub fn helper() -> Int { 42 }"),
            ("b", "pub use root::a"),
        ],
        "42",
    );
}

#[test]
fn test_pub_use_module_reexport_glob() {
    // Module b re-exports module a; root glob-imports through b::a
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod a
                mod b
                use root::b::a::*
                pub fn main() -> Int { add(10, 20) }
            "#,
            ),
            (
                "a",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
            ),
            ("b", "pub use root::a"),
        ],
        "30",
    );
}

#[test]
fn test_pub_use_module_reexport_group() {
    // Module b re-exports module a; root group-imports through b::a
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod a
                mod b
                use root::b::a::{add, mul}
                pub fn main() -> Int { add(2, 3) + mul(4, 5) }
            "#,
            ),
            (
                "a",
                r#"
                pub fn add(x: Int, y: Int) -> Int { x + y }
                pub fn mul(x: Int, y: Int) -> Int { x * y }
            "#,
            ),
            ("b", "pub use root::a"),
        ],
        "25",
    );
}

// ============================================================================
// Duplicate Import Tests
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

// ── Glob import on enum (use root::types::Color::*) ─────────────────

#[test]
fn test_glob_import_enum_variants() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                use root::types::Color::*
                pub fn main() -> Int {
                    match Red {
                        Red => 1,
                        Green => 2,
                        Blue => 3,
                    }
                }
            "#,
            ),
            ("types", "pub enum Color { Red, Green, Blue }"),
        ],
        "1",
    );
}

// ── Group import on enum (use root::types::Color::{Red, Green}) ─────

#[test]
fn test_group_import_enum_variants() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                use root::types::Color::{Red, Green, Blue}
                pub fn main() -> Int {
                    match Red {
                        Red => 1,
                        Green => 2,
                        Blue => 3,
                    }
                }
            "#,
            ),
            ("types", "pub enum Color { Red, Green, Blue }"),
        ],
        "1",
    );
}

// ── Glob import includes modules ────────────────────────────────────

#[test]
fn test_glob_import_includes_modules() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod parent
                use root::parent::*
                pub fn main() -> Int { child::helper() }
            "#,
            ),
            ("parent", "pub mod child"),
            ("parent/child", "pub fn helper() -> Int { 42 }"),
        ],
        "42",
    );
}

// ── pub use glob re-export includes modules ─────────────────────────

#[test]
fn test_pub_use_glob_reexport_module() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod parent
                mod reexporter
                use root::reexporter::child::helper
                pub fn main() -> Int { helper() }
            "#,
            ),
            (
                "parent",
                r#"
                pub mod child
                pub fn ignored() -> Int { 0 }
            "#,
            ),
            ("parent/child", "pub fn helper() -> Int { 99 }"),
            ("reexporter", "pub use root::parent::*"),
        ],
        "99",
    );
}

// ── glob import includes re-exported enum variants ──────────────────

#[test]
fn test_glob_import_reexported_enum_variants() {
    // Module "types" does pub use self::Color::* to lift variants to module level.
    // Root does use root::types::* and can reference Red/Green/Blue directly.
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                use root::types::*
                pub fn main() -> Int {
                    match Red {
                        Red => 1,
                        Green => 2,
                        Blue => 3,
                    }
                }
            "#,
            ),
            (
                "types",
                r#"
                pub enum Color { Red, Green, Blue }
                pub use self::Color::*
            "#,
            ),
        ],
        "1",
    );
}

// ── pub use glob re-export enum variants ────────────────────────────

#[test]
fn test_pub_use_glob_reexport_enum_variants() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                mod reexporter
                use root::reexporter::{Some, None}
                pub fn main() -> Int {
                    match Some(42) {
                        Some(x) => x,
                        None => 0,
                    }
                }
            "#,
            ),
            ("types", "pub enum Option<T> { None, Some(T) }"),
            ("reexporter", "pub use root::types::Option::*"),
        ],
        "42",
    );
}

// ── pub use group re-export enum variants ───────────────────────────

#[test]
fn test_pub_use_group_reexport_enum_variants() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod types
                mod reexporter
                use root::reexporter::{Red, Green, Blue}
                pub fn main() -> Int {
                    match Green {
                        Red => 1,
                        Green => 2,
                        Blue => 3,
                    }
                }
            "#,
            ),
            ("types", "pub enum Color { Red, Green, Blue }"),
            (
                "reexporter",
                "pub use root::types::Color::{Red, Green, Blue}",
            ),
        ],
        "2",
    );
}

// ── pub use group re-export mixing modules and items ────────────────

#[test]
fn test_pub_use_group_reexport_module_and_items() {
    run_multi_module(
        vec![
            (
                "root",
                r#"
                mod parent
                mod reexporter
                use root::reexporter::{child, add}
                pub fn main() -> Int { add(child::helper(), 1) }
            "#,
            ),
            (
                "parent",
                r#"
                pub mod child
                pub fn add(x: Int, y: Int) -> Int { x + y }
            "#,
            ),
            ("parent/child", "pub fn helper() -> Int { 10 }"),
            ("reexporter", "pub use root::parent::{child, add}"),
        ],
        "11",
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
fn test_std_option_match_some() {
    let source = r#"
        use std::option::{Option, Some, None}

        pub fn main() -> Int {
            let x = Some(42);
            match x {
                Some(v) => v,
                None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_std_option_match_none() {
    let source = r#"
        use std::option::{Option, None}

        pub fn main() -> Int {
            let x: Option<Int> = None::<Int>;
            match x {
                Option::Some(v) => v,
                None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_std_option_match_with_qualified_patterns() {
    let source = r#"
        use std::option::Option

        pub fn main() -> Int {
            let x = Option::Some(10);
            match x {
                Option::Some(v) => v * 2,
                Option::None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(20));
}

#[test]
fn test_std_option_in_function_signature() {
    let source = r#"
        use std::option::{Option, Some, None}

        fn unwrap_or(opt: Option<Int>, default: Int) -> Int {
            match opt {
                Some(v) => v,
                None => default
            }
        }

        pub fn main() -> Int {
            unwrap_or(Some(5), 0) + unwrap_or(None::<Int>, 10)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(15));
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
fn test_std_option_with_string() {
    let source = r#"
        use std::option::{Option, Some, None}

        pub fn main() -> Int {
            let x = Some("hello");
            match x {
                Some(s) => s.len(),
                None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(5));
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

#[test]
fn test_std_option_multi_module_with_use() {
    run_multi_module_with_std(
        vec![
            (
                "root",
                r#"
                mod helper
                use std::option::Option
                use root::helper::make_some

                pub fn main() -> Option<Int> { make_some(42) }
            "#,
            ),
            (
                "helper",
                r#"
                use std::option::{Option, Some}

                pub fn make_some(x: Int) -> Option<Int> { Some(x) }
            "#,
            ),
        ],
        "Option::Some(42)",
    );
}

#[test]
fn test_std_option_multi_module_match() {
    run_multi_module_with_std(
        vec![
            (
                "root",
                r#"
                mod utils
                use std::option::{Option, Some, None}
                use root::utils::safe_div

                pub fn main() -> Int {
                    let a = safe_div(10, 2);
                    let b = safe_div(10, 0);
                    let va = match a { Some(v) => v, None => 0 };
                    let vb = match b { Some(v) => v, None => -1 };
                    va + vb
                }
            "#,
            ),
            (
                "utils",
                r#"
                use std::option::{Option, Some, None}

                pub fn safe_div(a: Int, b: Int) -> Option<Int> {
                    match b == 0 {
                        true => None::<Int>,
                        false => Some(a / b)
                    }
                }
            "#,
            ),
        ],
        "4",
    );
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

#[test]
fn test_std_option_direct_match_pattern() {
    let source = r#"
        pub fn main() -> Int {
            let x = std::option::Some(42);
            match x {
                std::option::Some(v) => v,
                std::option::Option::None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_std_option_direct_type_annotation() {
    let source = r#"
        pub fn main() -> Int {
            let x: std::option::Option<Int> = std::option::Some(10);
            match x {
                std::option::Some(v) => v + 1,
                std::option::Option::None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(11));
}

#[test]
fn test_std_option_direct_multi_module() {
    run_multi_module_with_std(
        vec![
            (
                "root",
                r#"
                mod helper

                use root::helper::make_some

                pub fn main() -> std::option::Option<Int> { make_some(42) }
            "#,
            ),
            (
                "helper",
                r#"
                pub fn make_some(x: Int) -> std::option::Option<Int> { std::option::Some(x) }
            "#,
            ),
        ],
        "Option::Some(42)",
    );
}

#[test]
fn test_std_option_glob_import() {
    let source = r#"
        use std::option::*

        pub fn main() -> Int {
            match Some(42) {
                Some(v) => v,
                None => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
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
fn test_prelude_match_option_without_import() {
    let source = r#"
        pub fn main() -> Int {
            let x = Some(42);
            match x {
                Some(v) => v,
                None => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_prelude_match_result_without_import() {
    let source = r#"
        pub fn main() -> Int {
            let x: Result<Int, String> = Ok(10);
            match x {
                Ok(v) => v,
                Err(_) => 0
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_prelude_explicit_import_takes_priority() {
    // Explicit import should not conflict with prelude
    let source = r#"
        use std::option::{Option, Some, None}

        pub fn main() -> Option<Int> { Some(99) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "Option::Some(99)");
}

#[test]
fn test_cascading_glob_reexport_between_same_depth_modules() {
    // Module "types" defines enum + re-exports variants to module level
    // Module "facade" re-exports everything from types via pub use root::types::*
    // Root imports from facade and uses the variant directly
    // This tests that cascading re-exports work regardless of module processing order
    run_multi_module(
        vec![
            (
                "root",
                r#"
                pub mod types
                pub mod facade
                use root::facade::*

                pub fn main() -> Int {
                    match Red {
                        Red => 1,
                        Blue => 2,
                    }
                }
            "#,
            ),
            (
                "types",
                r#"
                pub enum Color { Red, Blue }
                pub use self::Color::*
            "#,
            ),
            (
                "facade",
                r#"
                pub use root::types::*
            "#,
            ),
        ],
        "1",
    );
}

// List index tests

#[test]
fn test_list_index_in_bounds() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [10, 20, 30];
            match xs[1] {
                Some(v) => v,
                None => -1
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(20));
}

#[test]
fn test_list_index_out_of_bounds() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [10, 20, 30];
            match xs[5] {
                Some(v) => v,
                None => -1
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(-1));
}

#[test]
fn test_list_index_negative() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [10, 20, 30];
            match xs[-1] {
                Some(v) => v,
                None => -1
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_list_index_negative_out_of_bounds() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [10, 20, 30];
            match xs[-10] {
                Some(v) => v,
                None => -1
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(-1));
}

#[test]
fn test_list_index_variable() {
    let source = r#"
        pub fn main() -> Int {
            let xs = [10, 20, 30];
            let i = 2;
            match xs[i] {
                Some(v) => v,
                None => -1
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_list_index_first_element() {
    let source = r#"
        pub fn main() -> Int {
            match [1, 2, 3][0] {
                Some(v) => v,
                None => -1
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

// === JSON module tests ===

#[test]
fn test_json_null() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> Int {
            let j = JSON::Null;
            match j {
                JSON::Null => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_bool() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> Bool {
            let j = JSON::Bool(true);
            match j {
                JSON::Bool(b) => b,
                _ => false,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_json_number_int() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> Int {
            let n = Number::Int(42);
            let j = JSON::Number(n);
            match j {
                JSON::Number(Number::Int(v)) => v,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_json_number_float() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> Float {
            let n = Number::Float(3.14);
            let j = JSON::Number(n);
            match j {
                JSON::Number(Number::Float(v)) => v,
                _ => 0.0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Float(3.14));
}

#[test]
fn test_json_string() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> String {
            let j = JSON::String("hello");
            match j {
                JSON::String(s) => s,
                _ => "fail",
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("hello".to_string()));
}

#[test]
fn test_json_array() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> Int {
            let arr = JSON::Array([JSON::Null, JSON::Bool(true)]);
            match arr {
                JSON::Array(_) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_object() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> Int {
            let obj = JSON::Object(Dict::from([("key", JSON::String("value"))]));
            match obj {
                JSON::Object(_) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_fmt_null() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> JSON { JSON::Null }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "JSON::Null");
}

#[test]
fn test_json_fmt_bool() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> JSON { JSON::Bool(true) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "JSON::Bool(true)");
}

#[test]
fn test_json_fmt_number_int() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> JSON { JSON::Number(Number::Int(42)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "JSON::Number(Number::Int(42))");
}

#[test]
fn test_json_fmt_number_float() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> JSON { JSON::Number(Number::Float(3.14)) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "JSON::Number(Number::Float(3.14))");
}

#[test]
fn test_json_fmt_string() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> JSON { JSON::String("hello") }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), r#"JSON::String("hello")"#);
}

#[test]
fn test_json_fmt_array() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> JSON { JSON::Array([JSON::Null, JSON::Bool(true)]) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result.to_string(),
        "JSON::Array([JSON::Null, JSON::Bool(true)])"
    );
}

#[test]
fn test_json_fmt_object() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> JSON { JSON::Object(Dict::from([("key", JSON::String("value"))])) }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result.to_string(),
        "JSON::Object({\"key\": JSON::String(\"value\")})"
    );
}

#[test]
fn test_json_fmt_parse_error() {
    let source = r#"
        use std::json::ParseError

        pub fn main() -> ParseError { ParseError::ParseError }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result.to_string(), "ParseError::ParseError");
}

// ==================== Recursive Type Tests ====================

#[test]
fn test_recursive_enum_exhaustive_match_inner() {
    // Matching on an inner recursive enum value should check all variants
    let source = r#"
        pub enum Tree { Leaf(Int), Branch(List<Tree>) }

        fn get_value(t: Tree) -> Int {
            match t {
                Tree::Leaf(n) => n,
                Tree::Branch(_) => -1,
            }
        }

        fn first_child(children: List<Tree>) -> Tree {
            match children {
                [h, ..] => h,
                [] => Tree::Leaf(0),
            }
        }

        pub fn main() -> Int {
            let t = Tree::Branch([Tree::Leaf(1), Tree::Leaf(2)]);
            match t {
                Tree::Branch(children) => get_value(first_child(children)),
                Tree::Leaf(n) => n,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_recursive_enum_non_exhaustive_inner() {
    // Matching on inner recursive enum with missing variant should fail
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
fn test_recursive_struct_field_access() {
    // Field access on inner recursive struct values should work
    let source = r#"
        pub struct Node { value: Int, children: List<Node> }

        pub fn main() -> Int {
            let child = Node { value: 10, children: [] };
            let parent = Node { value: 1, children: [child] };
            match parent.children {
                [first, ..] => first.value,
                [] => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_json_exhaustive_match_inner() {
    // Exhaustive match on inner JSON value extracted from Array
    let source = r#"
        use std::json::JSON

        fn json_tag(j: JSON) -> Int {
            match j {
                JSON::Null => 1,
                JSON::Bool(_) => 2,
                JSON::Number(_) => 3,
                JSON::String(_) => 4,
                JSON::Array(_) => 5,
                JSON::Object(_) => 6,
            }
        }

        fn first_or_null(items: List<JSON>) -> JSON {
            match items {
                [h, ..] => h,
                [] => JSON::Null,
            }
        }

        pub fn main() -> Int {
            let arr = JSON::Array([JSON::Null, JSON::Bool(true)]);
            match arr {
                JSON::Array(items) => json_tag(first_or_null(items)),
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_recursive_enum_non_exhaustive_inner_multi_variant() {
    // Non-exhaustive match on recursive enum with many variants should fail
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

// ── builtin function tests ─────────────────────────────────────────

#[test]
fn test_json_parse_int() {
    let source = r#"
        use std::json::{JSON, parse}

        pub fn main() -> Int {
            match parse("42") {
                Ok(JSON::Number(_)) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_parse_string() {
    let source = r#"
        use std::json::{JSON, parse}

        pub fn main() -> Int {
            match parse("\"hello\"") {
                Ok(JSON::String(s)) => {
                    match s == "hello" {
                        true => 1,
                        false => 0,
                    }
                },
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_parse_null() {
    let source = r#"
        use std::json::{JSON, parse}

        pub fn main() -> Int {
            match parse("null") {
                Ok(JSON::Null) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_parse_bool() {
    let source = r#"
        use std::json::{JSON, parse}

        pub fn main() -> Int {
            match parse("true") {
                Ok(JSON::Bool(b)) => {
                    match b {
                        true => 1,
                        false => 0,
                    }
                },
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_parse_array() {
    let source = r#"
        use std::json::{JSON, parse}

        pub fn main() -> Int {
            match parse("[1, 2, 3]") {
                Ok(JSON::Array(_)) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_parse_object() {
    let source = r#"
        use std::json::{JSON, parse}

        pub fn main() -> Int {
            match parse("{\"a\": 1}") {
                Ok(JSON::Object(_)) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_json_parse_invalid() {
    let source = r#"
        use std::json::{parse, ParseError}

        pub fn main() -> Int {
            match parse("not json") {
                Err(ParseError::ParseError) => 1,
                _ => 0,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

// ==================== JSON to_string Tests ====================

#[test]
fn test_json_to_string_null() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> String {
            JSON::Null.to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("null".to_string()));
}

#[test]
fn test_json_to_string_bool_true() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> String {
            JSON::Bool(true).to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("true".to_string()));
}

#[test]
fn test_json_to_string_bool_false() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> String {
            JSON::Bool(false).to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("false".to_string()));
}

#[test]
fn test_json_to_string_number_int() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> String {
            JSON::Number(Number::Int(42)).to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("42".to_string()));
}

#[test]
fn test_json_to_string_number_float() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> String {
            JSON::Number(Number::Float(3.14)).to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("3.14".to_string()));
}

#[test]
fn test_json_to_string_string() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> String {
            JSON::String("hello").to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("\"hello\"".to_string()));
}

#[test]
fn test_json_to_string_array() {
    let source = r#"
        use std::json::{JSON, Number}

        pub fn main() -> String {
            JSON::Array([JSON::Number(Number::Int(1)), JSON::Number(Number::Int(2)), JSON::Number(Number::Int(3))]).to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("[1,2,3]".to_string()));
}

#[test]
fn test_json_to_string_object() {
    let source = r#"
        use std::json::JSON

        pub fn main() -> String {
            JSON::Object(Dict::from([("key", JSON::String("value"))])).to_string()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("{\"key\":\"value\"}".to_string()));
}

#[test]
fn test_json_to_string_round_trip() {
    let source = r#"
        use std::json::{JSON, Number, parse}

        pub fn main() -> Bool {
            let original = JSON::Array([JSON::Number(Number::Int(1)), JSON::Bool(true), JSON::Null]);
            let serialized = original.to_string();
            let parsed = parse(serialized);
            parsed == Ok(original)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

// ==================== Rest Binding Tests ====================

#[test]
fn test_enum_tuple_rest_binding_prefix() {
    let source = r#"
        enum Data {
            Quad(Int, Int, Int, Int),
        }

        pub fn main() -> Int {
            let d = Data::Quad(10, 20, 30, 40);
            match d {
                Data::Quad(first, rest @ ..) => match rest {
                    (a, b, c) => a + b + c,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(90));
}

#[test]
fn test_enum_tuple_rest_binding_suffix() {
    let source = r#"
        enum Data {
            Quad(Int, Int, Int, Int),
        }

        pub fn main() -> Int {
            let d = Data::Quad(10, 20, 30, 40);
            match d {
                Data::Quad(rest @ .., last) => match rest {
                    (a, b, c) => a + b + c,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(60));
}

#[test]
fn test_enum_tuple_rest_binding_prefix_suffix() {
    let source = r#"
        enum Data {
            Quad(Int, Int, Int, Int),
        }

        pub fn main() -> Int {
            let d = Data::Quad(10, 20, 30, 40);
            match d {
                Data::Quad(first, rest @ .., last) => match rest {
                    (a, b) => a + b,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(50));
}

#[test]
fn test_tuple_struct_rest_binding_prefix() {
    let source = r#"
        struct Quad(Int, Int, Int, Int)

        pub fn main() -> Int {
            let q = Quad(10, 20, 30, 40);
            match q {
                Quad(first, rest @ ..) => match rest {
                    (a, b, c) => a + b + c,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(90));
}

#[test]
fn test_tuple_struct_rest_binding_suffix() {
    let source = r#"
        struct Quad(Int, Int, Int, Int)

        pub fn main() -> Int {
            let q = Quad(10, 20, 30, 40);
            match q {
                Quad(rest @ .., last) => match rest {
                    (a, b, c) => a + b + c,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(60));
}

#[test]
fn test_tuple_struct_rest_binding_prefix_suffix() {
    let source = r#"
        struct Quad(Int, Int, Int, Int)

        pub fn main() -> Int {
            let q = Quad(10, 20, 30, 40);
            match q {
                Quad(first, rest @ .., last) => match rest {
                    (a, b) => a + b,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(50));
}

#[test]
fn test_tuple_rest_binding_prefix_suffix() {
    let source = r#"
        pub fn main() -> Int {
            let t = (10, 20, 30, 40);
            match t {
                (first, rest @ .., last) => match rest {
                    (a, b) => a + b,
                },
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(50));
}

// =============================================================================
// Audit fixes: correctness tests
// =============================================================================

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
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("arithmetic operators only work on numeric types"))
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
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("ordering operators only work on numeric types"))
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
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("ordering operators only work on numeric types"))
    );
}

// Issue 1: Numeric operators consistent with type variables (lambda inference)
#[test]
fn test_arithmetic_lambda_inference() {
    let source = r#"
        pub fn main() -> Int {
            let add = |x, y| x + y;
            add(10, 32)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_ordering_lambda_inference() {
    let source = r#"
        pub fn main() -> Bool {
            let lt = |x, y| x < y;
            lt(1, 2)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_negation_lambda_inference() {
    let source = r#"
        pub fn main() -> Int {
            let neg = |x| -x;
            neg(42)
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(-42));
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
    assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("type mismatch")));
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
    assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("type argument")));
}

#[test]
fn test_turbofish_named_struct_correct() {
    let source = r#"
        pub struct Pair<A, B> { first: A, second: B }
        pub fn main() -> Int {
            let p = Pair::<Int, String> { first: 42, second: "hello" };
            p.first
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
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
    assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("type mismatch")));
}

#[test]
fn test_turbofish_enum_struct_variant_correct() {
    let source = r#"
        pub enum Container<T> {
            Named { value: T },
        }
        pub fn main() -> Int {
            let c = Container::Named::<Int> { value: 42 };
            match c {
                Container::Named { value } => value,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
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
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("duplicate binding"))
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
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("duplicate binding"))
    );
}

// ===== Tuple Index Tests =====

#[test]
fn test_run_tuple_index() {
    let source = r#"
        pub fn main() -> Int {
            (1, "hello").0
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_run_tuple_index_second() {
    let source = r#"
        pub fn main() -> String {
            (1, "hello").1
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("hello".to_string()));
}

#[test]
fn test_run_tuple_struct_index() {
    let source = r#"
        struct Pair(Int, String)
        pub fn main() -> Int {
            let p = Pair(42, "hi");
            p.0
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_run_tuple_index_chained() {
    let source = r#"
        pub fn main() -> Int {
            ((1, 2), (3, 4)).0.1
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(2));
}

#[test]
fn test_run_tuple_index_in_expression() {
    let source = r#"
        pub fn main() -> Int {
            let t = (10, 20);
            t.0 + t.1
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(30));
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
fn test_run_assert_true_succeeds() {
    let source = r#"
        pub fn main() -> Int {
            let _ = assert(true);
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

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
fn test_run_assert_eq_same_values_succeeds() {
    let source = r#"
        pub fn main() -> Int {
            let _ = assert_eq(1, 1);
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
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
fn test_run_assert_ne_different_values_succeeds() {
    let source = r#"
        pub fn main() -> Int {
            let _ = assert_ne(1, 2);
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
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
fn test_run_assert_eq_deep_equality_lists() {
    let source = r#"
        pub fn main() -> Int {
            let _ = assert_eq([1, 2], [1, 2]);
            42
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
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
// Entry Point Tests
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
        .entry(QualifiedPath::root().child("answer"))
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
        .entry(QualifiedPath::root().child("utils").child("compute"))
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
    let result = Runner::new().package(&checked, []).entry(path).run();
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
fn test_entry_error_on_function_with_parameters() {
    let source = r#"
        pub fn add(x: Int, y: Int) -> Int { x + y }
        pub fn main() -> Int { 0 }
    "#;
    let mem_source = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem_source, zoya_loader::Mode::Dev).unwrap();
    let checked = check(&package, &[]).unwrap();
    let result = Runner::new()
        .package(&checked, [])
        .entry(QualifiedPath::root().child("add"))
        .run();
    assert!(
        matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("must not take any parameters"))
    );
}

// ── Runner::test() integration tests ────────────────────────────────

#[test]
fn test_test_path_all_pass() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.zy");
    std::fs::write(
        &file,
        r#"
        #[test]
        fn test_one() -> () { () }

        #[test]
        fn test_two() -> () { () }
        "#,
    )
    .unwrap();

    let report = zoya_run::Runner::new().test(&file).unwrap().run().unwrap();
    assert_eq!(report.total(), 2);
    assert_eq!(report.passed(), 2);
    assert_eq!(report.failed(), 0);
    assert!(report.is_success());
}

#[test]
fn test_test_path_mix_pass_fail() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.zy");
    std::fs::write(
        &file,
        r#"
        #[test]
        fn test_ok() -> () { () }

        #[test]
        fn test_panic() -> () { panic("boom") }
        "#,
    )
    .unwrap();

    let report = zoya_run::Runner::new().test(&file).unwrap().run().unwrap();
    assert_eq!(report.total(), 2);
    assert_eq!(report.passed(), 1);
    assert_eq!(report.failed(), 1);
    assert!(!report.is_success());
}

#[test]
fn test_test_path_no_tests() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.zy");
    std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

    let report = zoya_run::Runner::new().test(&file).unwrap().run().unwrap();
    assert_eq!(report.total(), 0);
    assert!(report.is_success());
}

#[test]
fn test_test_path_result_err_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.zy");
    std::fs::write(
        &file,
        r#"
        #[test]
        fn test_err() -> Result<(), String> { Err("something wrong") }
        "#,
    )
    .unwrap();

    let report = zoya_run::Runner::new().test(&file).unwrap().run().unwrap();
    assert_eq!(report.total(), 1);
    assert_eq!(report.failed(), 1);
    assert!(!report.is_success());
    assert!(
        report.results[0]
            .outcome
            .as_ref()
            .unwrap_err()
            .contains("something wrong")
    );
}

#[test]
fn test_test_path_result_ok_passes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.zy");
    std::fs::write(
        &file,
        r#"
        #[test]
        fn test_ok() -> Result<(), String> { Ok(()) }
        "#,
    )
    .unwrap();

    let report = zoya_run::Runner::new().test(&file).unwrap().run().unwrap();
    assert_eq!(report.total(), 1);
    assert_eq!(report.passed(), 1);
    assert!(report.is_success());
}

// ===== List Spread Tests =====

#[test]
fn test_list_spread_copy() {
    let source = r#"
        pub fn main() -> List<Int> {
            let xs = [1, 2, 3];
            [..xs]
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
}

#[test]
fn test_list_spread_with_items() {
    let source = r#"
        pub fn main() -> List<Int> {
            let xs = [2, 3];
            [1, ..xs, 4]
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::List(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4)
        ])
    );
}

#[test]
fn test_list_spread_multiple() {
    let source = r#"
        pub fn main() -> List<Int> {
            let a = [1, 2];
            let b = [3, 4];
            [..a, ..b]
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::List(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4)
        ])
    );
}

#[test]
fn test_list_spread_empty() {
    let source = r#"
        pub fn main() -> List<Int> {
            let xs: List<Int> = [];
            [0, ..xs, 99]
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::List(vec![Value::Int(0), Value::Int(99)]));
}

#[test]
fn test_list_spread_with_function_call() {
    let source = r#"
        fn double(list: List<Int>) -> List<Int> {
            match list {
                [] => [],
                [head, rest @ ..] => [head * 2, ..double(rest)],
            }
        }
        pub fn main() -> List<Int> {
            double([1, 2, 3])
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(
        result,
        Value::List(vec![Value::Int(2), Value::Int(4), Value::Int(6)])
    );
}

// ===== Modulo operator tests =====

#[test]
fn test_modulo_int() {
    let source = "pub fn main() -> Int { 10 % 3 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_modulo_int_negative() {
    let source = "pub fn main() -> Int { -7 % 3 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(-1));
}

#[test]
fn test_modulo_float() {
    let source = "pub fn main() -> Float { 10.5 % 3.0 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Float(1.5));
}

#[test]
fn test_modulo_bigint() {
    let source = "pub fn main() -> BigInt { 10n % 3n }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::BigInt(1));
}

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
fn test_power_int() {
    let source = "pub fn main() -> Int { 2 ** 10 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1024));
}

#[test]
fn test_power_float() {
    let source = "pub fn main() -> Float { 2.0 ** 3.0 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Float(8.0));
}

#[test]
fn test_power_bigint() {
    let source = "pub fn main() -> BigInt { 2n ** 10n }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::BigInt(1024));
}

#[test]
fn test_power_right_associative() {
    // 2 ** 3 ** 2 = 2 ** (3 ** 2) = 2 ** 9 = 512
    let source = "pub fn main() -> Int { 2 ** 3 ** 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(512));
}

#[test]
fn test_power_precedence() {
    // 2 * 3 ** 2 = 2 * (3 ** 2) = 2 * 9 = 18
    let source = "pub fn main() -> Int { 2 * 3 ** 2 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(18));
}

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

#[test]
fn test_power_zero_exponent() {
    let source = "pub fn main() -> Int { 5 ** 0 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_modulo_precedence_same_as_mul() {
    // 2 + 10 % 3 = 2 + (10 % 3) = 2 + 1 = 3
    let source = "pub fn main() -> Int { 2 + 10 % 3 }";
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(3));
}

// ── Impl block tests ──────────────────────────────────────────────────

#[test]
fn test_impl_method_basic() {
    let source = r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn sum(self) -> Int {
                self.x + self.y
            }
        }
        pub fn main() -> Int {
            let p = Point { x: 3, y: 4 };
            p.sum()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_impl_associated_function() {
    let source = r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn origin() -> Self {
                Point { x: 0, y: 0 }
            }
        }
        pub fn main() -> Int {
            let p = Point::origin();
            p.x + p.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_impl_method_with_args() {
    let source = r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn add(self, other: Point) -> Point {
                Point { x: self.x + other.x, y: self.y + other.y }
            }
        }
        pub fn main() -> Int {
            let a = Point { x: 1, y: 2 };
            let b = Point { x: 3, y: 4 };
            let c = a.add(b);
            c.x + c.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_impl_generic() {
    let source = r#"
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
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_impl_generic_associated_function() {
    let source = r#"
        struct Wrapper<T> { value: T }
        impl<T> Wrapper<T> {
            fn new(v: T) -> Self {
                Wrapper { value: v }
            }
        }
        pub fn main() -> Int {
            let w = Wrapper::new(99);
            w.value
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(99));
}

#[test]
fn test_impl_on_enum() {
    let source = r#"
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
            let c = Shape::Circle(5);
            let s = Shape::Square(4);
            c.area() + s.area()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(91)); // 75 + 16
}

#[test]
fn test_impl_enum_associated_function() {
    let source = r#"
        enum Color {
            Red,
            Green,
            Blue,
        }
        impl Color {
            fn default() -> Self {
                Color::Red
            }
        }
        pub fn main() -> Bool {
            let c = Color::default();
            match c {
                Color::Red => true,
                _ => false,
            }
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_impl_multiple_blocks() {
    let source = r#"
        struct Counter { value: Int }
        impl Counter {
            fn new() -> Self {
                Counter { value: 0 }
            }
        }
        impl Counter {
            fn get(self) -> Int {
                self.value
            }
        }
        pub fn main() -> Int {
            let c = Counter::new();
            c.get()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn test_impl_self_type_in_return() {
    let source = r#"
        struct Point { x: Int, y: Int }
        impl Point {
            fn mirror(self) -> Self {
                Point { x: self.y, y: self.x }
            }
        }
        pub fn main() -> Int {
            let p = Point { x: 1, y: 2 };
            let m = p.mirror();
            m.x * 10 + m.y
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(21)); // 2*10 + 1
}

#[test]
fn test_impl_method_chaining() {
    let source = r#"
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
            Builder::new().add(10).add(20).add(12).build()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_impl_pub_method() {
    let source = r#"
        struct Foo { x: Int }
        impl Foo {
            pub fn get(self) -> Int {
                self.x
            }
        }
        pub fn main() -> Int {
            let f = Foo { x: 7 };
            f.get()
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_impl_method_with_let_block() {
    let source = r#"
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
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::Int(25)); // (4-1)^2 + (6-2)^2 = 9 + 16
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
fn test_run_interpolated_string_simple() {
    let source = r#"
        pub fn main() -> String {
            let name = "world";
            $"hello {name}!"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("hello world!".to_string()));
}

#[test]
fn test_run_interpolated_string_int() {
    let source = r#"
        pub fn main() -> String {
            let x = 42;
            $"value: {x}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("value: 42".to_string()));
}

#[test]
fn test_run_interpolated_string_float() {
    let source = r#"
        pub fn main() -> String {
            let x = 3.14;
            $"pi is {x}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("pi is 3.14".to_string()));
}

#[test]
fn test_run_interpolated_string_bigint() {
    let source = r#"
        pub fn main() -> String {
            let x = 42n;
            $"big: {x}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("big: 42".to_string()));
}

#[test]
fn test_run_interpolated_string_expression() {
    let source = r#"
        pub fn main() -> String {
            let x = 1;
            let y = 2;
            $"sum: {x + y}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("sum: 3".to_string()));
}

#[test]
fn test_run_interpolated_string_method_call() {
    let source = r#"
        pub fn main() -> String {
            let name = "hello";
            $"upper: {name.to_uppercase()}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("upper: HELLO".to_string()));
}

#[test]
fn test_run_interpolated_string_literal_only() {
    let source = r#"
        pub fn main() -> String {
            $"plain text"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("plain text".to_string()));
}

#[test]
fn test_run_interpolated_string_adjacent_expressions() {
    let source = r#"
        pub fn main() -> String {
            let a = "hello";
            let b = "world";
            $"{a}{b}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("helloworld".to_string()));
}

#[test]
fn test_run_interpolated_string_escaped_braces() {
    let source = r#"
        pub fn main() -> String {
            $"literal \{ brace \}"
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("literal { brace }".to_string()));
}

#[test]
fn test_run_interpolated_string_empty() {
    let source = r#"
        pub fn main() -> String {
            $""
        }
    "#;
    let result = run_source(source).unwrap();
    assert_eq!(result, Value::String("".to_string()));
}

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

    // tasks() should return the task function
    let tasks = checked.tasks();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0], QualifiedPath::root().child("my_task"));

    // The function should run normally
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
