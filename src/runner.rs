use std::path::Path;

use crate::check::check_file;
use crate::codegen::codegen_function;
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
}
