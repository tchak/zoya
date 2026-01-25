use crate::ast::{BinOp, Expr};

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    DivisionByZero,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::DivisionByZero => write!(f, "division by zero"),
        }
    }
}

pub fn eval(expr: &Expr) -> Result<i64, EvalError> {
    match expr {
        Expr::Int(n) => Ok(*n),
        Expr::BinOp { op, left, right } => {
            let l = eval(left)?;
            let r = eval(right)?;
            match op {
                BinOp::Add => Ok(l + r),
                BinOp::Sub => Ok(l - r),
                BinOp::Mul => Ok(l * r),
                BinOp::Div => {
                    if r == 0 {
                        Err(EvalError::DivisionByZero)
                    } else {
                        Ok(l / r)
                    }
                }
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
        assert_eq!(eval(&expr), Ok(42));
    }

    #[test]
    fn test_eval_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int(2)),
            right: Box::new(Expr::Int(3)),
        };
        assert_eq!(eval(&expr), Ok(5));
    }

    #[test]
    fn test_eval_subtraction() {
        let expr = Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(Expr::Int(10)),
            right: Box::new(Expr::Int(4)),
        };
        assert_eq!(eval(&expr), Ok(6));
    }

    #[test]
    fn test_eval_multiplication() {
        let expr = Expr::BinOp {
            op: BinOp::Mul,
            left: Box::new(Expr::Int(3)),
            right: Box::new(Expr::Int(7)),
        };
        assert_eq!(eval(&expr), Ok(21));
    }

    #[test]
    fn test_eval_division() {
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Int(20)),
            right: Box::new(Expr::Int(4)),
        };
        assert_eq!(eval(&expr), Ok(5));
    }

    #[test]
    fn test_eval_division_by_zero() {
        let expr = Expr::BinOp {
            op: BinOp::Div,
            left: Box::new(Expr::Int(10)),
            right: Box::new(Expr::Int(0)),
        };
        assert_eq!(eval(&expr), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn test_eval_complex_expression() {
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
        assert_eq!(eval(&expr), Ok(11));
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
    fn test_eval_negative_result() {
        let expr = Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(Expr::Int(3)),
            right: Box::new(Expr::Int(10)),
        };
        assert_eq!(eval(&expr), Ok(-7));
    }
}
