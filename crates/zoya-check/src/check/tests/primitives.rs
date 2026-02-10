use zoya_ast::Expr;
use zoya_ir::{Type, TypedExpr};

use super::check_expr_with_env;

#[test]
fn test_check_int() {
    let expr = Expr::Int(42);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
    assert_eq!(result, TypedExpr::Int(42));
}

#[test]
fn test_check_int_large() {
    // Large integers now work fine since Int uses i64 internally
    let expr = Expr::Int(3_000_000_000);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
    assert_eq!(result, TypedExpr::Int(3_000_000_000));
}

#[test]
fn test_check_bigint() {
    let expr = Expr::BigInt(42);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::BigInt);
    assert_eq!(result, TypedExpr::BigInt(42));
}

#[test]
fn test_check_bigint_large() {
    let expr = Expr::BigInt(9_000_000_000);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::BigInt);
    assert_eq!(result, TypedExpr::BigInt(9_000_000_000));
}

#[test]
fn test_check_float() {
    let expr = Expr::Float(3.15);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Float);
    assert_eq!(result, TypedExpr::Float(3.15));
}

#[test]
fn test_check_bool_true() {
    let expr = Expr::Bool(true);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
    assert_eq!(result, TypedExpr::Bool(true));
}

#[test]
fn test_check_bool_false() {
    let expr = Expr::Bool(false);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
    assert_eq!(result, TypedExpr::Bool(false));
}

#[test]
fn test_check_expression_type_inference() {
    // Simple expression check - no need for module infrastructure
    let result = check_expr_with_env(&Expr::Int(42)).unwrap();
    assert_eq!(result.ty(), Type::Int);
}
