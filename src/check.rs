use std::collections::HashMap;

use crate::ast::{Expr, FunctionDef, TypeAnnotation, UnaryOp};
use crate::ir::{TypedExpr, TypedFunction};
use crate::types::{FunctionType, Type, TypeError};

/// Check an expression without any environment (for simple REPL expressions)
pub fn check(expr: &Expr) -> Result<TypedExpr, TypeError> {
    check_with_env(expr, &TypeEnv::default())
}

/// Type environment for checking expressions
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// Function signatures
    pub functions: HashMap<String, FunctionType>,
    /// Local variable types (parameters in function bodies)
    pub locals: HashMap<String, Type>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_locals(&self, locals: HashMap<String, Type>) -> Self {
        TypeEnv {
            functions: self.functions.clone(),
            locals,
        }
    }

    pub fn add_function(&mut self, name: String, func_type: FunctionType) {
        self.functions.insert(name, func_type);
    }
}

/// Resolve a type annotation to a concrete Type
fn resolve_type_annotation(
    annotation: &TypeAnnotation,
    type_params: &[String],
) -> Result<Type, TypeError> {
    match annotation {
        TypeAnnotation::Named(name) => {
            if name == "Int" {
                Ok(Type::Int)
            } else if name == "Float" {
                Ok(Type::Float)
            } else if type_params.contains(name) {
                Ok(Type::Var(name.clone()))
            } else {
                Err(TypeError {
                    message: format!("unknown type: {}", name),
                })
            }
        }
    }
}

/// Check a function definition and return a typed function
pub fn check_function(func: &FunctionDef, env: &TypeEnv) -> Result<TypedFunction, TypeError> {
    // Build local environment with parameters
    let mut locals = HashMap::new();
    let mut param_types = Vec::new();

    for param in &func.params {
        let ty = resolve_type_annotation(&param.typ, &func.type_params)?;
        locals.insert(param.name.clone(), ty.clone());
        param_types.push(ty);
    }

    // Create environment with locals for checking body
    let body_env = env.with_locals(locals);

    // Check the body
    let typed_body = check_with_env(&func.body, &body_env)?;
    let body_type = typed_body.ty();

    // Determine return type
    let return_type = if let Some(ref annotation) = func.return_type {
        let declared_return = resolve_type_annotation(annotation, &func.type_params)?;
        // Verify body type matches declared return type
        if !types_compatible(&body_type, &declared_return) {
            return Err(TypeError {
                message: format!(
                    "function '{}' declares return type {} but body has type {}",
                    func.name, declared_return, body_type
                ),
            });
        }
        declared_return
    } else {
        // Infer return type from body
        body_type.clone()
    };

    Ok(TypedFunction {
        name: func.name.clone(),
        params: func
            .params
            .iter()
            .zip(param_types.iter())
            .map(|(p, t)| (p.name.clone(), t.clone()))
            .collect(),
        body: typed_body,
        return_type,
    })
}

/// Extract function type from a function definition (for adding to env)
pub fn function_type_from_def(func: &FunctionDef) -> Result<FunctionType, TypeError> {
    let mut param_types = Vec::new();
    for param in &func.params {
        let ty = resolve_type_annotation(&param.typ, &func.type_params)?;
        param_types.push(ty);
    }

    let return_type = if let Some(ref annotation) = func.return_type {
        resolve_type_annotation(annotation, &func.type_params)?
    } else {
        // For now, if no return type is specified, we need to infer it
        // This will be determined when checking the body
        Type::Var("_inferred".to_string())
    };

    Ok(FunctionType {
        type_params: func.type_params.clone(),
        params: param_types,
        return_type,
    })
}

/// Check if two types are compatible (for type checking)
fn types_compatible(actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
        (Type::Int, Type::Int) => true,
        (Type::Float, Type::Float) => true,
        (Type::Var(a), Type::Var(b)) => a == b,
        // Type variables can match any concrete type during instantiation
        (_, Type::Var(_)) => true,
        (Type::Var(_), _) => true,
        _ => false,
    }
}

/// Check an expression with a type environment
pub fn check_with_env(expr: &Expr, env: &TypeEnv) -> Result<TypedExpr, TypeError> {
    match expr {
        Expr::Int(n) => Ok(TypedExpr::Int(*n)),
        Expr::Float(n) => Ok(TypedExpr::Float(*n)),

        Expr::Var(name) => {
            if let Some(ty) = env.locals.get(name) {
                Ok(TypedExpr::Var {
                    name: name.clone(),
                    ty: ty.clone(),
                })
            } else {
                Err(TypeError {
                    message: format!("unknown variable: {}", name),
                })
            }
        }

        Expr::Call { func, args } => {
            // Look up function
            let func_type = env.functions.get(func).ok_or_else(|| TypeError {
                message: format!("unknown function: {}", func),
            })?;

            // Check argument count
            if args.len() != func_type.params.len() {
                return Err(TypeError {
                    message: format!(
                        "function '{}' expects {} arguments, got {}",
                        func,
                        func_type.params.len(),
                        args.len()
                    ),
                });
            }

            // Type check arguments and build substitutions for generics
            let mut typed_args = Vec::new();
            let mut substitutions: HashMap<String, Type> = HashMap::new();

            for (arg, param_type) in args.iter().zip(func_type.params.iter()) {
                let typed_arg = check_with_env(arg, env)?;
                let arg_type = typed_arg.ty();

                // Handle generic type instantiation
                match param_type {
                    Type::Var(type_var) => {
                        if let Some(existing) = substitutions.get(type_var) {
                            // Type variable already bound, check consistency
                            if !types_compatible(&arg_type, existing) {
                                return Err(TypeError {
                                    message: format!(
                                        "type parameter {} bound to {} but got {}",
                                        type_var, existing, arg_type
                                    ),
                                });
                            }
                        } else {
                            // Bind type variable
                            substitutions.insert(type_var.clone(), arg_type.clone());
                        }
                    }
                    _ => {
                        // Concrete type, check match
                        if !types_compatible(&arg_type, param_type) {
                            return Err(TypeError {
                                message: format!(
                                    "argument type mismatch: expected {}, got {}",
                                    param_type, arg_type
                                ),
                            });
                        }
                    }
                }

                typed_args.push(typed_arg);
            }

            // Instantiate return type with substitutions
            let instantiated = func_type.instantiate(&substitutions);
            let return_type = instantiated.return_type;

            Ok(TypedExpr::Call {
                func: func.clone(),
                args: typed_args,
                ty: return_type,
            })
        }

        Expr::UnaryOp { op, expr } => {
            let typed_expr = check_with_env(expr, env)?;
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
            let typed_left = check_with_env(left, env)?;
            let typed_right = check_with_env(right, env)?;
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

    use crate::ast::{FunctionDef, Param, TypeAnnotation};
    use crate::types::FunctionType;
    use std::collections::HashMap;

    #[test]
    fn test_check_variable() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int);

        let expr = Expr::Var("x".to_string());
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_unknown_variable() {
        let env = TypeEnv::default();
        let expr = Expr::Var("x".to_string());
        let result = check_with_env(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown variable"));
    }

    #[test]
    fn test_check_variable_in_expression() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int);
        env.locals.insert("y".to_string(), Type::Int);

        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Var("y".to_string())),
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_function_call() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int],
                return_type: Type::Int,
            },
        );

        let expr = Expr::Call {
            func: "square".to_string(),
            args: vec![Expr::Int(5)],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_function_call_wrong_arg_type() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int],
                return_type: Type::Int,
            },
        );

        let expr = Expr::Call {
            func: "square".to_string(),
            args: vec![Expr::Float(5.0)],
        };
        let result = check_with_env(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("argument type mismatch"));
    }

    #[test]
    fn test_check_function_call_wrong_arity() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "add".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int, Type::Int],
                return_type: Type::Int,
            },
        );

        let expr = Expr::Call {
            func: "add".to_string(),
            args: vec![Expr::Int(1)],
        };
        let result = check_with_env(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("expects 2 arguments"));
    }

    #[test]
    fn test_check_generic_function_call() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "identity".to_string(),
            FunctionType {
                type_params: vec!["T".to_string()],
                params: vec![Type::Var("T".to_string())],
                return_type: Type::Var("T".to_string()),
            },
        );

        // identity(42) should return Int
        let expr = Expr::Call {
            func: "identity".to_string(),
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_generic_function_call_float() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "identity".to_string(),
            FunctionType {
                type_params: vec!["T".to_string()],
                params: vec![Type::Var("T".to_string())],
                return_type: Type::Var("T".to_string()),
            },
        );

        // identity(3.14) should return Float
        let expr = Expr::Call {
            func: "identity".to_string(),
            args: vec![Expr::Float(3.14)],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Float);
    }

    #[test]
    fn test_check_function_def() {
        let env = TypeEnv::default();
        let func = FunctionDef {
            name: "double".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "x".to_string(),
                typ: TypeAnnotation::Named("Int".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Int".to_string())),
            body: Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Var("x".to_string())),
            },
        };

        let result = check_function(&func, &env).unwrap();
        assert_eq!(result.name, "double");
        assert_eq!(result.return_type, Type::Int);
    }

    #[test]
    fn test_check_function_def_return_type_mismatch() {
        let env = TypeEnv::default();
        let func = FunctionDef {
            name: "wrong".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "x".to_string(),
                typ: TypeAnnotation::Named("Int".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Float".to_string())),
            body: Expr::Var("x".to_string()), // Returns Int, not Float
        };

        let result = check_function(&func, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("declares return type"));
    }

    #[test]
    fn test_check_function_def_with_call() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "add".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int, Type::Int],
                return_type: Type::Int,
            },
        );

        let func = FunctionDef {
            name: "double".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "x".to_string(),
                typ: TypeAnnotation::Named("Int".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Int".to_string())),
            body: Expr::Call {
                func: "add".to_string(),
                args: vec![
                    Expr::Var("x".to_string()),
                    Expr::Var("x".to_string()),
                ],
            },
        };

        let result = check_function(&func, &env).unwrap();
        assert_eq!(result.return_type, Type::Int);
    }

    #[test]
    fn test_function_type_from_def() {
        let func = FunctionDef {
            name: "add".to_string(),
            type_params: vec![],
            params: vec![
                Param {
                    name: "x".to_string(),
                    typ: TypeAnnotation::Named("Int".to_string()),
                },
                Param {
                    name: "y".to_string(),
                    typ: TypeAnnotation::Named("Int".to_string()),
                },
            ],
            return_type: Some(TypeAnnotation::Named("Int".to_string())),
            body: Expr::Int(0), // body doesn't matter for type extraction
        };

        let ft = function_type_from_def(&func).unwrap();
        assert_eq!(ft.params, vec![Type::Int, Type::Int]);
        assert_eq!(ft.return_type, Type::Int);
    }

    #[test]
    fn test_function_type_from_def_generic() {
        let func = FunctionDef {
            name: "identity".to_string(),
            type_params: vec!["T".to_string()],
            params: vec![Param {
                name: "x".to_string(),
                typ: TypeAnnotation::Named("T".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("T".to_string())),
            body: Expr::Int(0),
        };

        let ft = function_type_from_def(&func).unwrap();
        assert_eq!(ft.type_params, vec!["T".to_string()]);
        assert_eq!(ft.params, vec![Type::Var("T".to_string())]);
        assert_eq!(ft.return_type, Type::Var("T".to_string()));
    }
}
