use zoya_ast::{Expr, StringPart};
use zoya_ir::Type;

use super::check_expr_with_env;

#[test]
fn test_interpolated_string_literal_only() {
    let expr = Expr::InterpolatedString(vec![StringPart::Literal("hello".to_string())]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_interpolated_string_with_string_expr() {
    let expr = Expr::InterpolatedString(vec![
        StringPart::Literal("hello ".to_string()),
        StringPart::Expr(Box::new(Expr::String("world".to_string()))),
        StringPart::Literal("!".to_string()),
    ]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_interpolated_string_with_int_expr() {
    let expr = Expr::InterpolatedString(vec![
        StringPart::Literal("count: ".to_string()),
        StringPart::Expr(Box::new(Expr::Int(42))),
    ]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_interpolated_string_with_float_expr() {
    let expr = Expr::InterpolatedString(vec![
        StringPart::Literal("pi: ".to_string()),
        StringPart::Expr(Box::new(Expr::Float(3.14))),
    ]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_interpolated_string_with_bigint_expr() {
    let expr = Expr::InterpolatedString(vec![
        StringPart::Literal("big: ".to_string()),
        StringPart::Expr(Box::new(Expr::BigInt(999))),
    ]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_interpolated_string_with_bool_expr_error() {
    let expr = Expr::InterpolatedString(vec![
        StringPart::Literal("flag: ".to_string()),
        StringPart::Expr(Box::new(Expr::Bool(true))),
    ]);
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("cannot interpolate expression of type Bool")
    );
}

#[test]
fn test_interpolated_string_empty() {
    let expr = Expr::InterpolatedString(vec![]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}
