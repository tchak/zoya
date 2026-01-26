use std::fmt;

use crate::ast::{BinOp, Expr, UnaryOp};

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
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::DivisionByZero => write!(f, "division by zero"),
        }
    }
}

pub fn eval(expr: &Expr) -> Result<Value, EvalError> {
    match expr {
        Expr::Int(n) => Ok(Value::Int(*n)),
        Expr::Float(n) => Ok(Value::Float(*n)),
        Expr::UnaryOp { op, expr } => {
            let val = eval(expr)?;
            match op {
                UnaryOp::Neg => match val {
                    Value::Int(n) => Ok(Value::Int(-n)),
                    Value::Float(n) => Ok(Value::Float(-n)),
                },
            }
        }
        Expr::BinOp { op, left, right } => {
            let l = eval(left)?;
            let r = eval(right)?;
            // Type checker ensures types match, so we can safely match
            match (l, r) {
                (Value::Int(l), Value::Int(r)) => eval_int_op(*op, l, r),
                (Value::Float(l), Value::Float(r)) => eval_float_op(*op, l, r),
                _ => unreachable!("type checker ensures matching types"),
            }
        }
    }
}

fn eval_int_op(op: BinOp, l: i64, r: i64) -> Result<Value, EvalError> {
    match op {
        BinOp::Add => Ok(Value::Int(l + r)),
        BinOp::Sub => Ok(Value::Int(l - r)),
        BinOp::Mul => Ok(Value::Int(l * r)),
        BinOp::Div => {
            if r == 0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(Value::Int(l / r))
            }
        }
    }
}

fn eval_float_op(op: BinOp, l: f64, r: f64) -> Result<Value, EvalError> {
    match op {
        BinOp::Add => Ok(Value::Float(l + r)),
        BinOp::Sub => Ok(Value::Float(l - r)),
        BinOp::Mul => Ok(Value::Float(l * r)),
        BinOp::Div => {
            if r == 0.0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(Value::Float(l / r))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_integer() {
        let expr = Expr::Int(42);
        assert_eq!(eval(&expr), Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_float() {
        let expr = Expr::Float(3.14);
        assert_eq!(eval(&expr), Ok(Value::Float(3.14)));
    }

    #[test]
    fn test_eval_int_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int(2)),
            right: Box::new(Expr::Int(3)),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_float_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Float(1.5)),
            right: Box::new(Expr::Float(2.5)),
        };
        assert_eq!(eval(&expr), Ok(Value::Float(4.0)));
    }

    #[test]
    fn test_eval_int_subtraction() {
        let expr = Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(Expr::Int(10)),
            right: Box::new(Expr::Int(4)),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(6)));
    }

    #[test]
    fn test_eval_int_multiplication() {
        let expr = Expr::BinOp {
            op: BinOp::Mul,
            left: Box::new(Expr::Int(3)),
            right: Box::new(Expr::Int(7)),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(21)));
    }

    #[test]
    fn test_eval_int_division() {
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Int(20)),
            right: Box::new(Expr::Int(4)),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_float_division() {
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Float(5.0)),
            right: Box::new(Expr::Float(2.0)),
        };
        assert_eq!(eval(&expr), Ok(Value::Float(2.5)));
    }

    #[test]
    fn test_eval_int_division_by_zero() {
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Int(10)),
            right: Box::new(Expr::Int(0)),
        };
        assert_eq!(eval(&expr), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn test_eval_float_division_by_zero() {
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Float(10.0)),
            right: Box::new(Expr::Float(0.0)),
        };
        assert_eq!(eval(&expr), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn test_eval_complex_int_expression() {
        // 2 + 3 * (4 - 1) = 2 + 3 * 3 = 2 + 9 = 11
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
        assert_eq!(eval(&expr), Ok(Value::Int(11)));
    }

    #[test]
    fn test_eval_nested_division_by_zero() {
        // 10 / (5 - 5) should error
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Int(10)),
            right: Box::new(Expr::BinOp {
                op: BinOp::Sub,
                left: Box::new(Expr::Int(5)),
                right: Box::new(Expr::Int(5)),
            }),
        };
        assert_eq!(eval(&expr), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn test_eval_negative_int_result() {
        let expr = Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(Expr::Int(3)),
            right: Box::new(Expr::Int(10)),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(-7)));
    }

    #[test]
    fn test_eval_unary_negation_int() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Int(42)),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(-42)));
    }

    #[test]
    fn test_eval_unary_negation_float() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Float(3.14)),
        };
        assert_eq!(eval(&expr), Ok(Value::Float(-3.14)));
    }

    #[test]
    fn test_eval_double_negation() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(Expr::Int(42)),
            }),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_negate_expression() {
        // -(2 + 3) = -5
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Int(2)),
                right: Box::new(Expr::Int(3)),
            }),
        };
        assert_eq!(eval(&expr), Ok(Value::Int(-5)));
    }
}
