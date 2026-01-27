use std::fmt;

pub use rquickjs::Context;
use rquickjs::{BigInt, CatchResultExt, Runtime};

use crate::codegen::codegen;
use crate::ir::TypedExpr;
use crate::types::Type;

/// Create a new QuickJS runtime and context
pub fn create_context() -> Result<(Runtime, Context), String> {
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    Ok((runtime, context))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int32(i32),
    Int64(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int32(n) => write!(f, "{}", n),
            Value::Int64(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::List(elements) => {
                let items: Vec<String> = elements.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Tuple(elements) => {
                let items: Vec<String> = elements.iter().map(|v| v.to_string()).collect();
                if elements.len() == 1 {
                    write!(f, "({},)", items.join(", "))
                } else {
                    write!(f, "({})", items.join(", "))
                }
            }
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

/// Evaluate JS code in an existing context and convert to Value
pub fn eval_js_in_context(
    ctx: &rquickjs::Ctx<'_>,
    js_code: String,
    result_type: Type,
) -> Result<Value, EvalError> {
    match result_type {
        Type::Int32 => {
            let result: f64 = ctx.eval(js_code).catch(ctx).map_err(|e| {
                let msg = e.to_string();
                if msg.contains("division by zero") {
                    EvalError::DivisionByZero
                } else if msg.contains("Int32 overflow") {
                    EvalError::RuntimeError("Int32 overflow".to_string())
                } else {
                    EvalError::RuntimeError(msg)
                }
            })?;

            // Backup check for non-finite results
            if result.is_infinite() || result.is_nan() {
                return Err(EvalError::DivisionByZero);
            }

            Ok(Value::Int32(result as i32))
        }
        Type::Int64 => {
            let result: BigInt = ctx
                .eval(js_code)
                .catch(ctx)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            let value = result.to_i64().map_err(|_| {
                EvalError::RuntimeError("BigInt value too large for i64".to_string())
            })?;

            Ok(Value::Int64(value))
        }
        Type::Float => {
            let result: f64 = ctx
                .eval(js_code)
                .catch(ctx)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            if result.is_infinite() || result.is_nan() {
                return Err(EvalError::DivisionByZero);
            }

            Ok(Value::Float(result))
        }
        Type::Bool => {
            let result: bool = ctx
                .eval(js_code)
                .catch(ctx)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            Ok(Value::Bool(result))
        }
        Type::String => {
            let result: String = ctx
                .eval(js_code)
                .catch(ctx)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            Ok(Value::String(result))
        }
        Type::List(elem_type) => {
            let result: rquickjs::Array = ctx
                .eval(js_code)
                .catch(ctx)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            let mut values = Vec::new();
            for i in 0..result.len() {
                // Get each element as a JS value and convert based on element type
                let elem_value = js_array_elem_to_value(ctx, &result, i, &elem_type)?;
                values.push(elem_value);
            }

            Ok(Value::List(values))
        }
        Type::Tuple(elem_types) => {
            let result: rquickjs::Array = ctx
                .eval(js_code)
                .catch(ctx)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            let mut values = Vec::new();
            for (i, elem_type) in elem_types.iter().enumerate() {
                let elem_value = js_array_elem_to_value(ctx, &result, i, elem_type)?;
                values.push(elem_value);
            }

            Ok(Value::Tuple(values))
        }
        Type::Var(name) => Err(EvalError::RuntimeError(format!(
            "unresolved type variable: {}",
            name
        ))),
        Type::Function { .. } => Err(EvalError::RuntimeError(
            "cannot return function as top-level value".to_string(),
        )),
    }
}

/// Convert a JS array element to a Zoya Value based on element type
#[allow(clippy::only_used_in_recursion)]
fn js_array_elem_to_value(
    ctx: &rquickjs::Ctx<'_>,
    array: &rquickjs::Array,
    index: usize,
    elem_type: &Type,
) -> Result<Value, EvalError> {
    match elem_type {
        Type::Int32 => {
            let val: f64 = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::Int32(val as i32))
        }
        Type::Int64 => {
            let val: BigInt = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let value = val.to_i64().map_err(|_| {
                EvalError::RuntimeError("BigInt value too large for i64".to_string())
            })?;
            Ok(Value::Int64(value))
        }
        Type::Float => {
            let val: f64 = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::Float(val))
        }
        Type::Bool => {
            let val: bool = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::Bool(val))
        }
        Type::String => {
            let val: String = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::String(val))
        }
        Type::List(inner_type) => {
            let inner_array: rquickjs::Array = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut values = Vec::new();
            for i in 0..inner_array.len() {
                let elem_value = js_array_elem_to_value(ctx, &inner_array, i, inner_type)?;
                values.push(elem_value);
            }
            Ok(Value::List(values))
        }
        Type::Tuple(elem_types) => {
            let inner_array: rquickjs::Array = array
                .get(index)
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut values = Vec::new();
            for (i, elem_type) in elem_types.iter().enumerate() {
                let elem_value = js_array_elem_to_value(ctx, &inner_array, i, elem_type)?;
                values.push(elem_value);
            }
            Ok(Value::Tuple(values))
        }
        Type::Var(name) => Err(EvalError::RuntimeError(format!(
            "unresolved type variable in list element: {}",
            name
        ))),
        Type::Function { .. } => Err(EvalError::RuntimeError(
            "cannot have function as list/tuple element value".to_string(),
        )),
    }
}

#[allow(dead_code)]
pub fn eval(expr: &TypedExpr) -> Result<Value, EvalError> {
    let js_code = codegen(expr);
    let result_type = expr.ty();

    let rt = Runtime::new().map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    let ctx = Context::full(&rt).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    ctx.with(|ctx| eval_js_in_context(&ctx, js_code, result_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, UnaryOp};

    #[test]
    fn test_eval_int32() {
        let expr = TypedExpr::Int32(42);
        assert_eq!(eval(&expr), Ok(Value::Int32(42)));
    }

    #[test]
    fn test_eval_int64() {
        let expr = TypedExpr::Int64(42);
        assert_eq!(eval(&expr), Ok(Value::Int64(42)));
    }

    #[test]
    fn test_eval_float() {
        let expr = TypedExpr::Float(3.14);
        assert_eq!(eval(&expr), Ok(Value::Float(3.14)));
    }

    #[test]
    fn test_eval_int32_addition() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int32(2)),
            right: Box::new(TypedExpr::Int32(3)),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(5)));
    }

    #[test]
    fn test_eval_int64_addition() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int64(2)),
            right: Box::new(TypedExpr::Int64(3)),
            ty: Type::Int64,
        };
        assert_eq!(eval(&expr), Ok(Value::Int64(5)));
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
    fn test_eval_int32_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int32(10)),
            right: Box::new(TypedExpr::Int32(4)),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(6)));
    }

    #[test]
    fn test_eval_int32_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int32(3)),
            right: Box::new(TypedExpr::Int32(7)),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(21)));
    }

    #[test]
    fn test_eval_int32_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int32(20)),
            right: Box::new(TypedExpr::Int32(4)),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(5)));
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
    fn test_eval_int32_division_by_zero() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int32(10)),
            right: Box::new(TypedExpr::Int32(0)),
            ty: Type::Int32,
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
    fn test_eval_complex_int32_expression() {
        // 2 + 3 * (4 - 1) = 2 + 3 * 3 = 2 + 9 = 11
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int32(2)),
            right: Box::new(TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Int32(3)),
                right: Box::new(TypedExpr::BinOp {
                    op: BinOp::Sub,
                    left: Box::new(TypedExpr::Int32(4)),
                    right: Box::new(TypedExpr::Int32(1)),
                    ty: Type::Int32,
                }),
                ty: Type::Int32,
            }),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(11)));
    }

    #[test]
    fn test_eval_unary_negation_int32() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int32(42)),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(-42)));
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
                expr: Box::new(TypedExpr::Int32(42)),
                ty: Type::Int32,
            }),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(42)));
    }

    #[test]
    fn test_eval_negate_expression() {
        // -(2 + 3) = -5
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Int32(2)),
                right: Box::new(TypedExpr::Int32(3)),
                ty: Type::Int32,
            }),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(-5)));
    }

    #[test]
    fn test_eval_negative_int32_result() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int32(3)),
            right: Box::new(TypedExpr::Int32(10)),
            ty: Type::Int32,
        };
        assert_eq!(eval(&expr), Ok(Value::Int32(-7)));
    }

    #[test]
    fn test_eval_int64_large_value() {
        let expr = TypedExpr::Int64(9_000_000_000);
        assert_eq!(eval(&expr), Ok(Value::Int64(9_000_000_000)));
    }
}
