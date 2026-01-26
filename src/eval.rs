use std::fmt;

use rquickjs::{Context, Runtime};

use crate::codegen::codegen;
use crate::ir::TypedExpr;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    DivisionByZero,
    RuntimeError(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::DivisionByZero => write!(f, "division by zero"),
            EvalError::RuntimeError(msg) => write!(f, "runtime error: {}", msg),
        }
    }
}

pub fn eval(expr: &TypedExpr) -> Result<Value, EvalError> {
    let js_code = codegen(expr);
    let result_type = expr.ty();

    let rt = Runtime::new().map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    let ctx = Context::full(&rt).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    ctx.with(|ctx| {
        let result: f64 = ctx
            .eval(js_code)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

        // Check for division by zero (JS returns Infinity or NaN)
        if result.is_infinite() || result.is_nan() {
            return Err(EvalError::DivisionByZero);
        }

        match result_type {
            Type::Int => Ok(Value::Int(result as i64)),
            Type::Float => Ok(Value::Float(result)),
            Type::Var(name) => Err(EvalError::RuntimeError(format!(
                "unresolved type variable: {}",
                name
            ))),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, UnaryOp};

    #[test]
    fn test_eval_integer() {
        let expr = TypedExpr::Int(42);
        assert_eq!(eval(&expr), Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_float() {
        let expr = TypedExpr::Float(3.14);
        assert_eq!(eval(&expr), Ok(Value::Float(3.14)));
    }

    #[test]
    fn test_eval_int_addition() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(2)),
            right: Box::new(TypedExpr::Int(3)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_float_addition() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        assert_eq!(eval(&expr), Ok(Value::Float(4.0)));
    }

    #[test]
    fn test_eval_int_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int(10)),
            right: Box::new(TypedExpr::Int(4)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(6)));
    }

    #[test]
    fn test_eval_int_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int(3)),
            right: Box::new(TypedExpr::Int(7)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(21)));
    }

    #[test]
    fn test_eval_int_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int(20)),
            right: Box::new(TypedExpr::Int(4)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_float_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Float(5.0)),
            right: Box::new(TypedExpr::Float(2.0)),
            ty: Type::Float,
        };
        assert_eq!(eval(&expr), Ok(Value::Float(2.5)));
    }

    #[test]
    fn test_eval_int_division_by_zero() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int(10)),
            right: Box::new(TypedExpr::Int(0)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn test_eval_float_division_by_zero() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Float(10.0)),
            right: Box::new(TypedExpr::Float(0.0)),
            ty: Type::Float,
        };
        assert_eq!(eval(&expr), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn test_eval_complex_int_expression() {
        // 2 + 3 * (4 - 1) = 2 + 3 * 3 = 2 + 9 = 11
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(2)),
            right: Box::new(TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Int(3)),
                right: Box::new(TypedExpr::BinOp {
                    op: BinOp::Sub,
                    left: Box::new(TypedExpr::Int(4)),
                    right: Box::new(TypedExpr::Int(1)),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            }),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(11)));
    }

    #[test]
    fn test_eval_unary_negation_int() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int(42)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(-42)));
    }

    #[test]
    fn test_eval_unary_negation_float() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Float(3.14)),
            ty: Type::Float,
        };
        assert_eq!(eval(&expr), Ok(Value::Float(-3.14)));
    }

    #[test]
    fn test_eval_double_negation() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(TypedExpr::Int(42)),
                ty: Type::Int,
            }),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_negate_expression() {
        // -(2 + 3) = -5
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Int(2)),
                right: Box::new(TypedExpr::Int(3)),
                ty: Type::Int,
            }),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(-5)));
    }

    #[test]
    fn test_eval_negative_int_result() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int(3)),
            right: Box::new(TypedExpr::Int(10)),
            ty: Type::Int,
        };
        assert_eq!(eval(&expr), Ok(Value::Int(-7)));
    }
}
