use zoya_check::check;
use crate::eval::{self, EvalError, Value};
use zoya_codegen::codegen;
use zoya_ir::CheckedItem;
use zoya_loader::{load_modules_with, MemorySource};

/// Run Zoya source code and return the result
pub fn run(source: &str) -> Result<Value, EvalError> {
    // Load module using memory source
    let mem_source = MemorySource::new().with_module("root", source);
    let tree = load_modules_with(&mem_source, &"root".to_string())
        .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Type check module tree
    let checked_tree =
        check(&tree).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Find main in root module
    let root_module = checked_tree
        .root()
        .ok_or_else(|| EvalError::RuntimeError("root module not found".to_string()))?;

    let main_func = root_module
        .items
        .iter()
        .find_map(|item| match item {
            CheckedItem::Function(f) if f.name == "main" => Some(f.as_ref()),
            _ => None,
        })
        .ok_or_else(|| EvalError::RuntimeError("no main() function found".to_string()))?;

    if !main_func.params.is_empty() {
        return Err(EvalError::RuntimeError(
            "main() must not take any parameters".to_string(),
        ));
    }

    // Generate JS code
    let mut js_code = codegen(&checked_tree);
    js_code.push_str("$root$main()");

    // Execute
    let (_runtime, context) =
        eval::create_context().map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    context.with(|ctx| eval::eval(&ctx, js_code, main_func.return_type.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_simple_main() {
        let source = "fn main() -> Int { 42 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_main_with_expression() {
        let source = "fn main() -> Int { 1 + 2 * 3 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_run_main_calling_function() {
        let source = r#"
            fn add(x: Int, y: Int) -> Int { x + y }
            fn main() -> Int { add(10, 20) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_run_main_with_float() {
        let source = "fn main() -> Float { 3.14 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_run_no_main_error() {
        let source = "fn foo() -> Int { 42 }";
        let result = run(source);
        assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("no main()")));
    }

    #[test]
    fn test_run_main_with_params_error() {
        let source = "fn main(x: Int) -> Int { x }";
        let result = run(source);
        assert!(
            matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("must not take any parameters"))
        );
    }

    #[test]
    fn test_run_multiple_functions() {
        let source = r#"
            fn square(x: Int) -> Int { x * x }
            fn double(x: Int) -> Int { x + x }
            fn main() -> Int { square(double(3)) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(36)); // double(3) = 6, square(6) = 36
    }

    #[test]
    fn test_run_function_no_braces() {
        // Functions with simple expression bodies can omit braces
        let source = r#"
            fn square(x: Int) -> Int x * x
            fn main() -> Int { square(5) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(25));
    }

    #[test]
    fn test_run_function_no_braces_multiple() {
        // Multiple functions without braces
        let source = r#"
            fn add(x: Int, y: Int) -> Int x + y
            fn double(x: Int) -> Int x * 2
            fn main() -> Int add(double(3), 4)
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(10)); // double(3) = 6, add(6, 4) = 10
    }

    #[test]
    fn test_run_division_by_zero() {
        let source = "fn main() -> Int { 1 / 0 }";
        let result = run(source);
        assert!(matches!(result, Err(EvalError::DivisionByZero)));
    }

    #[test]
    fn test_run_bool_true() {
        let source = "fn main() -> Bool { true }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_bool_false() {
        let source = "fn main() -> Bool { false }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_equality_true() {
        let source = "fn main() -> Bool { 1 == 1 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_equality_false() {
        let source = "fn main() -> Bool { 1 == 2 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_inequality() {
        let source = "fn main() -> Bool { 1 != 2 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_less_than() {
        let source = "fn main() -> Bool { 1 < 2 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_greater_than() {
        let source = "fn main() -> Bool { 2 > 1 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_less_equal() {
        let source = "fn main() -> Bool { 2 <= 2 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_greater_equal() {
        let source = "fn main() -> Bool { 2 >= 2 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_bool_equality() {
        let source = "fn main() -> Bool { true == false }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_float_comparison() {
        let source = "fn main() -> Bool { 1.5 < 2.5 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_comparison_with_arithmetic() {
        let source = "fn main() -> Bool { 1 + 2 == 3 }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_len() {
        let source = r#"fn main() -> Int { "hello".len() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_run_string_is_empty_false() {
        let source = r#"fn main() -> Bool { "hello".is_empty() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_string_is_empty_true() {
        let source = r#"fn main() -> Bool { "".is_empty() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_contains_true() {
        let source = r#"fn main() -> Bool { "hello world".contains("world") }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_contains_false() {
        let source = r#"fn main() -> Bool { "hello".contains("xyz") }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_string_starts_with() {
        let source = r#"fn main() -> Bool { "hello".starts_with("he") }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_ends_with() {
        let source = r#"fn main() -> Bool { "hello".ends_with("lo") }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_to_uppercase() {
        let source = r#"fn main() -> String { "hello".to_uppercase() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_run_string_to_lowercase() {
        let source = r#"fn main() -> String { "HELLO".to_lowercase() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_run_string_trim() {
        let source = r#"fn main() -> String { "  hello  ".trim() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_run_chained_method_calls() {
        let source = r#"fn main() -> Int { "hello".to_uppercase().len() }"#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_run_method_call_in_function() {
        let source = r#"
            fn get_length(s: String) -> Int { s.len() }
            fn main() -> Int { get_length("hello") }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    // Int method tests
    #[test]
    fn test_run_int32_abs() {
        let source = "fn main() -> Int { (-5).abs() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_run_int32_to_string() {
        let source = "fn main() -> String { 42.to_string() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("42".to_string()));
    }

    #[test]
    fn test_run_int32_to_float() {
        let source = "fn main() -> Float { 42.to_float() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(42.0));
    }

    #[test]
    fn test_run_int32_min() {
        let source = "fn main() -> Int { 3.min(5) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_run_int32_max() {
        let source = "fn main() -> Int { 3.max(5) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    // BigInt literal tests
    #[test]
    fn test_run_int64_literal() {
        let source = "fn main() -> BigInt { 42n }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::BigInt(42));
    }

    #[test]
    fn test_run_int64_large_literal() {
        let source = "fn main() -> BigInt { 9_000_000_000n }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::BigInt(9_000_000_000));
    }

    #[test]
    fn test_run_int64_addition() {
        let source = "fn main() -> BigInt { 1n + 2n }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::BigInt(3));
    }

    #[test]
    fn test_run_int64_method_abs() {
        let source = "fn main() -> BigInt { (-42n).abs() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::BigInt(42));
    }

    #[test]
    fn test_run_int64_method_to_string() {
        let source = "fn main() -> String { 42n.to_string() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("42".to_string()));
    }

    // Float method tests
    #[test]
    fn test_run_float_abs() {
        let source = "fn main() -> Float { (-3.14).abs() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_run_float_to_string() {
        let source = "fn main() -> String { 3.14.to_string() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("3.14".to_string()));
    }

    #[test]
    fn test_run_float_to_int() {
        let source = "fn main() -> Int { 3.7.to_int() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_run_float_floor() {
        let source = "fn main() -> Float { 3.7.floor() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(3.0));
    }

    #[test]
    fn test_run_float_ceil() {
        let source = "fn main() -> Float { 3.2.ceil() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_run_float_round() {
        let source = "fn main() -> Float { 3.5.round() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_run_float_sqrt() {
        let source = "fn main() -> Float { 9.0.sqrt() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(3.0));
    }

    #[test]
    fn test_run_float_min() {
        let source = "fn main() -> Float { 3.5.min(2.5) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(2.5));
    }

    #[test]
    fn test_run_float_max() {
        let source = "fn main() -> Float { 3.5.max(2.5) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Float(3.5));
    }

    // List tests
    #[test]
    fn test_run_list_literal() {
        let source = "fn main() -> List<Int> { [1, 2, 3] }";
        let result = run(source).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn test_run_empty_list() {
        let source = "fn main() -> List<Int> { [] }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_run_list_equality_true() {
        let source = "fn main() -> Bool { [1, 2, 3] == [1, 2, 3] }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_equality_false_different_elements() {
        let source = "fn main() -> Bool { [1, 2, 3] == [1, 2, 4] }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_equality_false_different_length() {
        let source = "fn main() -> Bool { [1, 2] == [1, 2, 3] }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_inequality() {
        let source = "fn main() -> Bool { [1, 2] != [1, 3] }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_empty_list_equality() {
        let source = "fn main() -> Bool { [] == [] }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_tuple_equality_true() {
        let source = "fn main() -> Bool { (1, 2) == (1, 2) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_tuple_equality_false() {
        let source = "fn main() -> Bool { (1, 2) == (1, 3) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_tuple_inequality() {
        let source = "fn main() -> Bool { (1, 2) != (1, 3) }";
        let result = run(source).unwrap();
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
            fn main() -> Bool { is_empty([]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { is_empty([1, 2, 3]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { head_or_zero([42, 1, 2]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { head_or_zero([]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { sum_pair([10, 20]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { sum_pair([1, 2, 3]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { starts_with_one([1, 2, 3]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { starts_with_one([2, 3, 4]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { bad([1]) }
        "#;
        let result = run(source);
        assert!(matches!(
            result,
            Err(EvalError::RuntimeError(msg)) if msg.contains("non-exhaustive")
        ));
    }

    #[test]
    fn test_run_list_string() {
        let source = r#"fn main() -> List<String> { ["hello", "world"] }"#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { len_check([1, 2]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { last_elem([1, 2, 3]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { last_elem([42]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { last_two([1, 2, 3, 4]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { ends_with_zero([1, 2, 0]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { first_and_last([1, 2, 3, 4]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { first_and_last([10, 20]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { bookended_by_ones([1, 2, 3, 1]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { middle_free([1, 2, 3, 4, 5, 6]) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(14)); // 1 + 2 + 5 + 6
    }

    // List method tests

    #[test]
    fn test_run_list_len() {
        let source = "fn main() -> Int { [1, 2, 3].len() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_run_list_len_empty() {
        let source = "fn main() -> Int { [].len() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_run_list_is_empty_true() {
        let source = "fn main() -> Bool { [].is_empty() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_is_empty_false() {
        let source = "fn main() -> Bool { [1, 2].is_empty() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_reverse() {
        let source = "fn main() -> List<Int> { [1, 2, 3].reverse() }";
        let result = run(source).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(3), Value::Int(2), Value::Int(1)])
        );
    }

    #[test]
    fn test_run_list_reverse_empty() {
        let source = "fn main() -> List<Int> { [].reverse() }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_run_list_push() {
        let source = "fn main() -> List<Int> { [1, 2].push(3) }";
        let result = run(source).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn test_run_list_push_empty() {
        let source = "fn main() -> List<Int> { [].push(1) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1)]));
    }

    #[test]
    fn test_run_list_concat() {
        let source = "fn main() -> List<Int> { [1, 2].concat([3, 4]) }";
        let result = run(source).unwrap();
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
    fn test_run_list_concat_empty() {
        let source = "fn main() -> List<Int> { [1, 2].concat([]) }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn test_run_list_chained_methods() {
        let source = "fn main() -> List<Int> { [1, 2].push(3).reverse() }";
        let result = run(source).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(3), Value::Int(2), Value::Int(1)])
        );
    }

    // Tuple tests
    #[test]
    fn test_run_tuple_literal() {
        let source = r#"fn main() -> (Int, String) { (42, "hello") }"#;
        let result = run(source).unwrap();
        assert_eq!(
            result,
            Value::Tuple(vec![Value::Int(42), Value::String("hello".to_string())])
        );
    }

    #[test]
    fn test_run_empty_tuple() {
        let source = "fn main() -> () { () }";
        let result = run(source).unwrap();
        assert_eq!(result, Value::Tuple(vec![]));
    }

    #[test]
    fn test_run_single_element_tuple() {
        let source = "fn main() -> (Int,) { (42,) }";
        let result = run(source).unwrap();
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
            fn main() -> Int { first((10, "hello")) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { get_first((1, 2, 3)) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { get_last((1, 2, 3)) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { first_and_last((1, 2, 3)) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { get_int((42, "hello", true)) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_tuple_with_list() {
        let source = r#"
            fn main() -> (Int, List<Int>) { (1, [2, 3]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int {
                match 1 { 0 => 0, 1 => 10, _ => 100 }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn test_run_match_braced_simple() {
        let source = r#"
            fn main() -> Int {
                match 1 { 0 => { 0 }, 1 => { 10 }, _ => { 100 } }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn test_run_match_braced_block() {
        let source = r#"
            fn main() -> Int {
                match 5 {
                    n => {
                        let doubled = n * 2;
                        doubled + 1
                    }
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(11)); // 5 * 2 + 1
    }

    #[test]
    fn test_run_match_block_multiple_bindings() {
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(14)); // ((3 * 2) + 1) * 2
    }

    #[test]
    fn test_run_match_block_pattern_binding_visible() {
        let source = r#"
            fn main() -> Int {
                match 10 {
                    x => {
                        let y = x + 5;
                        x + y
                    }
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(25)); // 10 + 15
    }

    #[test]
    fn test_run_match_mixed_arms() {
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
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
            fn main() -> Int { sum_first_two([5, 7, 9]) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { process((3, 4)) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(19)); // (3 + 4) + (3 * 4) = 7 + 12
    }

    // Forward reference and mutual recursion tests
    #[test]
    fn test_run_forward_reference() {
        // caller is defined before callee but calls it
        let source = r#"
            fn caller() -> Int { callee() }
            fn callee() -> Int { 42 }
            fn main() -> Int { caller() }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { is_even(4) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Bool { is_odd(3) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // Lambda tests
    #[test]
    fn test_run_simple_lambda() {
        let source = r#"
            fn main() -> Int {
                let f = |x| x + 1;
                f(41)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_multi_param() {
        let source = r#"
            fn main() -> Int {
                let add = |x, y| x + y;
                add(10, 32)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_with_type_annotation() {
        let source = r#"
            fn main() -> Int {
                let f = |x: Int| -> Int x * 2;
                f(21)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_block_body() {
        let source = r#"
            fn main() -> Int {
                let f = |x| {
                    let y = x * 2;
                    y + 1
                };
                f(20)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(41));
    }

    #[test]
    fn test_run_lambda_nested() {
        let source = r#"
            fn main() -> Int {
                let add = |x| |y| x + y;
                let add10 = add(10);
                add10(32)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_polymorphic_identity() {
        // Test let polymorphism: id can be used at different types
        let source = r#"
            fn main() -> Int {
                let id = |x| x;
                let a = id(42);
                let b = id(true);
                a
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_polymorphic_const() {
        // Test polymorphic const function
        let source = r#"
            fn main() -> Int {
                let const_ = |x| |y| x;
                let always42 = const_(42);
                always42(true)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_no_params() {
        let source = r#"
            fn main() -> Int {
                let f = || 42;
                f()
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_captures_outer_var() {
        // Lambda captures variable from outer scope
        let source = r#"
            fn main() -> Int {
                let x = 10;
                let f = |y| x + y;
                f(32)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_with_function_type_annotation() {
        let source = r#"
            fn main() -> Int {
                let f: Int -> Int = |x| x * 2;
                f(21)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_multi_param_function_type() {
        let source = r#"
            fn main() -> Int {
                let add: (Int, Int) -> Int = |x, y| x + y;
                add(10, 32)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_no_param_function_type() {
        let source = r#"
            fn main() -> Int {
                let f: () -> Int = || 42;
                f()
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_lambda_passed_to_function() {
        let source = r#"
            fn apply(f: Int -> Int, x: Int) -> Int f(x)

            fn main() -> Int {
                apply(|x| x * 2, 21)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_higher_order_function_returns_lambda() {
        let source = r#"
            fn make_adder(n: Int) -> Int -> Int |x| x + n

            fn main() -> Int {
                let add5 = make_adder(5);
                add5(37)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_function_type_mismatch_error() {
        let source = r#"
            fn main() -> Int {
                let f: Int -> Int = |x| true;
                0
            }
        "#;
        let result = run(source);
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
            fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                p.x + p.y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_run_struct_field_access() {
        let source = r#"
            struct Person { name: String, age: Int }
            fn main() -> String {
                let p = Person { name: "Alice", age: 30 };
                p.name
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::String("Alice".to_string()));
    }

    #[test]
    fn test_run_struct_empty() {
        let source = r#"
            struct Empty {}
            fn main() -> Int {
                let e = Empty {};
                42
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_struct_generic() {
        let source = r#"
            struct Pair<T, U> { first: T, second: U }
            fn main() -> Int {
                let p = Pair { first: 1, second: 2 };
                p.first + p.second
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_run_struct_match_exact() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                match p {
                    Point { x, y } => x + y,
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_run_struct_match_partial() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                match p {
                    Point { x, .. } => x,
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn test_run_struct_match_with_binding() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                match p {
                    Point { x: a, y: b } => a * b,
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(200));
    }

    #[test]
    fn test_run_struct_nested() {
        let source = r#"
            struct Point { x: Int, y: Int }
            struct Line { start: Point, end: Point }
            fn main() -> Int {
                let l = Line {
                    start: Point { x: 0, y: 0 },
                    end: Point { x: 10, y: 20 }
                };
                l.end.x + l.end.y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_run_struct_field_shorthand() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Int {
                let x = 10;
                let y = 20;
                let p = Point { x, y };
                p.x + p.y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_run_struct_equality() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Bool {
                let p1 = Point { x: 10, y: 20 };
                let p2 = Point { x: 10, y: 20 };
                p1 == p2
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_struct_inequality() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Bool {
                let p1 = Point { x: 10, y: 20 };
                let p2 = Point { x: 10, y: 30 };
                p1 != p2
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // Enum tests

    #[test]
    fn test_run_enum_unit_variant() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
                let x = Option::None;
                match x {
                    Option::None => 0,
                    Option::Some(v) => v
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_run_enum_tuple_variant() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
                let x = Option::Some(42);
                match x {
                    Option::None => 0,
                    Option::Some(v) => v
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_enum_struct_variant() {
        let source = r#"
            enum Message { Quit, Move { x: Int, y: Int }, Write(String) }
            fn main() -> Int {
                let msg = Message::Move { x: 10, y: 20 };
                match msg {
                    Message::Quit => 0,
                    Message::Move { x, y } => x + y,
                    Message::Write(s) => s.len()
                }
            }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int {
                handle_quit() + handle_write()
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(6)); // 1 + 5
    }

    #[test]
    fn test_run_enum_generic_multiple_types() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(15)); // 10 + 5
    }

    #[test]
    fn test_run_enum_nested_pattern() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
                let x = Option::Some((1, 2));
                match x {
                    Option::None => 0,
                    Option::Some((a, b)) => a + b
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_run_enum_partial_struct_pattern() {
        let source = r#"
            enum Message { Quit, Move { x: Int, y: Int, z: Int } }
            fn main() -> Int {
                let msg = Message::Move { x: 1, y: 2, z: 3 };
                match msg {
                    Message::Quit => 0,
                    Message::Move { x, .. } => x
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_run_enum_wildcard_pattern() {
        let source = r#"
            enum Message { Quit, Move { x: Int, y: Int }, Write(String) }
            fn main() -> Int {
                let msg = Message::Write("hello");
                match msg {
                    Message::Quit => 0,
                    _ => 42
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_run_enum_equality() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Bool {
                let x = Option::Some(42);
                let y = Option::Some(42);
                x == y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_enum_inequality() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Bool {
                let x = Option::Some(42);
                let y = Option::None;
                x != y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_enum_multi_field_tuple() {
        let source = r#"
            enum Result<T, E> { Ok(T), Err(E) }
            fn main() -> Int {
                let x = Result::Ok(42);
                match x {
                    Result::Ok(v) => v,
                    Result::Err(e) => e
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    // Turbofish syntax tests

    #[test]
    fn test_turbofish_unit_variant() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
                let x = Option::None::<Int>;
                match x {
                    Option::None => 0,
                    Option::Some(v) => v
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_turbofish_tuple_variant() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
                let x = Option::Some::<Int>(42);
                match x {
                    Option::None => 0,
                    Option::Some(v) => v
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_turbofish_function_call() {
        let source = r#"
            fn identity<T>(x: T) -> T x
            fn main() -> Int {
                identity::<Int>(42)
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_turbofish_multiple_type_args() {
        let source = r#"
            enum Result<T, E> { Ok(T), Err(E) }
            fn main() -> Int {
                let x = Result::Ok::<Int, String>(42);
                match x {
                    Result::Ok(v) => v,
                    Result::Err(_) => 0
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    // As pattern (@) tests
    #[test]
    fn test_as_pattern_literal() {
        let source = r#"
            fn main() -> Int {
                let x = 42;
                match x {
                    n @ 42 => n,
                    _ => 0
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_as_pattern_with_enum() {
        let source = r#"
            enum Option<T> { None, Some(T) }
            fn main() -> Int {
                let opt = Option::Some(10);
                match opt {
                    whole @ Option::Some(x) => x,
                    Option::None => 0
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn test_list_rest_binding() {
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn test_list_rest_binding_suffix() {
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_list_rest_binding_middle() {
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn test_tuple_rest_binding() {
        // rest @ .. on (1, 2, 3) with (first, rest @ ..) gives rest: (Int, Int)
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn test_tuple_rest_binding_suffix() {
        // rest @ .. on (1, 2, 3, 4) with (rest @ .., last) gives rest: (Int, Int, Int)
        let source = r#"
            fn main() -> Int {
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
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    // ===== Let Pattern Destructuring Tests =====

    #[test]
    fn test_let_tuple_destructuring() {
        let source = r#"
            fn main() -> Int {
                let (a, b) = (1, 2);
                a + b
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_let_tuple_nested_destructuring() {
        let source = r#"
            fn main() -> Int {
                let (a, (b, c)) = (1, (2, 3));
                a + b + c
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(6));
    }

    #[test]
    fn test_let_struct_destructuring() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn main() -> Int {
                let p = Point { x: 10, y: 20 };
                let Point { x, y } = p;
                x + y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_let_struct_partial_destructuring() {
        let source = r#"
            struct Point { x: Int, y: Int, z: Int }
            fn main() -> Int {
                let p = Point { x: 1, y: 2, z: 3 };
                let Point { x, .. } = p;
                x
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_let_tuple_rest_prefix() {
        let source = r#"
            fn main() -> Int {
                let (first, ..) = (1, 2, 3, 4);
                first
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_let_tuple_rest_suffix() {
        let source = r#"
            fn main() -> Int {
                let (.., last) = (1, 2, 3, 4);
                last
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(4));
    }

    #[test]
    fn test_let_wildcard() {
        let source = r#"
            fn main() -> Int {
                let _ = 42;
                0
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_let_as_pattern() {
        let source = r#"
            fn main() -> Int {
                let pair @ (a, b) = (1, 2);
                match pair {
                    (x, y) => a + b + x + y,
                }
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(6));
    }

    #[test]
    fn test_let_empty_tuple() {
        let source = r#"
            fn main() -> Int {
                let () = ();
                42
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_function_tuple_param_destructuring() {
        let source = r#"
            fn swap((a, b): (Int, Int)) -> (Int, Int) (b, a)
            fn main() -> Int {
                let (x, y) = swap((1, 2));
                x * 10 + y
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(21)); // x=2, y=1, so 2*10+1=21
    }

    #[test]
    fn test_function_nested_tuple_param() {
        let source = r#"
            fn nested(((a, b), c): ((Int, Int), Int)) -> Int a + b + c
            fn main() -> Int { nested(((1, 2), 3)) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(6));
    }

    #[test]
    fn test_function_struct_param_destructuring() {
        let source = r#"
            struct Point { x: Int, y: Int }
            fn get_x(Point { x, .. }: Point) -> Int x
            fn main() -> Int { get_x(Point { x: 42, y: 0 }) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_lambda_tuple_param_destructuring() {
        let source = r#"
            fn main() -> Int {
                let add = |(a, b): (Int, Int)| a + b;
                add((3, 4))
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_lambda_tuple_param_type_inference() {
        let source = r#"
            fn main() -> Int {
                let add = |(a, b)| a + b;
                add((3, 4))
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_function_wildcard_param() {
        let source = r#"
            fn first((a, _): (Int, Int)) -> Int a
            fn main() -> Int { first((5, 10)) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_function_as_pattern_param() {
        let source = r#"
            fn with_as(pair @ (a, b): (Int, Int)) -> Int {
                let (x, y) = pair;
                a + b + x + y
            }
            fn main() -> Int { with_as((1, 2)) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(6)); // 1+2+1+2=6
    }

    #[test]
    fn test_type_alias_simple() {
        let source = r#"
            type UserId = Int
            fn get_id() -> UserId { 42 }
            fn main() -> Int { get_id() }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_type_alias_generic() {
        let source = r#"
            type Pair<A, B> = (A, B)
            fn make_pair() -> Pair<Int, Bool> { (1, true) }
            fn main() -> Int {
                let (x, _) = make_pair();
                x
            }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_type_alias_function_type() {
        // Test that type alias works for function types as parameter types
        let source = r#"
            type IntOp = (Int) -> Int
            fn apply(f: IntOp, x: Int) -> Int { f(x) }
            fn main() -> Int { apply(|x| x * 2, 21) }
        "#;
        let result = run(source).unwrap();
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
            fn main() -> Int { sum([1, 2, 3, 4]) }
        "#;
        let result = run(source).unwrap();
        assert_eq!(result, Value::Int(10));
    }
}
