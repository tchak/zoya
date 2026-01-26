use std::collections::HashMap;

use crate::ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, MatchArm, Pattern, Statement, TypeAnnotation,
    UnaryOp,
};
use crate::ir::{TypedExpr, TypedFunction, TypedLetBinding, TypedMatchArm, TypedPattern};
use crate::types::{FunctionType, Type, TypeError};

/// Type environment for checking expressions
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// Function signatures
    pub functions: HashMap<String, FunctionType>,
    /// Local variable types (parameters in function bodies)
    pub locals: HashMap<String, Type>,
}

impl TypeEnv {
    pub fn with_locals(&self, locals: HashMap<String, Type>) -> Self {
        TypeEnv {
            functions: self.functions.clone(),
            locals,
        }
    }
}

/// Resolve a type annotation to a concrete Type
fn resolve_type_annotation(
    annotation: &TypeAnnotation,
    type_params: &[String],
) -> Result<Type, TypeError> {
    match annotation {
        TypeAnnotation::Named(name) => {
            if name == "Int32" {
                Ok(Type::Int32)
            } else if name == "Int64" {
                Ok(Type::Int64)
            } else if name == "Float" {
                Ok(Type::Float)
            } else if name == "Bool" {
                Ok(Type::Bool)
            } else if name == "String" {
                Ok(Type::String)
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
fn check_function(func: &FunctionDef, env: &TypeEnv) -> Result<TypedFunction, TypeError> {
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
fn function_type_from_def(func: &FunctionDef) -> Result<FunctionType, TypeError> {
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
        (Type::Int32, Type::Int32) => true,
        (Type::Int64, Type::Int64) => true,
        (Type::Float, Type::Float) => true,
        (Type::Bool, Type::Bool) => true,
        (Type::String, Type::String) => true,
        (Type::Var(a), Type::Var(b)) => a == b,
        // Type variables can match any concrete type during instantiation
        (_, Type::Var(_)) => true,
        (Type::Var(_), _) => true,
        _ => false,
    }
}

/// Check if a type is numeric (for ordering comparisons)
fn is_numeric_type(ty: &Type) -> bool {
    matches!(ty, Type::Int32 | Type::Int64 | Type::Float)
}

/// Check an expression with a type environment
fn check_with_env(expr: &Expr, env: &TypeEnv) -> Result<TypedExpr, TypeError> {
    match expr {
        Expr::Int(n) => {
            // Default to Int32 if value fits, otherwise error
            if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                Ok(TypedExpr::Int32(*n as i32))
            } else {
                Err(TypeError {
                    message: format!(
                        "integer literal {} is too large for Int32 (max: {})",
                        n,
                        i32::MAX
                    ),
                })
            }
        }
        Expr::Float(n) => Ok(TypedExpr::Float(*n)),
        Expr::Bool(b) => Ok(TypedExpr::Bool(*b)),
        Expr::String(s) => Ok(TypedExpr::String(s.clone())),

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

            // Determine result type based on operator
            let result_ty = match op {
                // Arithmetic operators: result has same type as operands
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => left_ty,

                // Equality operators: work on any type, result is Bool
                BinOp::Eq | BinOp::Ne => Type::Bool,

                // Ordering operators: only work on numeric types, result is Bool
                BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    if !is_numeric_type(&left_ty) {
                        return Err(TypeError {
                            message: format!(
                                "ordering operators only work on numeric types, not {}",
                                left_ty
                            ),
                        });
                    }
                    Type::Bool
                }
            };

            Ok(TypedExpr::BinOp {
                op: *op,
                left: Box::new(typed_left),
                right: Box::new(typed_right),
                ty: result_ty,
            })
        }

        Expr::Block { bindings, result } => {
            // Create new environment for block scope
            let mut block_env = env.clone();
            let mut typed_bindings = Vec::new();

            for binding in bindings {
                let typed_binding = check_let_binding(binding, &block_env)?;
                // Add binding to environment for subsequent bindings
                block_env
                    .locals
                    .insert(binding.name.clone(), typed_binding.ty.clone());
                typed_bindings.push(typed_binding);
            }

            // Type-check the result expression with all bindings in scope
            let typed_result = check_with_env(result, &block_env)?;

            Ok(TypedExpr::Block {
                bindings: typed_bindings,
                result: Box::new(typed_result),
            })
        }

        Expr::Match { scrutinee, arms } => {
            let typed_scrutinee = check_with_env(scrutinee, env)?;
            let scrutinee_ty = typed_scrutinee.ty();

            if arms.is_empty() {
                return Err(TypeError {
                    message: "match expression must have at least one arm".to_string(),
                });
            }

            let mut typed_arms = Vec::new();
            let mut result_ty: Option<Type> = None;

            for arm in arms {
                let typed_arm = check_match_arm(arm, &scrutinee_ty, env)?;
                let arm_ty = typed_arm.result.ty();

                // Verify all arms have same result type
                match &result_ty {
                    None => result_ty = Some(arm_ty),
                    Some(ty) if *ty != arm_ty => {
                        return Err(TypeError {
                            message: format!(
                                "match arms have different types: {} vs {}",
                                ty, arm_ty
                            ),
                        });
                    }
                    _ => {}
                }

                typed_arms.push(typed_arm);
            }

            Ok(TypedExpr::Match {
                scrutinee: Box::new(typed_scrutinee),
                arms: typed_arms,
                ty: result_ty.unwrap(),
            })
        }
    }
}

/// Check a pattern and return typed pattern with any bindings it introduces
fn check_pattern(
    pattern: &Pattern,
    scrutinee_ty: &Type,
    env: &TypeEnv,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    match pattern {
        Pattern::Literal(expr) => {
            let typed = check_with_env(expr, env)?;
            let lit_ty = typed.ty();
            if lit_ty != *scrutinee_ty {
                return Err(TypeError {
                    message: format!(
                        "pattern type {} does not match scrutinee type {}",
                        lit_ty, scrutinee_ty
                    ),
                });
            }
            Ok((TypedPattern::Literal(typed), HashMap::new()))
        }
        Pattern::Var(name) => {
            let mut bindings = HashMap::new();
            bindings.insert(name.clone(), scrutinee_ty.clone());
            Ok((
                TypedPattern::Var {
                    name: name.clone(),
                    ty: scrutinee_ty.clone(),
                },
                bindings,
            ))
        }
        Pattern::Wildcard => Ok((TypedPattern::Wildcard, HashMap::new())),
    }
}

/// Check a match arm
fn check_match_arm(
    arm: &MatchArm,
    scrutinee_ty: &Type,
    env: &TypeEnv,
) -> Result<TypedMatchArm, TypeError> {
    let (typed_pattern, bindings) = check_pattern(&arm.pattern, scrutinee_ty, env)?;

    // Create arm environment with pattern bindings
    let mut arm_env = env.clone();
    arm_env.locals.extend(bindings);

    let typed_result = check_with_env(&arm.result, &arm_env)?;

    Ok(TypedMatchArm {
        pattern: typed_pattern,
        result: typed_result,
    })
}

/// Check a let binding and return a typed let binding
fn check_let_binding(binding: &LetBinding, env: &TypeEnv) -> Result<TypedLetBinding, TypeError> {
    let typed_value = check_with_env(&binding.value, env)?;
    let inferred_type = typed_value.ty();

    // If type annotation exists, verify it matches
    let binding_type = if let Some(ref annotation) = binding.type_annotation {
        let declared_type = resolve_type_annotation(annotation, &[])?;
        if !types_compatible(&inferred_type, &declared_type) {
            return Err(TypeError {
                message: format!(
                    "let binding '{}' declares type {} but value has type {}",
                    binding.name, declared_type, inferred_type
                ),
            });
        }
        declared_type
    } else {
        inferred_type
    };

    Ok(TypedLetBinding {
        name: binding.name.clone(),
        value: typed_value,
        ty: binding_type,
    })
}

/// Check a file's items (functions), returning typed functions
pub fn check_file(items: &[Item]) -> Result<Vec<TypedFunction>, TypeError> {
    // Build type environment with all function signatures first
    let mut env = TypeEnv::default();
    for item in items {
        let Item::Function(func) = item;
        let func_type = function_type_from_def(func)?;
        env.functions.insert(func.name.clone(), func_type);
    }

    // Type-check all functions
    let mut typed_functions = Vec::new();
    for item in items {
        let Item::Function(func) = item;
        let typed = check_function(func, &env)?;
        typed_functions.push(typed);
    }

    Ok(typed_functions)
}

/// Type-checked statement result for REPL
#[derive(Debug, Clone, PartialEq)]
pub enum CheckedStatement {
    Function(TypedFunction),
    Expr(TypedExpr),
    Let(TypedLetBinding),
}

/// Check REPL statements, updating env for items, returning checked results
pub fn check_repl(
    statements: &[Statement],
    env: &mut TypeEnv,
) -> Result<Vec<CheckedStatement>, TypeError> {
    let mut results = Vec::new();

    for statement in statements {
        match statement {
            Statement::Item(Item::Function(func)) => {
                // Add function type to environment first
                let func_type = function_type_from_def(func)?;
                env.functions.insert(func.name.clone(), func_type);

                // Type-check the function
                let typed_func = check_function(func, env)?;
                results.push(CheckedStatement::Function(typed_func));
            }
            Statement::Expr(expr) => {
                let typed_expr = check_with_env(expr, env)?;
                results.push(CheckedStatement::Expr(typed_expr));
            }
            Statement::Let(binding) => {
                let typed_binding = check_let_binding(binding, env)?;
                // Add to environment for future statements
                env.locals
                    .insert(binding.name.clone(), typed_binding.ty.clone());
                results.push(CheckedStatement::Let(typed_binding));
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;
    use crate::types::Type;

    fn check(expr: &Expr) -> Result<TypedExpr, TypeError> {
        check_with_env(expr, &TypeEnv::default())
    }

    #[test]
    fn test_check_int() {
        let expr = Expr::Int(42);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int32);
        assert_eq!(result, TypedExpr::Int32(42));
    }

    #[test]
    fn test_check_int_too_large() {
        let expr = Expr::Int(3_000_000_000); // Exceeds i32::MAX
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("too large for Int32"));
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
        assert_eq!(result.ty(), Type::Int32);
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
        assert_eq!(result.ty(), Type::Int32);
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
        assert_eq!(result.ty(), Type::Int32);
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
        assert_eq!(result.ty(), Type::Int32);
    }

    use crate::ast::{FunctionDef, Param, TypeAnnotation};
    use crate::types::FunctionType;

    #[test]
    fn test_check_variable() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let expr = Expr::Var("x".to_string());
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int32);
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
        env.locals.insert("x".to_string(), Type::Int32);
        env.locals.insert("y".to_string(), Type::Int32);

        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Var("y".to_string())),
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_function_call() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int32],
                return_type: Type::Int32,
            },
        );

        let expr = Expr::Call {
            func: "square".to_string(),
            args: vec![Expr::Int(5)],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_function_call_wrong_arg_type() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int32],
                return_type: Type::Int32,
            },
        );

        let expr = Expr::Call {
            func: "square".to_string(),
            args: vec![Expr::Float(5.0)],
        };
        let result = check_with_env(&expr, &env);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("argument type mismatch")
        );
    }

    #[test]
    fn test_check_function_call_wrong_arity() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "add".to_string(),
            FunctionType {
                type_params: vec![],
                params: vec![Type::Int32, Type::Int32],
                return_type: Type::Int32,
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

        // identity(42) should return Int32
        let expr = Expr::Call {
            func: "identity".to_string(),
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int32);
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
                typ: TypeAnnotation::Named("Int32".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Int32".to_string())),
            body: Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Var("x".to_string())),
            },
        };

        let result = check_function(&func, &env).unwrap();
        assert_eq!(result.name, "double");
        assert_eq!(result.return_type, Type::Int32);
    }

    #[test]
    fn test_check_function_def_return_type_mismatch() {
        let env = TypeEnv::default();
        let func = FunctionDef {
            name: "wrong".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "x".to_string(),
                typ: TypeAnnotation::Named("Int32".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Float".to_string())),
            body: Expr::Var("x".to_string()), // Returns Int32, not Float
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
                params: vec![Type::Int32, Type::Int32],
                return_type: Type::Int32,
            },
        );

        let func = FunctionDef {
            name: "double".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "x".to_string(),
                typ: TypeAnnotation::Named("Int32".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Int32".to_string())),
            body: Expr::Call {
                func: "add".to_string(),
                args: vec![Expr::Var("x".to_string()), Expr::Var("x".to_string())],
            },
        };

        let result = check_function(&func, &env).unwrap();
        assert_eq!(result.return_type, Type::Int32);
    }

    #[test]
    fn test_function_type_from_def() {
        let func = FunctionDef {
            name: "add".to_string(),
            type_params: vec![],
            params: vec![
                Param {
                    name: "x".to_string(),
                    typ: TypeAnnotation::Named("Int32".to_string()),
                },
                Param {
                    name: "y".to_string(),
                    typ: TypeAnnotation::Named("Int32".to_string()),
                },
            ],
            return_type: Some(TypeAnnotation::Named("Int32".to_string())),
            body: Expr::Int(0), // body doesn't matter for type extraction
        };

        let ft = function_type_from_def(&func).unwrap();
        assert_eq!(ft.params, vec![Type::Int32, Type::Int32]);
        assert_eq!(ft.return_type, Type::Int32);
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

    #[test]
    fn test_check_bool_true() {
        let expr = Expr::Bool(true);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
        assert_eq!(result, TypedExpr::Bool(true));
    }

    #[test]
    fn test_check_bool_false() {
        let expr = Expr::Bool(false);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
        assert_eq!(result, TypedExpr::Bool(false));
    }

    #[test]
    fn test_check_equality_int() {
        let expr = Expr::BinOp {
            op: BinOp::Eq,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_inequality_int() {
        let expr = Expr::BinOp {
            op: BinOp::Ne,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_equality_bool() {
        let expr = Expr::BinOp {
            op: BinOp::Eq,
            left: Box::new(Expr::Bool(true)),
            right: Box::new(Expr::Bool(false)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_less_than_int() {
        let expr = Expr::BinOp {
            op: BinOp::Lt,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_greater_than_float() {
        let expr = Expr::BinOp {
            op: BinOp::Gt,
            left: Box::new(Expr::Float(1.5)),
            right: Box::new(Expr::Float(2.5)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_less_equal_int() {
        let expr = Expr::BinOp {
            op: BinOp::Le,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_greater_equal_int() {
        let expr = Expr::BinOp {
            op: BinOp::Ge,
            left: Box::new(Expr::Int(1)),
            right: Box::new(Expr::Int(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_ordering_on_bool_error() {
        let expr = Expr::BinOp {
            op: BinOp::Lt,
            left: Box::new(Expr::Bool(true)),
            right: Box::new(Expr::Bool(false)),
        };
        let result = check(&expr);
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
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_check_repl_single_expr() {
        let mut env = TypeEnv::default();
        let stmts = vec![Statement::Expr(Expr::Int(42))];
        let result = check_repl(&stmts, &mut env).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(
            result[0],
            CheckedStatement::Expr(TypedExpr::Int32(42))
        ));
    }

    #[test]
    fn test_check_repl_function_def() {
        let mut env = TypeEnv::default();
        let stmts = vec![Statement::Item(Item::Function(FunctionDef {
            name: "foo".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named("Int32".to_string())),
            body: Expr::Int(42),
        }))];
        let result = check_repl(&stmts, &mut env).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], CheckedStatement::Function(_)));
        // Function should be added to env
        assert!(env.functions.contains_key("foo"));
    }

    #[test]
    fn test_check_repl_function_then_call() {
        let mut env = TypeEnv::default();
        let stmts = vec![
            Statement::Item(Item::Function(FunctionDef {
                name: "double".to_string(),
                type_params: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    typ: TypeAnnotation::Named("Int32".to_string()),
                }],
                return_type: Some(TypeAnnotation::Named("Int32".to_string())),
                body: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Var("x".to_string())),
                    right: Box::new(Expr::Var("x".to_string())),
                },
            })),
            Statement::Expr(Expr::Call {
                func: "double".to_string(),
                args: vec![Expr::Int(5)],
            }),
        ];
        let result = check_repl(&stmts, &mut env).unwrap();
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], CheckedStatement::Function(_)));
        assert!(matches!(result[1], CheckedStatement::Expr(_)));
    }

    #[test]
    fn test_check_repl_let_binding() {
        let mut env = TypeEnv::default();
        let stmts = vec![Statement::Let(LetBinding {
            name: "x".to_string(),
            type_annotation: None,
            value: Box::new(Expr::Int(42)),
        })];
        let result = check_repl(&stmts, &mut env).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], CheckedStatement::Let(_)));
        // Variable should be added to env
        assert_eq!(env.locals.get("x"), Some(&Type::Int32));
    }

    #[test]
    fn test_check_repl_let_then_use() {
        let mut env = TypeEnv::default();
        let stmts = vec![
            Statement::Let(LetBinding {
                name: "x".to_string(),
                type_annotation: None,
                value: Box::new(Expr::Int(42)),
            }),
            Statement::Expr(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Int(1)),
            }),
        ];
        let result = check_repl(&stmts, &mut env).unwrap();
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], CheckedStatement::Let(_)));
        assert!(matches!(result[1], CheckedStatement::Expr(_)));
    }

    #[test]
    fn test_check_let_with_type_annotation() {
        let mut env = TypeEnv::default();
        let stmts = vec![Statement::Let(LetBinding {
            name: "x".to_string(),
            type_annotation: Some(TypeAnnotation::Named("Int32".to_string())),
            value: Box::new(Expr::Int(42)),
        })];
        let result = check_repl(&stmts, &mut env).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(env.locals.get("x"), Some(&Type::Int32));
    }

    #[test]
    fn test_check_let_type_mismatch() {
        let mut env = TypeEnv::default();
        let stmts = vec![Statement::Let(LetBinding {
            name: "x".to_string(),
            type_annotation: Some(TypeAnnotation::Named("Float".to_string())),
            value: Box::new(Expr::Int(42)),
        })];
        let result = check_repl(&stmts, &mut env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("declares type"));
    }

    #[test]
    fn test_check_block_expression() {
        let expr = Expr::Block {
            bindings: vec![LetBinding {
                name: "x".to_string(),
                type_annotation: None,
                value: Box::new(Expr::Int(1)),
            }],
            result: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Int(2)),
            }),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_block_multiple_bindings() {
        let expr = Expr::Block {
            bindings: vec![
                LetBinding {
                    name: "x".to_string(),
                    type_annotation: None,
                    value: Box::new(Expr::Int(1)),
                },
                LetBinding {
                    name: "y".to_string(),
                    type_annotation: None,
                    value: Box::new(Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Var("x".to_string())),
                        right: Box::new(Expr::Int(1)),
                    }),
                },
            ],
            result: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Var("y".to_string())),
            }),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    use crate::ast::{MatchArm, Pattern};

    #[test]
    fn test_check_match_with_literals() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".to_string())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                    result: Expr::String("zero".to_string()),
                },
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(1))),
                    result: Expr::String("one".to_string()),
                },
            ],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::String);
    }

    #[test]
    fn test_check_match_with_wildcard() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".to_string())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                    result: Expr::Int(1),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    result: Expr::Int(2),
                },
            ],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_match_with_variable_binding() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Var("n".to_string()),
                result: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Var("n".to_string())),
                    right: Box::new(Expr::Int(1)),
                },
            }],
        };
        let result = check_with_env(&expr, &env).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_match_pattern_type_mismatch() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::String("hello".to_string()))),
                result: Expr::Int(1),
            }],
        };
        let result = check_with_env(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("does not match scrutinee"));
    }

    #[test]
    fn test_check_match_arm_type_mismatch() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".to_string())),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                    result: Expr::String("zero".to_string()),
                },
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(1))),
                    result: Expr::Int(1), // Type mismatch: String vs Int32
                },
            ],
        };
        let result = check_with_env(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("different types"));
    }
}
