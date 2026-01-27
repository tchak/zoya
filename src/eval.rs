use std::fmt;

pub use rquickjs::Context;
use rquickjs::{BigInt, CatchResultExt, Runtime};

use crate::codegen::{codegen, prelude};
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
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Enum {
        enum_name: String,
        variant_name: String,
        fields: EnumValueFields,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnumValueFields {
    Unit,
    Tuple(Vec<Value>),
    Struct(Vec<(String, Value)>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::BigInt(n) => write!(f, "{}", n),
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
            Value::Struct { name, fields } => {
                if fields.is_empty() {
                    write!(f, "{} {{}}", name)
                } else {
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, v))
                        .collect();
                    write!(f, "{} {{ {} }}", name, field_strs.join(", "))
                }
            }
            Value::Fn { params, ret } => {
                if params.is_empty() {
                    write!(f, "<fn() -> {}>", ret)
                } else if params.len() == 1 {
                    write!(f, "<fn({}) -> {}>", params[0], ret)
                } else {
                    let param_strs: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                    write!(f, "<fn({}) -> {}>", param_strs.join(", "), ret)
                }
            }
            Value::Enum {
                enum_name,
                variant_name,
                fields,
            } => match fields {
                EnumValueFields::Unit => write!(f, "{}::{}", enum_name, variant_name),
                EnumValueFields::Tuple(values) => {
                    let items: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                    write!(f, "{}::{}({})", enum_name, variant_name, items.join(", "))
                }
                EnumValueFields::Struct(field_values) => {
                    if field_values.is_empty() {
                        write!(f, "{}::{} {{}}", enum_name, variant_name)
                    } else {
                        let field_strs: Vec<String> = field_values
                            .iter()
                            .map(|(k, v)| format!("{}: {}", k, v))
                            .collect();
                        write!(
                            f,
                            "{}::{} {{ {} }}",
                            enum_name,
                            variant_name,
                            field_strs.join(", ")
                        )
                    }
                }
            },
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
    let js_val: rquickjs::Value = ctx.eval(js_code).catch(ctx).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("division by zero") {
            EvalError::DivisionByZero
        } else {
            EvalError::RuntimeError(msg)
        }
    })?;

    js_value_to_value(ctx, js_val, &result_type)
}

/// Convert a JavaScript value to a Zoya Value based on expected type
#[allow(clippy::only_used_in_recursion)]
fn js_value_to_value(
    ctx: &rquickjs::Ctx<'_>,
    js_val: rquickjs::Value<'_>,
    expected_type: &Type,
) -> Result<Value, EvalError> {
    match expected_type {
        Type::Int => {
            let val: f64 = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            if !val.is_finite() {
                return Err(EvalError::DivisionByZero);
            }
            Ok(Value::Int(val as i64))
        }
        Type::BigInt => {
            let val: BigInt = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let value = val.to_i64().map_err(|_| {
                EvalError::RuntimeError("BigInt value too large for i64".to_string())
            })?;
            Ok(Value::BigInt(value))
        }
        Type::Float => {
            let val: f64 = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            if !val.is_finite() {
                return Err(EvalError::DivisionByZero);
            }
            Ok(Value::Float(val))
        }
        Type::Bool => {
            let val: bool = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::Bool(val))
        }
        Type::String => {
            let val: String = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::String(val))
        }
        Type::List(elem_type) => {
            let array: rquickjs::Array = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut values = Vec::new();
            for i in 0..array.len() {
                let elem_js: rquickjs::Value = array
                    .get(i)
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let elem_value = js_value_to_value(ctx, elem_js, elem_type)?;
                values.push(elem_value);
            }
            Ok(Value::List(values))
        }
        Type::Tuple(elem_types) => {
            let array: rquickjs::Array = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut values = Vec::new();
            for (i, elem_type) in elem_types.iter().enumerate() {
                let elem_js: rquickjs::Value = array
                    .get(i)
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let elem_value = js_value_to_value(ctx, elem_js, elem_type)?;
                values.push(elem_value);
            }
            Ok(Value::Tuple(values))
        }
        Type::Struct { name, fields, .. } => {
            let obj: rquickjs::Object = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut field_values = Vec::new();
            for (field_name, field_type) in fields {
                let field_js: rquickjs::Value = obj
                    .get(field_name.as_str())
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let field_value = js_value_to_value(ctx, field_js, field_type)?;
                field_values.push((field_name.clone(), field_value));
            }
            Ok(Value::Struct {
                name: name.clone(),
                fields: field_values,
            })
        }
        Type::Var(id) => Err(EvalError::RuntimeError(format!(
            "unresolved type variable: {}",
            id
        ))),
        Type::Function { params, ret } => Ok(Value::Fn {
            params: params.clone(),
            ret: ret.clone(),
        }),
        Type::Enum {
            name: enum_name,
            variants,
            ..
        } => {
            use crate::types::EnumVariantType;
            let obj: rquickjs::Object = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let tag: String = obj
                .get("$tag")
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            // Find the variant type
            let variant_type = variants
                .iter()
                .find(|(vname, _)| vname == &tag)
                .map(|(_, vt)| vt)
                .ok_or_else(|| {
                    EvalError::RuntimeError(format!("unknown enum variant: {}", tag))
                })?;

            let fields = match variant_type {
                EnumVariantType::Unit => EnumValueFields::Unit,
                EnumVariantType::Tuple(field_types) => {
                    let mut values = Vec::new();
                    for (i, field_type) in field_types.iter().enumerate() {
                        let field_js: rquickjs::Value = obj
                            .get(format!("${}", i))
                            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                        let field_value = js_value_to_value(ctx, field_js, field_type)?;
                        values.push(field_value);
                    }
                    EnumValueFields::Tuple(values)
                }
                EnumVariantType::Struct(field_defs) => {
                    let mut field_values = Vec::new();
                    for (field_name, field_type) in field_defs {
                        let field_js: rquickjs::Value = obj
                            .get(field_name.as_str())
                            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                        let field_value = js_value_to_value(ctx, field_js, field_type)?;
                        field_values.push((field_name.clone(), field_value));
                    }
                    EnumValueFields::Struct(field_values)
                }
            };

            Ok(Value::Enum {
                enum_name: enum_name.clone(),
                variant_name: tag,
                fields,
            })
        }
    }
}

#[allow(dead_code)]
pub fn eval(expr: &TypedExpr) -> Result<Value, EvalError> {
    let js_code = codegen(expr);
    let result_type = expr.ty();

    let rt = Runtime::new().map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    let ctx = Context::full(&rt).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    ctx.with(|ctx| {
        // Load prelude helpers first
        ctx.eval::<(), _>(prelude())
            .catch(&ctx)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        eval_js_in_context(&ctx, js_code, result_type)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, UnaryOp};

    #[test]
    fn test_eval_int() {
        let expr = TypedExpr::Int(42);
        assert_eq!(eval(&expr), Ok(Value::Int(42)));
    }

    #[test]
    fn test_eval_bigint() {
        let expr = TypedExpr::BigInt(42);
        assert_eq!(eval(&expr), Ok(Value::BigInt(42)));
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
    fn test_eval_bigint_addition() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::BigInt(2)),
            right: Box::new(TypedExpr::BigInt(3)),
            ty: Type::BigInt,
        };
        assert_eq!(eval(&expr), Ok(Value::BigInt(5)));
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

    #[test]
    fn test_eval_bigint_large_value() {
        let expr = TypedExpr::BigInt(9_000_000_000);
        assert_eq!(eval(&expr), Ok(Value::BigInt(9_000_000_000)));
    }
}
