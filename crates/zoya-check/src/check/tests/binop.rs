use zoya_ast::{BinOp, Expr, UnaryOp};
use zoya_ir::Type;

use super::check_expr_with_env;

#[test]
fn test_check_bigint_addition() {
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::BigInt(1)),
        right: Box::new(Expr::BigInt(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::BigInt);
}

#[test]
fn test_check_int_addition() {
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Int(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_float_addition() {
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::Float(1.0)),
        right: Box::new(Expr::Float(2.0)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Float);
}

#[test]
fn test_check_type_mismatch() {
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Float(2.0)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("type mismatch"));
}

#[test]
fn test_check_negate_int() {
    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        expr: Box::new(Expr::Int(42)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_negate_float() {
    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        expr: Box::new(Expr::Float(3.15)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Float);
}

#[test]
fn test_check_negate_expression() {
    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        expr: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        }),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_negate_bool_error() {
    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        expr: Box::new(Expr::Bool(true)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("negation"));
}

#[test]
fn test_check_negate_string_error() {
    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        expr: Box::new(Expr::String("hello".to_string())),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("negation"));
}

#[test]
fn test_check_nested_type_mismatch() {
    // 1 + (2.0 * 3.0) should fail because 1 is Int and (2.0 * 3.0) is Float
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::BinOp {
            op: BinOp::Mul,
            left: Box::new(Expr::Float(2.0)),
            right: Box::new(Expr::Float(3.0)),
        }),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
}

#[test]
fn test_check_complex_int_expression() {
    // 2 + 3 * (4 - 1)
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::Int(2)),
        right: Box::new(Expr::BinOp {
            op: BinOp::Mul,
            left: Box::new(Expr::Int(3)),
            right: Box::new(Expr::BinOp {
                op: BinOp::Sub,
                left: Box::new(Expr::Int(4)),
                right: Box::new(Expr::Int(1)),
            }),
        }),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_equality_int() {
    let expr = Expr::BinOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Int(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_inequality_int() {
    let expr = Expr::BinOp {
        op: BinOp::Ne,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Int(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_equality_bool() {
    let expr = Expr::BinOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Bool(true)),
        right: Box::new(Expr::Bool(false)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_less_than_int() {
    let expr = Expr::BinOp {
        op: BinOp::Lt,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Int(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_greater_than_float() {
    let expr = Expr::BinOp {
        op: BinOp::Gt,
        left: Box::new(Expr::Float(1.5)),
        right: Box::new(Expr::Float(2.5)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_less_equal_int() {
    let expr = Expr::BinOp {
        op: BinOp::Le,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Int(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_greater_equal_int() {
    let expr = Expr::BinOp {
        op: BinOp::Ge,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Int(2)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Bool);
}

#[test]
fn test_check_ordering_on_bool_error() {
    let expr = Expr::BinOp {
        op: BinOp::Lt,
        left: Box::new(Expr::Bool(true)),
        right: Box::new(Expr::Bool(false)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .message
            .contains("ordering operators only work on numeric types")
    );
}

#[test]
fn test_check_comparison_type_mismatch() {
    let expr = Expr::BinOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Int(1)),
        right: Box::new(Expr::Float(1.0)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("type mismatch"));
}
