use crate::ast::{BinOp, UnaryOp};
use crate::ir::TypedExpr;

use crate::ir::TypedFunction;

pub fn codegen(expr: &TypedExpr) -> String {
    match expr {
        TypedExpr::Int(n) => n.to_string(),
        TypedExpr::Float(n) => format_float(*n),
        TypedExpr::Var { name, .. } => name.clone(),
        TypedExpr::Call { func, args, .. } => {
            let args_str: Vec<String> = args.iter().map(codegen).collect();
            format!("{}({})", func, args_str.join(", "))
        }
        TypedExpr::UnaryOp { op, expr, .. } => {
            let inner = codegen(expr);
            match op {
                UnaryOp::Neg => format!("(-({}))", inner),
            }
        }
        TypedExpr::BinOp { op, left, right, .. } => {
            let l = codegen(left);
            let r = codegen(right);
            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
            };
            format!("(({}) {} ({}))", l, op_str, r)
        }
    }
}

/// Generate JS code for a function definition
pub fn codegen_function(func: &TypedFunction) -> String {
    let params: Vec<&str> = func.params.iter().map(|(name, _)| name.as_str()).collect();
    let body = codegen(&func.body);
    format!(
        "function {}({}) {{ return {}; }}",
        func.name,
        params.join(", "),
        body
    )
}

fn format_float(n: f64) -> String {
    let s = n.to_string();
    // Ensure float always has decimal point for JS
    if s.contains('.') {
        s
    } else {
        format!("{}.0", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    #[test]
    fn test_codegen_int() {
        let expr = TypedExpr::Int(42);
        assert_eq!(codegen(&expr), "42");
    }

    #[test]
    fn test_codegen_negative_int() {
        let expr = TypedExpr::Int(-42);
        assert_eq!(codegen(&expr), "-42");
    }

    #[test]
    fn test_codegen_float() {
        let expr = TypedExpr::Float(3.14);
        assert_eq!(codegen(&expr), "3.14");
    }

    #[test]
    fn test_codegen_float_whole_number() {
        let expr = TypedExpr::Float(5.0);
        assert_eq!(codegen(&expr), "5.0");
    }

    #[test]
    fn test_codegen_unary_neg() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int(42)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "(-(42))");
    }

    #[test]
    fn test_codegen_addition() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(1)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((1) + (2))");
    }

    #[test]
    fn test_codegen_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int(5)),
            right: Box::new(TypedExpr::Int(3)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((5) - (3))");
    }

    #[test]
    fn test_codegen_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int(3)),
            right: Box::new(TypedExpr::Int(4)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((3) * (4))");
    }

    #[test]
    fn test_codegen_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int(10)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((10) / (2))");
    }

    #[test]
    fn test_codegen_complex_expression() {
        // 2 + 3 * 4
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(2)),
            right: Box::new(TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Int(3)),
                right: Box::new(TypedExpr::Int(4)),
                ty: Type::Int,
            }),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((2) + (((3) * (4))))");
    }

    #[test]
    fn test_codegen_float_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        assert_eq!(codegen(&expr), "((1.5) + (2.5))");
    }

    #[test]
    fn test_codegen_var() {
        let expr = TypedExpr::Var {
            name: "x".to_string(),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "x");
    }

    #[test]
    fn test_codegen_call_no_args() {
        let expr = TypedExpr::Call {
            func: "foo".to_string(),
            args: vec![],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "foo()");
    }

    #[test]
    fn test_codegen_call_one_arg() {
        let expr = TypedExpr::Call {
            func: "square".to_string(),
            args: vec![TypedExpr::Int(5)],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "square(5)");
    }

    #[test]
    fn test_codegen_call_multiple_args() {
        let expr = TypedExpr::Call {
            func: "add".to_string(),
            args: vec![TypedExpr::Int(1), TypedExpr::Int(2)],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "add(1, 2)");
    }

    #[test]
    fn test_codegen_call_with_vars() {
        let expr = TypedExpr::Call {
            func: "add".to_string(),
            args: vec![
                TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int,
                },
                TypedExpr::Var {
                    name: "y".to_string(),
                    ty: Type::Int,
                },
            ],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "add(x, y)");
    }

    #[test]
    fn test_codegen_var_in_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Var {
                name: "x".to_string(),
                ty: Type::Int,
            }),
            right: Box::new(TypedExpr::Int(1)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((x) + (1))");
    }

    #[test]
    fn test_codegen_function() {
        let func = TypedFunction {
            name: "square".to_string(),
            params: vec![("x".to_string(), Type::Int)],
            body: TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int,
                }),
                right: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            },
            return_type: Type::Int,
        };
        assert_eq!(
            codegen_function(&func),
            "function square(x) { return ((x) * (x)); }"
        );
    }

    #[test]
    fn test_codegen_function_multiple_params() {
        let func = TypedFunction {
            name: "add".to_string(),
            params: vec![
                ("x".to_string(), Type::Int),
                ("y".to_string(), Type::Int),
            ],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int,
                }),
                right: Box::new(TypedExpr::Var {
                    name: "y".to_string(),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            },
            return_type: Type::Int,
        };
        assert_eq!(
            codegen_function(&func),
            "function add(x, y) { return ((x) + (y)); }"
        );
    }

    #[test]
    fn test_codegen_function_no_params() {
        let func = TypedFunction {
            name: "answer".to_string(),
            params: vec![],
            body: TypedExpr::Int(42),
            return_type: Type::Int,
        };
        assert_eq!(
            codegen_function(&func),
            "function answer() { return 42; }"
        );
    }
}
