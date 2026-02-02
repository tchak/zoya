use zoya_ast::Expr;
use zoya_ir::Type;

use super::check_expr_with_env;

// String method tests

#[test]
fn test_check_method_call_len() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("hello".to_string())),
        method: "len".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_method_call_is_empty() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("".to_string())),
        method: "is_empty".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_method_call_contains() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("hello".to_string())),
        method: "contains".to_string(),
        args: vec![Expr::String("ell".to_string())],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_method_call_to_uppercase() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("hello".to_string())),
        method: "to_uppercase".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_check_method_call_trim() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("  hello  ".to_string())),
        method: "trim".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_check_method_call_unknown_method() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("hello".to_string())),
        method: "foo".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("no method 'foo'"));
}

#[test]
fn test_check_method_call_on_int_error() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::Int(42)),
        method: "len".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("no method 'len' on type Int"));
}

#[test]
fn test_check_method_call_wrong_arg_count() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("hello".to_string())),
        method: "contains".to_string(),
        args: vec![], // contains expects 1 argument
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("expects 1 argument"));
}

#[test]
fn test_check_method_call_wrong_arg_type() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::String("hello".to_string())),
        method: "contains".to_string(),
        args: vec![Expr::Int(42)], // contains expects String, not Int
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("type mismatch"));
}

#[test]
fn test_check_chained_method_calls() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "to_uppercase".to_string(),
            args: vec![],
        }),
        method: "len".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

// List method tests

#[test]
fn test_check_list_len() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
        method: "len".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_list_is_empty() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![])),
        method: "is_empty".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_list_reverse() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
        method: "reverse".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
}

#[test]
fn test_check_list_push() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
        method: "push".to_string(),
        args: vec![Expr::Int(3)],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
}

#[test]
fn test_check_list_push_type_mismatch() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
        method: "push".to_string(),
        args: vec![Expr::String("hello".to_string())],
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("type mismatch"));
}

#[test]
fn test_check_list_concat() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
        method: "concat".to_string(),
        args: vec![Expr::List(vec![Expr::Int(3), Expr::Int(4)])],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
}

#[test]
fn test_check_list_concat_type_mismatch() {
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
        method: "concat".to_string(),
        args: vec![Expr::List(vec![Expr::String("hello".to_string())])],
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("type mismatch"));
}

#[test]
fn test_check_list_chained_methods() {
    // [1, 2].push(3).reverse()
    let expr = Expr::MethodCall {
        receiver: Box::new(Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "push".to_string(),
            args: vec![Expr::Int(3)],
        }),
        method: "reverse".to_string(),
        args: vec![],
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
}
