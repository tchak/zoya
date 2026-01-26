use std::path::Path;

use crate::check::check_file;
use crate::codegen::{codegen_function, deep_eq_prelude};
use crate::eval::{self, EvalError, Value};
use crate::lexer;
use crate::parser;

/// Run a Zoya source file and print the result
pub fn run(path: &Path) -> Result<(), EvalError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| EvalError::RuntimeError(format!("failed to read file: {}", e)))?;

    let value = run_source(&source)?;
    println!("{}", value);
    Ok(())
}

/// Run Zoya source code and return the result
fn run_source(source: &str) -> Result<Value, EvalError> {
    // Lex and parse all items
    let tokens = lexer::lex(source).map_err(|e| EvalError::RuntimeError(e.message))?;
    let items = parser::parse_file(tokens).map_err(|e| EvalError::RuntimeError(e.message))?;

    // Type-check all items
    let typed_functions = check_file(&items).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Find main function
    let main_func = typed_functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or_else(|| EvalError::RuntimeError("no main() function found".to_string()))?;

    // Check main has no parameters
    if !main_func.params.is_empty() {
        return Err(EvalError::RuntimeError(
            "main() must not take any parameters".to_string(),
        ));
    }

    // Generate JS code
    let mut js_code = String::new();

    // Add prelude for deep equality (used by list comparison)
    js_code.push_str(deep_eq_prelude());
    js_code.push('\n');

    for typed_func in &typed_functions {
        js_code.push_str(&codegen_function(typed_func));
        js_code.push('\n');
    }
    js_code.push_str("main()");

    // Execute
    let (_runtime, context) =
        eval::create_context().map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    context.with(|ctx| eval::eval_js_in_context(&ctx, js_code, main_func.return_type.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_simple_main() {
        let source = "fn main() -> Int32 { 42 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(42));
    }

    #[test]
    fn test_run_main_with_expression() {
        let source = "fn main() -> Int32 { 1 + 2 * 3 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(7));
    }

    #[test]
    fn test_run_main_calling_function() {
        let source = r#"
            fn add(x: Int32, y: Int32) -> Int32 { x + y }
            fn main() -> Int32 { add(10, 20) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(30));
    }

    #[test]
    fn test_run_main_with_float() {
        let source = "fn main() -> Float { 3.14 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_run_no_main_error() {
        let source = "fn foo() -> Int32 { 42 }";
        let result = run_source(source);
        assert!(matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("no main()")));
    }

    #[test]
    fn test_run_main_with_params_error() {
        let source = "fn main(x: Int32) -> Int32 { x }";
        let result = run_source(source);
        assert!(
            matches!(result, Err(EvalError::RuntimeError(msg)) if msg.contains("must not take any parameters"))
        );
    }

    #[test]
    fn test_run_multiple_functions() {
        let source = r#"
            fn square(x: Int32) -> Int32 { x * x }
            fn double(x: Int32) -> Int32 { x + x }
            fn main() -> Int32 { square(double(3)) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(36)); // double(3) = 6, square(6) = 36
    }

    #[test]
    fn test_run_division_by_zero() {
        let source = "fn main() -> Int32 { 1 / 0 }";
        let result = run_source(source);
        assert!(matches!(result, Err(EvalError::DivisionByZero)));
    }

    #[test]
    fn test_run_bool_true() {
        let source = "fn main() -> Bool { true }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_bool_false() {
        let source = "fn main() -> Bool { false }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_equality_true() {
        let source = "fn main() -> Bool { 1 == 1 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_equality_false() {
        let source = "fn main() -> Bool { 1 == 2 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_inequality() {
        let source = "fn main() -> Bool { 1 != 2 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_less_than() {
        let source = "fn main() -> Bool { 1 < 2 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_greater_than() {
        let source = "fn main() -> Bool { 2 > 1 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_less_equal() {
        let source = "fn main() -> Bool { 2 <= 2 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_greater_equal() {
        let source = "fn main() -> Bool { 2 >= 2 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_bool_equality() {
        let source = "fn main() -> Bool { true == false }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_float_comparison() {
        let source = "fn main() -> Bool { 1.5 < 2.5 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_comparison_with_arithmetic() {
        let source = "fn main() -> Bool { 1 + 2 == 3 }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_len() {
        let source = r#"fn main() -> Int32 { "hello".len() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(5));
    }

    #[test]
    fn test_run_string_is_empty_false() {
        let source = r#"fn main() -> Bool { "hello".is_empty() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_string_is_empty_true() {
        let source = r#"fn main() -> Bool { "".is_empty() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_contains_true() {
        let source = r#"fn main() -> Bool { "hello world".contains("world") }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_contains_false() {
        let source = r#"fn main() -> Bool { "hello".contains("xyz") }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_string_starts_with() {
        let source = r#"fn main() -> Bool { "hello".starts_with("he") }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_ends_with() {
        let source = r#"fn main() -> Bool { "hello".ends_with("lo") }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_string_to_uppercase() {
        let source = r#"fn main() -> String { "hello".to_uppercase() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_run_string_to_lowercase() {
        let source = r#"fn main() -> String { "HELLO".to_lowercase() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_run_string_trim() {
        let source = r#"fn main() -> String { "  hello  ".trim() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_run_chained_method_calls() {
        let source = r#"fn main() -> Int32 { "hello".to_uppercase().len() }"#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(5));
    }

    #[test]
    fn test_run_method_call_in_function() {
        let source = r#"
            fn get_length(s: String) -> Int32 { s.len() }
            fn main() -> Int32 { get_length("hello") }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(5));
    }

    // Int32 method tests
    #[test]
    fn test_run_int32_abs() {
        let source = "fn main() -> Int32 { (-5).abs() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(5));
    }

    #[test]
    fn test_run_int32_to_string() {
        let source = "fn main() -> String { 42.to_string() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::String("42".to_string()));
    }

    #[test]
    fn test_run_int32_to_float() {
        let source = "fn main() -> Float { 42.to_float() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(42.0));
    }

    #[test]
    fn test_run_int32_min() {
        let source = "fn main() -> Int32 { 3.min(5) }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(3));
    }

    #[test]
    fn test_run_int32_max() {
        let source = "fn main() -> Int32 { 3.max(5) }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(5));
    }

    // Float method tests
    #[test]
    fn test_run_float_abs() {
        let source = "fn main() -> Float { (-3.14).abs() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_run_float_to_string() {
        let source = "fn main() -> String { 3.14.to_string() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::String("3.14".to_string()));
    }

    #[test]
    fn test_run_float_to_int() {
        let source = "fn main() -> Int32 { 3.7.to_int() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(3));
    }

    #[test]
    fn test_run_float_floor() {
        let source = "fn main() -> Float { 3.7.floor() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(3.0));
    }

    #[test]
    fn test_run_float_ceil() {
        let source = "fn main() -> Float { 3.2.ceil() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_run_float_round() {
        let source = "fn main() -> Float { 3.5.round() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_run_float_sqrt() {
        let source = "fn main() -> Float { 9.0.sqrt() }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(3.0));
    }

    #[test]
    fn test_run_float_min() {
        let source = "fn main() -> Float { 3.5.min(2.5) }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(2.5));
    }

    #[test]
    fn test_run_float_max() {
        let source = "fn main() -> Float { 3.5.max(2.5) }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Float(3.5));
    }

    // List tests
    #[test]
    fn test_run_list_literal() {
        let source = "fn main() -> List<Int32> { [1, 2, 3] }";
        let result = run_source(source).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int32(1), Value::Int32(2), Value::Int32(3)])
        );
    }

    #[test]
    fn test_run_empty_list() {
        let source = "fn main() -> List<Int32> { [] }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_run_list_equality_true() {
        let source = "fn main() -> Bool { [1, 2, 3] == [1, 2, 3] }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_equality_false_different_elements() {
        let source = "fn main() -> Bool { [1, 2, 3] == [1, 2, 4] }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_equality_false_different_length() {
        let source = "fn main() -> Bool { [1, 2] == [1, 2, 3] }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_inequality() {
        let source = "fn main() -> Bool { [1, 2] != [1, 3] }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_empty_list_equality() {
        let source = "fn main() -> Bool { [] == [] }";
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_match_empty() {
        let source = r#"
            fn is_empty<T>(xs: List<T>) -> Bool {
                match xs {
                    [] => true
                    [_, ..] => false
                }
            }
            fn main() -> Bool { is_empty([]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_match_nonempty() {
        let source = r#"
            fn is_empty<T>(xs: List<T>) -> Bool {
                match xs {
                    [] => true
                    [_, ..] => false
                }
            }
            fn main() -> Bool { is_empty([1, 2, 3]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_match_head() {
        let source = r#"
            fn head_or_zero(xs: List<Int32>) -> Int32 {
                match xs {
                    [] => 0
                    [x, ..] => x
                }
            }
            fn main() -> Int32 { head_or_zero([42, 1, 2]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(42));
    }

    #[test]
    fn test_run_list_match_head_empty() {
        let source = r#"
            fn head_or_zero(xs: List<Int32>) -> Int32 {
                match xs {
                    [] => 0
                    [x, ..] => x
                }
            }
            fn main() -> Int32 { head_or_zero([]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(0));
    }

    #[test]
    fn test_run_list_match_exact() {
        let source = r#"
            fn sum_pair(xs: List<Int32>) -> Int32 {
                match xs {
                    [a, b] => a + b
                    _ => 0
                }
            }
            fn main() -> Int32 { sum_pair([10, 20]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(30));
    }

    #[test]
    fn test_run_list_match_exact_wrong_length() {
        let source = r#"
            fn sum_pair(xs: List<Int32>) -> Int32 {
                match xs {
                    [a, b] => a + b
                    _ => 0
                }
            }
            fn main() -> Int32 { sum_pair([1, 2, 3]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(0));
    }

    #[test]
    fn test_run_list_match_literal_pattern() {
        let source = r#"
            fn starts_with_one(xs: List<Int32>) -> Bool {
                match xs {
                    [1, ..] => true
                    [_, ..] => false
                    [] => false
                }
            }
            fn main() -> Bool { starts_with_one([1, 2, 3]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_match_literal_pattern_not_matching() {
        let source = r#"
            fn starts_with_one(xs: List<Int32>) -> Bool {
                match xs {
                    [1, ..] => true
                    [_, ..] => false
                    [] => false
                }
            }
            fn main() -> Bool { starts_with_one([2, 3, 4]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_run_list_exhaustiveness_error() {
        // Missing empty list pattern should cause compile error
        let source = r#"
            fn bad(xs: List<Int32>) -> Int32 {
                match xs {
                    [x, ..] => x
                }
            }
            fn main() -> Int32 { bad([1]) }
        "#;
        let result = run_source(source);
        assert!(matches!(
            result,
            Err(EvalError::RuntimeError(msg)) if msg.contains("non-exhaustive")
        ));
    }

    #[test]
    fn test_run_list_string() {
        let source = r#"fn main() -> List<String> { ["hello", "world"] }"#;
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
            fn len_check(xs: List<Int32>) -> Bool {
                match xs {
                    [] => true
                    [_] => true
                    [_, _] => true
                    _ => false
                }
            }
            fn main() -> Bool { len_check([1, 2]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // Suffix pattern tests
    #[test]
    fn test_run_list_match_suffix_pattern() {
        let source = r#"
            fn last_elem(xs: List<Int32>) -> Int32 {
                match xs {
                    [.., x] => x
                    [] => 0
                }
            }
            fn main() -> Int32 { last_elem([1, 2, 3]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(3));
    }

    #[test]
    fn test_run_list_match_suffix_pattern_single_elem() {
        let source = r#"
            fn last_elem(xs: List<Int32>) -> Int32 {
                match xs {
                    [.., x] => x
                    [] => 0
                }
            }
            fn main() -> Int32 { last_elem([42]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(42));
    }

    #[test]
    fn test_run_list_match_suffix_two_elements() {
        let source = r#"
            fn last_two(xs: List<Int32>) -> Int32 {
                match xs {
                    [.., a, b] => a + b
                    [x] => x
                    [] => 0
                }
            }
            fn main() -> Int32 { last_two([1, 2, 3, 4]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(7)); // 3 + 4
    }

    #[test]
    fn test_run_list_match_suffix_literal_pattern() {
        let source = r#"
            fn ends_with_zero(xs: List<Int32>) -> Bool {
                match xs {
                    [.., 0] => true
                    [_, ..] => false
                    [] => false
                }
            }
            fn main() -> Bool { ends_with_zero([1, 2, 0]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // Prefix+Suffix pattern tests
    #[test]
    fn test_run_list_match_prefix_suffix_pattern() {
        let source = r#"
            fn first_and_last(xs: List<Int32>) -> Int32 {
                match xs {
                    [a, .., b] => a + b
                    [a] => a
                    [] => 0
                }
            }
            fn main() -> Int32 { first_and_last([1, 2, 3, 4]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(5)); // 1 + 4
    }

    #[test]
    fn test_run_list_match_prefix_suffix_min_length() {
        // [a, .., b] requires at least 2 elements
        let source = r#"
            fn first_and_last(xs: List<Int32>) -> Int32 {
                match xs {
                    [a, .., b] => a + b
                    [a] => a
                    [] => 0
                }
            }
            fn main() -> Int32 { first_and_last([10, 20]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(30)); // 10 + 20
    }

    #[test]
    fn test_run_list_match_prefix_suffix_literals() {
        let source = r#"
            fn bookended_by_ones(xs: List<Int32>) -> Bool {
                match xs {
                    [1, .., 1] => true
                    [_, ..] => false
                    [] => false
                }
            }
            fn main() -> Bool { bookended_by_ones([1, 2, 3, 1]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_run_list_match_prefix_suffix_multiple() {
        let source = r#"
            fn middle_free(xs: List<Int32>) -> Int32 {
                match xs {
                    [a, b, .., y, z] => a + b + y + z
                    [a, b, c] => a + b + c
                    [a, b] => a + b
                    [a] => a
                    [] => 0
                }
            }
            fn main() -> Int32 { middle_free([1, 2, 3, 4, 5, 6]) }
        "#;
        let result = run_source(source).unwrap();
        assert_eq!(result, Value::Int32(14)); // 1 + 2 + 5 + 6
    }
}
