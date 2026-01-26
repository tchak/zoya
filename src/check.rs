use crate::ast::{Expr, UnaryOp};
use crate::ir::TypedExpr;
use crate::types::TypeError;

pub fn check(expr: &Expr) -> Result<TypedExpr, TypeError> {
    match expr {
        Expr::Int(n) => Ok(TypedExpr::Int(*n)),
        Expr::Float(n) => Ok(TypedExpr::Float(*n)),
        Expr::UnaryOp { op, expr } => {
            let typed_expr = check(expr)?;
            let ty = typed_expr.ty();
            match op {
                UnaryOp::Neg => Ok(TypedExpr::UnaryOp {
                    op: *op,
                    expr: Box::new(typed_expr),
                    ty,
                }),
            }
        }
        Expr::BinOp { op, left, right } => {
            let typed_left = check(left)?;
            let typed_right = check(right)?;
            let left_ty = typed_left.ty();
            let right_ty = typed_right.ty();
            if left_ty != right_ty {
                return Err(TypeError {
                    message: format!("type mismatch: {} vs {}", left_ty, right_ty),
                });
            }
            Ok(TypedExpr::BinOp {
                op: *op,
                left: Box::new(typed_left),
                right: Box::new(typed_right),
                ty: left_ty,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;
    use crate::types::Type;

    #[test]
    fn test_check_int() {
        let expr = Expr::Int(42);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
        assert_eq!(result, TypedExpr::Int(42));
    }

    #[test]
    fn test_check_float() {
        let expr = Expr::Float(3.14);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Float);
        assert_eq!(result, TypedExpr::Float(3.14));
    }

    #[test]
    fn test_check_int_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_float_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Float(1.0)),
            right: Box::new(Expr::Float(2.0)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Float);
    }

    #[test]
    fn test_check_type_mismatch() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Float(2.0)),
        };
        let result = check(&expr);
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
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_negate_float() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Float(3.14)),
        };
        let result = check(&expr).unwrap();
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
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
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
        let result = check(&expr);
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
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }
}
