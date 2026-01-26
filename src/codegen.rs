use crate::ast::{BinOp, UnaryOp};
use crate::ir::{TypedExpr, TypedFunction, TypedLetBinding, TypedMatchArm, TypedPattern};

use crate::types::Type;

pub fn codegen(expr: &TypedExpr) -> String {
    match expr {
        TypedExpr::Int32(n) => n.to_string(),
        TypedExpr::Int64(n) => format!("{}n", n), // BigInt literal
        TypedExpr::Float(n) => format_float(*n),
        TypedExpr::Bool(b) => b.to_string(),
        TypedExpr::String(s) => escape_js_string(s),
        TypedExpr::Var { name, .. } => name.clone(),
        TypedExpr::Call { func, args, ty } => {
            let args_str: Vec<String> = args.iter().map(codegen).collect();
            let call = format!("{}({})", func, args_str.join(", "));
            // Wrap Int32 function calls with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&call)
            } else {
                call
            }
        }
        TypedExpr::UnaryOp { op, expr, ty } => {
            let inner = codegen(expr);
            let result = match op {
                UnaryOp::Neg => format!("(-({}))", inner),
            };
            // Wrap Int32 unary ops with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&result)
            } else {
                result
            }
        }
        TypedExpr::BinOp {
            op,
            left,
            right,
            ty,
        } => {
            let l = codegen(left);
            let r = codegen(right);
            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Eq => "===",
                BinOp::Ne => "!==",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
            };
            let result = format!("(({}) {} ({}))", l, op_str, r);
            // Wrap Int32 operations with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&result)
            } else {
                result
            }
        }
        TypedExpr::Block { bindings, result } => {
            // Generate IIFE for proper scoping
            let mut parts = Vec::new();
            parts.push("(function() {".to_string());

            for binding in bindings {
                let value_code = codegen(&binding.value);
                parts.push(format!("const {} = {};", binding.name, value_code));
            }

            let result_code = codegen(result);
            parts.push(format!("return {};", result_code));
            parts.push("})()".to_string());

            parts.join(" ")
        }
        TypedExpr::Match { scrutinee, arms, .. } => {
            codegen_match(scrutinee, arms)
        }
        TypedExpr::MethodCall {
            receiver,
            method,
            args,
            ty,
        } => {
            let receiver_code = codegen(receiver);
            let receiver_ty = receiver.ty();
            let args_code: Vec<String> = args.iter().map(codegen).collect();

            let result = match method.as_str() {
                // String methods
                "len" => format!("({}).length", receiver_code),
                "is_empty" => format!("(({}).length === 0)", receiver_code),
                "contains" => format!("({}).includes({})", receiver_code, args_code[0]),
                "starts_with" => format!("({}).startsWith({})", receiver_code, args_code[0]),
                "ends_with" => format!("({}).endsWith({})", receiver_code, args_code[0]),
                "to_uppercase" => format!("({}).toUpperCase()", receiver_code),
                "to_lowercase" => format!("({}).toLowerCase()", receiver_code),
                "trim" => format!("({}).trim()", receiver_code),

                // Numeric methods - Int64 needs special handling (no Math functions for BigInt)
                "abs" => match receiver_ty {
                    Type::Int64 => format!("((x) => x < 0n ? -x : x)({})", receiver_code),
                    _ => format!("Math.abs({})", receiver_code),
                },
                "min" => match receiver_ty {
                    Type::Int64 => {
                        format!("((a, b) => a < b ? a : b)({}, {})", receiver_code, args_code[0])
                    }
                    _ => format!("Math.min({}, {})", receiver_code, args_code[0]),
                },
                "max" => match receiver_ty {
                    Type::Int64 => {
                        format!("((a, b) => a > b ? a : b)({}, {})", receiver_code, args_code[0])
                    }
                    _ => format!("Math.max({}, {})", receiver_code, args_code[0]),
                },

                // Type conversion
                "to_string" => format!("String({})", receiver_code),
                "to_float" => receiver_code, // JS numbers are already floats
                "to_int" => format!("Math.trunc({})", receiver_code),

                // Float-specific math
                "floor" => format!("Math.floor({})", receiver_code),
                "ceil" => format!("Math.ceil({})", receiver_code),
                "round" => format!("Math.round({})", receiver_code),
                "sqrt" => format!("Math.sqrt({})", receiver_code),

                _ => panic!("unknown method in codegen: {}", method),
            };

            // Wrap Int32 results with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&result)
            } else {
                result
            }
        }
    }
}

/// Generate JS code for a match expression
fn codegen_match(scrutinee: &TypedExpr, arms: &[TypedMatchArm]) -> String {
    let scrutinee_code = codegen(scrutinee);
    let mut parts = Vec::new();

    parts.push("(function($match) {".to_string());

    for arm in arms {
        match &arm.pattern {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "if ($match === {}) {{ return {}; }}",
                    lit_code, result_code
                ));
            }
            TypedPattern::Var { name, .. } => {
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "{{ const {} = $match; return {}; }}",
                    name, result_code
                ));
            }
            TypedPattern::Wildcard => {
                let result_code = codegen(&arm.result);
                parts.push(format!("return {};", result_code));
            }
        }
    }

    parts.push(format!("}})({})", scrutinee_code));
    parts.join(" ")
}

/// Wrap an Int32 expression with overflow checking
fn wrap_int32_overflow(expr: &str) -> String {
    // Check for non-finite (Infinity/NaN from division by zero) first,
    // then check for overflow
    format!(
        "(function(r){{if(!Number.isFinite(r))throw new Error(\"division by zero\");if(r>2147483647||r<-2147483648)throw new Error(\"Int32 overflow\");return r;}})({})",
        expr
    )
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

/// Generate JS code for a REPL let binding
pub fn codegen_let(binding: &TypedLetBinding) -> String {
    let value_code = codegen(&binding.value);
    // Use var for REPL to allow redefinition and global scope
    format!("var {} = {};", binding.name, value_code)
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

fn escape_js_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int32_wrap(expr: &str) -> String {
        format!(
            "(function(r){{if(!Number.isFinite(r))throw new Error(\"division by zero\");if(r>2147483647||r<-2147483648)throw new Error(\"Int32 overflow\");return r;}})({})",
            expr
        )
    }

    #[test]
    fn test_codegen_int32() {
        let expr = TypedExpr::Int32(42);
        assert_eq!(codegen(&expr), "42");
    }

    #[test]
    fn test_codegen_negative_int32() {
        let expr = TypedExpr::Int32(-42);
        assert_eq!(codegen(&expr), "-42");
    }

    #[test]
    fn test_codegen_int64() {
        let expr = TypedExpr::Int64(42);
        assert_eq!(codegen(&expr), "42n");
    }

    #[test]
    fn test_codegen_int64_large() {
        let expr = TypedExpr::Int64(9_000_000_000);
        assert_eq!(codegen(&expr), "9000000000n");
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
    fn test_codegen_unary_neg_int32() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int32(42)),
            ty: Type::Int32,
        };
        // Int32 gets overflow wrapped
        assert_eq!(codegen(&expr), int32_wrap("(-(42))"));
    }

    #[test]
    fn test_codegen_unary_neg_int64() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int64(42)),
            ty: Type::Int64,
        };
        // Int64 does not get overflow wrapped
        assert_eq!(codegen(&expr), "(-(42n))");
    }

    #[test]
    fn test_codegen_addition_int32() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int32(1)),
            right: Box::new(TypedExpr::Int32(2)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((1) + (2))"));
    }

    #[test]
    fn test_codegen_addition_int64() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int64(1)),
            right: Box::new(TypedExpr::Int64(2)),
            ty: Type::Int64,
        };
        // Int64 (BigInt) does not get overflow wrapped
        assert_eq!(codegen(&expr), "((1n) + (2n))");
    }

    #[test]
    fn test_codegen_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int32(5)),
            right: Box::new(TypedExpr::Int32(3)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((5) - (3))"));
    }

    #[test]
    fn test_codegen_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int32(3)),
            right: Box::new(TypedExpr::Int32(4)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((3) * (4))"));
    }

    #[test]
    fn test_codegen_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int32(10)),
            right: Box::new(TypedExpr::Int32(2)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((10) / (2))"));
    }

    #[test]
    fn test_codegen_complex_expression() {
        // 2 + 3 * 4 - nested Int32 ops each get wrapped
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int32(2)),
            right: Box::new(TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Int32(3)),
                right: Box::new(TypedExpr::Int32(4)),
                ty: Type::Int32,
            }),
            ty: Type::Int32,
        };
        let inner = int32_wrap("((3) * (4))");
        let expected = int32_wrap(&format!("((2) + ({}))", inner));
        assert_eq!(codegen(&expr), expected);
    }

    #[test]
    fn test_codegen_float_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        // Float does not get overflow wrapped
        assert_eq!(codegen(&expr), "((1.5) + (2.5))");
    }

    #[test]
    fn test_codegen_var() {
        let expr = TypedExpr::Var {
            name: "x".to_string(),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), "x");
    }

    #[test]
    fn test_codegen_call_no_args() {
        let expr = TypedExpr::Call {
            func: "foo".to_string(),
            args: vec![],
            ty: Type::Int32,
        };
        // Int32 function calls get overflow wrapped
        assert_eq!(codegen(&expr), int32_wrap("foo()"));
    }

    #[test]
    fn test_codegen_call_one_arg() {
        let expr = TypedExpr::Call {
            func: "square".to_string(),
            args: vec![TypedExpr::Int32(5)],
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("square(5)"));
    }

    #[test]
    fn test_codegen_call_multiple_args() {
        let expr = TypedExpr::Call {
            func: "add".to_string(),
            args: vec![TypedExpr::Int32(1), TypedExpr::Int32(2)],
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("add(1, 2)"));
    }

    #[test]
    fn test_codegen_call_with_vars() {
        let expr = TypedExpr::Call {
            func: "add".to_string(),
            args: vec![
                TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                },
                TypedExpr::Var {
                    name: "y".to_string(),
                    ty: Type::Int32,
                },
            ],
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("add(x, y)"));
    }

    #[test]
    fn test_codegen_var_in_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Var {
                name: "x".to_string(),
                ty: Type::Int32,
            }),
            right: Box::new(TypedExpr::Int32(1)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((x) + (1))"));
    }

    #[test]
    fn test_codegen_function() {
        let func = TypedFunction {
            name: "square".to_string(),
            params: vec![("x".to_string(), Type::Int32)],
            body: TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                }),
                right: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                }),
                ty: Type::Int32,
            },
            return_type: Type::Int32,
        };
        let body = int32_wrap("((x) * (x))");
        assert_eq!(
            codegen_function(&func),
            format!("function square(x) {{ return {}; }}", body)
        );
    }

    #[test]
    fn test_codegen_function_multiple_params() {
        let func = TypedFunction {
            name: "add".to_string(),
            params: vec![
                ("x".to_string(), Type::Int32),
                ("y".to_string(), Type::Int32),
            ],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                }),
                right: Box::new(TypedExpr::Var {
                    name: "y".to_string(),
                    ty: Type::Int32,
                }),
                ty: Type::Int32,
            },
            return_type: Type::Int32,
        };
        let body = int32_wrap("((x) + (y))");
        assert_eq!(
            codegen_function(&func),
            format!("function add(x, y) {{ return {}; }}", body)
        );
    }

    #[test]
    fn test_codegen_function_no_params() {
        let func = TypedFunction {
            name: "answer".to_string(),
            params: vec![],
            body: TypedExpr::Int32(42),
            return_type: Type::Int32,
        };
        assert_eq!(
            codegen_function(&func),
            "function answer() { return 42; }"
        );
    }

    #[test]
    fn test_codegen_int64_function() {
        let func = TypedFunction {
            name: "big".to_string(),
            params: vec![("x".to_string(), Type::Int64)],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int64,
                }),
                right: Box::new(TypedExpr::Int64(1)),
                ty: Type::Int64,
            },
            return_type: Type::Int64,
        };
        // Int64 does not get overflow wrapped
        assert_eq!(
            codegen_function(&func),
            "function big(x) { return ((x) + (1n)); }"
        );
    }
}
