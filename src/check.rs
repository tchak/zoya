use std::collections::HashMap;

use crate::ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, ListPattern, MatchArm, Pattern, Statement,
    TuplePattern, TypeAnnotation, UnaryOp,
};
use crate::ir::{TypedExpr, TypedFunction, TypedLetBinding, TypedMatchArm, TypedPattern};
use crate::types::{FunctionType, Type, TypeError, TypeVarId};
use crate::unify::UnifyCtx;

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

/// Resolve a type annotation to a concrete Type.
/// `type_param_map` maps source-level type parameter names (like "T") to TypeVarIds.
fn resolve_type_annotation(
    annotation: &TypeAnnotation,
    type_param_map: &HashMap<String, TypeVarId>,
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
            } else if let Some(&id) = type_param_map.get(name) {
                Ok(Type::Var(id))
            } else {
                Err(TypeError {
                    message: format!("unknown type: {}", name),
                })
            }
        }
        TypeAnnotation::Parameterized(name, params) => {
            if name == "List" {
                if params.len() != 1 {
                    return Err(TypeError {
                        message: "List requires exactly one type parameter".to_string(),
                    });
                }
                let elem_type = resolve_type_annotation(&params[0], type_param_map)?;
                Ok(Type::List(Box::new(elem_type)))
            } else {
                Err(TypeError {
                    message: format!("unknown parameterized type: {}", name),
                })
            }
        }
        TypeAnnotation::Tuple(params) => {
            let mut types = Vec::new();
            for param in params {
                types.push(resolve_type_annotation(param, type_param_map)?);
            }
            Ok(Type::Tuple(types))
        }
    }
}

/// Check a function definition and return a typed function
fn check_function(
    func: &FunctionDef,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedFunction, TypeError> {
    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    for name in &func.type_params {
        let var = ctx.fresh_var();
        if let Type::Var(id) = var {
            type_param_map.insert(name.clone(), id);
        }
    }

    // Build local environment with parameters
    let mut locals = HashMap::new();
    let mut param_types = Vec::new();

    for param in &func.params {
        let ty = resolve_type_annotation(&param.typ, &type_param_map)?;
        locals.insert(param.name.clone(), ty.clone());
        param_types.push(ty);
    }

    // Create environment with locals for checking body
    let body_env = env.with_locals(locals);

    // Check the body
    let typed_body = check_with_env(&func.body, &body_env, ctx)?;
    let body_type = ctx.resolve(&typed_body.ty());

    // Determine return type
    let return_type = if let Some(ref annotation) = func.return_type {
        let declared_return = resolve_type_annotation(annotation, &type_param_map)?;
        // Unify body type with declared return type
        ctx.unify(&body_type, &declared_return).map_err(|e| TypeError {
            message: format!(
                "function '{}' declares return type {} but body has type {}: {}",
                func.name,
                ctx.resolve(&declared_return),
                body_type,
                e.message
            ),
        })?;
        ctx.resolve(&declared_return)
    } else {
        // Infer return type from body
        body_type
    };

    Ok(TypedFunction {
        name: func.name.clone(),
        params: func
            .params
            .iter()
            .zip(param_types.iter())
            .map(|(p, t)| (p.name.clone(), ctx.resolve(t)))
            .collect(),
        body: typed_body,
        return_type: ctx.resolve(&return_type),
    })
}

/// Extract function type from a function definition (for adding to env).
/// Uses a separate UnifyCtx to create fresh type variables for the signature.
fn function_type_from_def(
    func: &FunctionDef,
    ctx: &mut UnifyCtx,
) -> Result<FunctionType, TypeError> {
    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    let mut type_var_ids = Vec::new();
    for name in &func.type_params {
        let var = ctx.fresh_var();
        if let Type::Var(id) = var {
            type_param_map.insert(name.clone(), id);
            type_var_ids.push(id);
        }
    }

    let mut param_types = Vec::new();
    for param in &func.params {
        let ty = resolve_type_annotation(&param.typ, &type_param_map)?;
        param_types.push(ty);
    }

    let return_type = if let Some(ref annotation) = func.return_type {
        resolve_type_annotation(annotation, &type_param_map)?
    } else {
        // Create a fresh type variable for inferred return type
        ctx.fresh_var()
    };

    Ok(FunctionType {
        type_params: func.type_params.clone(),
        type_var_ids,
        params: param_types,
        return_type,
    })
}

/// Check if a type is numeric (for ordering comparisons)
fn is_numeric_type(ty: &Type) -> bool {
    matches!(ty, Type::Int32 | Type::Int64 | Type::Float)
}

/// Get the type signature of a built-in method on a type.
/// Returns (parameter_types, return_type) if the method exists.
fn builtin_method(receiver_ty: &Type, method: &str) -> Option<(Vec<Type>, Type)> {
    match (receiver_ty, method) {
        // String methods
        (Type::String, "len") => Some((vec![], Type::Int32)),
        (Type::String, "is_empty") => Some((vec![], Type::Bool)),
        (Type::String, "contains") => Some((vec![Type::String], Type::Bool)),
        (Type::String, "starts_with") => Some((vec![Type::String], Type::Bool)),
        (Type::String, "ends_with") => Some((vec![Type::String], Type::Bool)),
        (Type::String, "to_uppercase") => Some((vec![], Type::String)),
        (Type::String, "to_lowercase") => Some((vec![], Type::String)),
        (Type::String, "trim") => Some((vec![], Type::String)),

        // Int32 methods
        (Type::Int32, "abs") => Some((vec![], Type::Int32)),
        (Type::Int32, "to_string") => Some((vec![], Type::String)),
        (Type::Int32, "to_float") => Some((vec![], Type::Float)),
        (Type::Int32, "min") => Some((vec![Type::Int32], Type::Int32)),
        (Type::Int32, "max") => Some((vec![Type::Int32], Type::Int32)),

        // Int64 methods
        (Type::Int64, "abs") => Some((vec![], Type::Int64)),
        (Type::Int64, "to_string") => Some((vec![], Type::String)),
        (Type::Int64, "min") => Some((vec![Type::Int64], Type::Int64)),
        (Type::Int64, "max") => Some((vec![Type::Int64], Type::Int64)),

        // Float methods
        (Type::Float, "abs") => Some((vec![], Type::Float)),
        (Type::Float, "to_string") => Some((vec![], Type::String)),
        (Type::Float, "to_int") => Some((vec![], Type::Int32)),
        (Type::Float, "floor") => Some((vec![], Type::Float)),
        (Type::Float, "ceil") => Some((vec![], Type::Float)),
        (Type::Float, "round") => Some((vec![], Type::Float)),
        (Type::Float, "sqrt") => Some((vec![], Type::Float)),
        (Type::Float, "min") => Some((vec![Type::Float], Type::Float)),
        (Type::Float, "max") => Some((vec![Type::Float], Type::Float)),

        _ => None,
    }
}

/// Check an expression with a type environment
fn check_with_env(
    expr: &Expr,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
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
                    ty: ctx.resolve(ty),
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

            // Create fresh type variables for this call's type parameters
            let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
            for &old_id in &func_type.type_var_ids {
                let new_var = ctx.fresh_var();
                instantiation.insert(old_id, new_var);
            }

            // Instantiate parameter types with fresh variables
            let instantiated_params: Vec<Type> = func_type
                .params
                .iter()
                .map(|t| substitute_type_vars(t, &instantiation))
                .collect();
            let instantiated_return =
                substitute_type_vars(&func_type.return_type, &instantiation);

            // Type check arguments and unify with parameter types
            let mut typed_args = Vec::new();
            for (arg, param_type) in args.iter().zip(instantiated_params.iter()) {
                let typed_arg = check_with_env(arg, env, ctx)?;
                let arg_type = typed_arg.ty();

                // Unify argument type with parameter type
                ctx.unify(&arg_type, param_type).map_err(|e| TypeError {
                    message: format!(
                        "argument type mismatch in call to '{}': expected {}, got {}: {}",
                        func,
                        ctx.resolve(param_type),
                        ctx.resolve(&arg_type),
                        e.message
                    ),
                })?;

                typed_args.push(typed_arg);
            }

            // Resolve the return type after unification
            let return_type = ctx.resolve(&instantiated_return);

            Ok(TypedExpr::Call {
                func: func.clone(),
                args: typed_args,
                ty: return_type,
            })
        }

        Expr::UnaryOp { op, expr } => {
            let typed_expr = check_with_env(expr, env, ctx)?;
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
            let typed_left = check_with_env(left, env, ctx)?;
            let typed_right = check_with_env(right, env, ctx)?;
            let left_ty = typed_left.ty();
            let right_ty = typed_right.ty();

            // Unify left and right types
            ctx.unify(&left_ty, &right_ty)?;

            let resolved_ty = ctx.resolve(&left_ty);

            // Determine result type based on operator
            let result_ty = match op {
                // Arithmetic operators: result has same type as operands
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => resolved_ty,

                // Equality operators: work on any type, result is Bool
                BinOp::Eq | BinOp::Ne => Type::Bool,

                // Ordering operators: only work on numeric types, result is Bool
                BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    if !is_numeric_type(&resolved_ty) {
                        return Err(TypeError {
                            message: format!(
                                "ordering operators only work on numeric types, not {}",
                                resolved_ty
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
                let typed_binding = check_let_binding(binding, &block_env, ctx)?;
                // Add binding to environment for subsequent bindings
                block_env
                    .locals
                    .insert(binding.name.clone(), typed_binding.ty.clone());
                typed_bindings.push(typed_binding);
            }

            // Type-check the result expression with all bindings in scope
            let typed_result = check_with_env(result, &block_env, ctx)?;

            Ok(TypedExpr::Block {
                bindings: typed_bindings,
                result: Box::new(typed_result),
            })
        }

        Expr::Match { scrutinee, arms } => {
            let typed_scrutinee = check_with_env(scrutinee, env, ctx)?;
            let scrutinee_ty = typed_scrutinee.ty();

            if arms.is_empty() {
                return Err(TypeError {
                    message: "match expression must have at least one arm".to_string(),
                });
            }

            let mut typed_arms = Vec::new();
            let mut result_ty: Option<Type> = None;

            for arm in arms {
                let typed_arm = check_match_arm(arm, &scrutinee_ty, env, ctx)?;
                let arm_ty = typed_arm.result.ty();

                // Unify all arm result types
                match &result_ty {
                    None => result_ty = Some(arm_ty),
                    Some(ty) => {
                        ctx.unify(ty, &arm_ty).map_err(|e| TypeError {
                            message: format!(
                                "match arms have different types: {} vs {}: {}",
                                ctx.resolve(ty),
                                ctx.resolve(&arm_ty),
                                e.message
                            ),
                        })?;
                    }
                }

                typed_arms.push(typed_arm);
            }

            // Check exhaustiveness for List types
            let resolved_scrutinee_ty = ctx.resolve(&scrutinee_ty);
            if let Type::List(_) = resolved_scrutinee_ty {
                check_list_exhaustiveness(&typed_arms)?;
            }

            Ok(TypedExpr::Match {
                scrutinee: Box::new(typed_scrutinee),
                arms: typed_arms,
                ty: ctx.resolve(&result_ty.unwrap()),
            })
        }

        Expr::MethodCall {
            receiver,
            method,
            args,
        } => {
            let typed_receiver = check_with_env(receiver, env, ctx)?;
            let receiver_ty = ctx.resolve(&typed_receiver.ty());

            // Look up the method signature
            let (param_types, return_type) =
                builtin_method(&receiver_ty, method).ok_or_else(|| TypeError {
                    message: format!("no method '{}' on type {}", method, receiver_ty),
                })?;

            // Check argument count
            if args.len() != param_types.len() {
                return Err(TypeError {
                    message: format!(
                        "method '{}' expects {} argument(s), got {}",
                        method,
                        param_types.len(),
                        args.len()
                    ),
                });
            }

            // Type check arguments
            let mut typed_args = Vec::new();
            for (arg, param_ty) in args.iter().zip(param_types.iter()) {
                let typed_arg = check_with_env(arg, env, ctx)?;
                let arg_ty = typed_arg.ty();

                ctx.unify(&arg_ty, param_ty).map_err(|e| TypeError {
                    message: format!(
                        "argument type mismatch in method '{}': expected {}, got {}: {}",
                        method,
                        ctx.resolve(param_ty),
                        ctx.resolve(&arg_ty),
                        e.message
                    ),
                })?;

                typed_args.push(typed_arg);
            }

            Ok(TypedExpr::MethodCall {
                receiver: Box::new(typed_receiver),
                method: method.clone(),
                args: typed_args,
                ty: return_type,
            })
        }

        Expr::List(elements) => {
            if elements.is_empty() {
                // Empty list: create fresh type variable for element type
                let elem_ty = ctx.fresh_var();
                Ok(TypedExpr::List {
                    elements: vec![],
                    ty: Type::List(Box::new(elem_ty)),
                })
            } else {
                // Non-empty list: infer element type from first element
                let first_typed = check_with_env(&elements[0], env, ctx)?;
                let elem_ty = first_typed.ty();
                let mut typed_elements = vec![first_typed];

                // Check remaining elements unify with first element's type
                for elem in &elements[1..] {
                    let typed = check_with_env(elem, env, ctx)?;
                    ctx.unify(&typed.ty(), &elem_ty).map_err(|e| TypeError {
                        message: format!("list element type mismatch: {}", e.message),
                    })?;
                    typed_elements.push(typed);
                }

                Ok(TypedExpr::List {
                    elements: typed_elements,
                    ty: Type::List(Box::new(ctx.resolve(&elem_ty))),
                })
            }
        }

        Expr::Tuple(elements) => {
            let mut typed_elements = Vec::new();
            let mut element_types = Vec::new();

            for elem in elements {
                let typed = check_with_env(elem, env, ctx)?;
                element_types.push(typed.ty());
                typed_elements.push(typed);
            }

            Ok(TypedExpr::Tuple {
                elements: typed_elements,
                ty: Type::Tuple(element_types),
            })
        }
    }
}

/// Substitute type variables in a type using a mapping
fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or_else(|| ty.clone()),
        _ => ty.clone(),
    }
}

/// Check a pattern and return typed pattern with any bindings it introduces
fn check_pattern(
    pattern: &Pattern,
    scrutinee_ty: &Type,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    match pattern {
        Pattern::Literal(expr) => {
            let typed = check_with_env(expr, env, ctx)?;
            let lit_ty = typed.ty();

            // Unify literal type with scrutinee type
            ctx.unify(&lit_ty, scrutinee_ty).map_err(|e| TypeError {
                message: format!(
                    "pattern type {} does not match scrutinee type {}: {}",
                    ctx.resolve(&lit_ty),
                    ctx.resolve(scrutinee_ty),
                    e.message
                ),
            })?;

            Ok((TypedPattern::Literal(typed), HashMap::new()))
        }
        Pattern::Var(name) => {
            let mut bindings = HashMap::new();
            bindings.insert(name.clone(), ctx.resolve(scrutinee_ty));
            Ok((
                TypedPattern::Var {
                    name: name.clone(),
                    ty: ctx.resolve(scrutinee_ty),
                },
                bindings,
            ))
        }
        Pattern::Wildcard => Ok((TypedPattern::Wildcard, HashMap::new())),

        Pattern::List(list_pattern) => {
            // Unify scrutinee with List<T> for some fresh T
            let elem_ty = ctx.fresh_var();
            ctx.unify(scrutinee_ty, &Type::List(Box::new(elem_ty.clone())))
                .map_err(|e| TypeError {
                    message: format!(
                        "list pattern cannot match type {}: {}",
                        ctx.resolve(scrutinee_ty),
                        e.message
                    ),
                })?;
            let resolved_elem = ctx.resolve(&elem_ty);

            match list_pattern {
                ListPattern::Empty => Ok((TypedPattern::ListEmpty, HashMap::new())),

                ListPattern::Exact(patterns) => {
                    let mut typed_patterns = Vec::new();
                    let mut all_bindings = HashMap::new();

                    for pat in patterns {
                        let (typed_pat, bindings) =
                            check_pattern(pat, &resolved_elem, env, ctx)?;
                        typed_patterns.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::ListExact {
                            patterns: typed_patterns,
                            len: patterns.len(),
                        },
                        all_bindings,
                    ))
                }

                ListPattern::Prefix(patterns) => {
                    let mut typed_patterns = Vec::new();
                    let mut all_bindings = HashMap::new();

                    for pat in patterns {
                        let (typed_pat, bindings) =
                            check_pattern(pat, &resolved_elem, env, ctx)?;
                        typed_patterns.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::ListPrefix {
                            patterns: typed_patterns,
                            min_len: patterns.len(),
                        },
                        all_bindings,
                    ))
                }

                ListPattern::Suffix(patterns) => {
                    let mut typed_patterns = Vec::new();
                    let mut all_bindings = HashMap::new();

                    for pat in patterns {
                        let (typed_pat, bindings) =
                            check_pattern(pat, &resolved_elem, env, ctx)?;
                        typed_patterns.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::ListSuffix {
                            patterns: typed_patterns,
                            min_len: patterns.len(),
                        },
                        all_bindings,
                    ))
                }

                ListPattern::PrefixSuffix(prefix_pats, suffix_pats) => {
                    let mut prefix_typed = Vec::new();
                    let mut suffix_typed = Vec::new();
                    let mut all_bindings = HashMap::new();

                    for pat in prefix_pats {
                        let (typed_pat, bindings) =
                            check_pattern(pat, &resolved_elem, env, ctx)?;
                        prefix_typed.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    for pat in suffix_pats {
                        let (typed_pat, bindings) =
                            check_pattern(pat, &resolved_elem, env, ctx)?;
                        suffix_typed.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::ListPrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            min_len: prefix_pats.len() + suffix_pats.len(),
                        },
                        all_bindings,
                    ))
                }
            }
        }

        Pattern::Tuple(tuple_pattern) => {
            // Get the tuple element types from scrutinee
            let tuple_types = match ctx.resolve(scrutinee_ty) {
                Type::Tuple(types) => types,
                other => {
                    return Err(TypeError {
                        message: format!("tuple pattern cannot match type {}", other),
                    });
                }
            };

            match tuple_pattern {
                TuplePattern::Empty => {
                    if !tuple_types.is_empty() {
                        return Err(TypeError {
                            message: format!(
                                "empty tuple pattern cannot match tuple with {} elements",
                                tuple_types.len()
                            ),
                        });
                    }
                    Ok((TypedPattern::TupleEmpty, HashMap::new()))
                }

                TuplePattern::Exact(patterns) => {
                    if patterns.len() != tuple_types.len() {
                        return Err(TypeError {
                            message: format!(
                                "tuple pattern has {} elements but tuple has {} elements",
                                patterns.len(),
                                tuple_types.len()
                            ),
                        });
                    }

                    let mut typed_patterns = Vec::new();
                    let mut all_bindings = HashMap::new();

                    for (pat, ty) in patterns.iter().zip(tuple_types.iter()) {
                        let (typed_pat, bindings) = check_pattern(pat, ty, env, ctx)?;
                        typed_patterns.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::TupleExact {
                            patterns: typed_patterns,
                            len: patterns.len(),
                        },
                        all_bindings,
                    ))
                }

                TuplePattern::Prefix(patterns) => {
                    if patterns.len() > tuple_types.len() {
                        return Err(TypeError {
                            message: format!(
                                "tuple pattern has {} prefix elements but tuple has only {} elements",
                                patterns.len(),
                                tuple_types.len()
                            ),
                        });
                    }

                    let mut typed_patterns = Vec::new();
                    let mut all_bindings = HashMap::new();

                    for (pat, ty) in patterns.iter().zip(tuple_types.iter()) {
                        let (typed_pat, bindings) = check_pattern(pat, ty, env, ctx)?;
                        typed_patterns.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::TuplePrefix {
                            patterns: typed_patterns,
                            total_len: tuple_types.len(),
                        },
                        all_bindings,
                    ))
                }

                TuplePattern::Suffix(patterns) => {
                    if patterns.len() > tuple_types.len() {
                        return Err(TypeError {
                            message: format!(
                                "tuple pattern has {} suffix elements but tuple has only {} elements",
                                patterns.len(),
                                tuple_types.len()
                            ),
                        });
                    }

                    let mut typed_patterns = Vec::new();
                    let mut all_bindings = HashMap::new();

                    // Suffix patterns match from the end
                    let start_idx = tuple_types.len() - patterns.len();
                    for (pat, ty) in patterns.iter().zip(tuple_types[start_idx..].iter()) {
                        let (typed_pat, bindings) = check_pattern(pat, ty, env, ctx)?;
                        typed_patterns.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::TupleSuffix {
                            patterns: typed_patterns,
                            total_len: tuple_types.len(),
                        },
                        all_bindings,
                    ))
                }

                TuplePattern::PrefixSuffix(prefix_pats, suffix_pats) => {
                    let total_patterns = prefix_pats.len() + suffix_pats.len();
                    if total_patterns > tuple_types.len() {
                        return Err(TypeError {
                            message: format!(
                                "tuple pattern has {} elements but tuple has only {} elements",
                                total_patterns,
                                tuple_types.len()
                            ),
                        });
                    }

                    let mut prefix_typed = Vec::new();
                    let mut suffix_typed = Vec::new();
                    let mut all_bindings = HashMap::new();

                    // Prefix patterns match from the start
                    for (pat, ty) in prefix_pats.iter().zip(tuple_types.iter()) {
                        let (typed_pat, bindings) = check_pattern(pat, ty, env, ctx)?;
                        prefix_typed.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    // Suffix patterns match from the end
                    let suffix_start = tuple_types.len() - suffix_pats.len();
                    for (pat, ty) in suffix_pats.iter().zip(tuple_types[suffix_start..].iter()) {
                        let (typed_pat, bindings) = check_pattern(pat, ty, env, ctx)?;
                        suffix_typed.push(typed_pat);
                        all_bindings.extend(bindings);
                    }

                    Ok((
                        TypedPattern::TuplePrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            total_len: tuple_types.len(),
                        },
                        all_bindings,
                    ))
                }
            }
        }
    }
}

/// Check a match arm
fn check_match_arm(
    arm: &MatchArm,
    scrutinee_ty: &Type,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedMatchArm, TypeError> {
    let (typed_pattern, bindings) = check_pattern(&arm.pattern, scrutinee_ty, env, ctx)?;

    // Create arm environment with pattern bindings
    let mut arm_env = env.clone();
    arm_env.locals.extend(bindings);

    let typed_result = check_with_env(&arm.result, &arm_env, ctx)?;

    Ok(TypedMatchArm {
        pattern: typed_pattern,
        result: typed_result,
    })
}

/// Check exhaustiveness of list patterns
/// For lists, we need to cover both empty and non-empty cases
fn check_list_exhaustiveness(arms: &[TypedMatchArm]) -> Result<(), TypeError> {
    let mut has_catch_all = false;
    let mut empty_covered = false;
    let mut nonempty_covered = false;

    for arm in arms {
        match &arm.pattern {
            // Variable or wildcard covers everything
            TypedPattern::Var { .. } | TypedPattern::Wildcard => {
                has_catch_all = true;
            }
            // Empty list pattern covers empty case
            TypedPattern::ListEmpty => {
                empty_covered = true;
            }
            // Prefix pattern covers non-empty (matches any length >= min_len)
            TypedPattern::ListPrefix { .. } => {
                nonempty_covered = true;
            }
            // Suffix pattern covers non-empty (matches any length >= min_len)
            TypedPattern::ListSuffix { .. } => {
                nonempty_covered = true;
            }
            // PrefixSuffix pattern covers non-empty (matches any length >= min_len)
            TypedPattern::ListPrefixSuffix { .. } => {
                nonempty_covered = true;
            }
            // Exact pattern only covers specific length, but combined with others might help
            TypedPattern::ListExact { .. } => {
                // ListExact alone doesn't guarantee non-empty coverage
                // But if we have at least one exact pattern with len > 0, it covers some non-empty
                // We'll be conservative here - require explicit coverage
            }
            // Literal patterns don't cover list cases
            TypedPattern::Literal(_) => {}
            // Tuple patterns don't cover list cases
            TypedPattern::TupleEmpty
            | TypedPattern::TupleExact { .. }
            | TypedPattern::TuplePrefix { .. }
            | TypedPattern::TupleSuffix { .. }
            | TypedPattern::TuplePrefixSuffix { .. } => {}
        }

        // If we have a catch-all, we're done
        if has_catch_all {
            return Ok(());
        }
    }

    // Check if all cases are covered
    if !empty_covered && !nonempty_covered {
        return Err(TypeError {
            message: "non-exhaustive match on list: missing patterns for both empty and non-empty lists".to_string(),
        });
    }

    if !empty_covered {
        return Err(TypeError {
            message: "non-exhaustive match on list: missing pattern for empty list []".to_string(),
        });
    }

    if !nonempty_covered {
        return Err(TypeError {
            message: "non-exhaustive match on list: missing pattern for non-empty list (use [_, ..] or similar)".to_string(),
        });
    }

    Ok(())
}

/// Check a let binding and return a typed let binding
fn check_let_binding(
    binding: &LetBinding,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedLetBinding, TypeError> {
    let typed_value = check_with_env(&binding.value, env, ctx)?;
    let inferred_type = typed_value.ty();

    // If type annotation exists, unify with inferred type
    let binding_type = if let Some(ref annotation) = binding.type_annotation {
        let declared_type = resolve_type_annotation(annotation, &HashMap::new())?;
        ctx.unify(&inferred_type, &declared_type).map_err(|e| TypeError {
            message: format!(
                "let binding '{}' declares type {} but value has type {}: {}",
                binding.name,
                declared_type,
                ctx.resolve(&inferred_type),
                e.message
            ),
        })?;
        declared_type
    } else {
        ctx.resolve(&inferred_type)
    };

    Ok(TypedLetBinding {
        name: binding.name.clone(),
        value: typed_value,
        ty: binding_type,
    })
}

/// Check a file's items (functions), returning typed functions
pub fn check_file(items: &[Item]) -> Result<Vec<TypedFunction>, TypeError> {
    let mut ctx = UnifyCtx::new();

    // Build type environment with all function signatures first
    let mut env = TypeEnv::default();
    for item in items {
        let Item::Function(func) = item;
        let func_type = function_type_from_def(func, &mut ctx)?;
        env.functions.insert(func.name.clone(), func_type);
    }

    // Type-check all functions
    let mut typed_functions = Vec::new();
    for item in items {
        let Item::Function(func) = item;
        let typed = check_function(func, &env, &mut ctx)?;
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
    let mut ctx = UnifyCtx::new();
    let mut results = Vec::new();

    for statement in statements {
        match statement {
            Statement::Item(Item::Function(func)) => {
                // Add function type to environment first
                let func_type = function_type_from_def(func, &mut ctx)?;
                env.functions.insert(func.name.clone(), func_type);

                // Type-check the function
                let typed_func = check_function(func, env, &mut ctx)?;
                results.push(CheckedStatement::Function(typed_func));
            }
            Statement::Expr(expr) => {
                let typed_expr = check_with_env(expr, env, &mut ctx)?;
                results.push(CheckedStatement::Expr(typed_expr));
            }
            Statement::Let(binding) => {
                let typed_binding = check_let_binding(binding, env, &mut ctx)?;
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
        let mut ctx = UnifyCtx::new();
        check_with_env(expr, &TypeEnv::default(), &mut ctx)
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

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Var("x".to_string());
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_unknown_variable() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Var("x".to_string());
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown variable"));
    }

    #[test]
    fn test_check_variable_in_expression() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);
        env.locals.insert("y".to_string(), Type::Int32);

        let mut ctx = UnifyCtx::new();
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Var("y".to_string())),
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_function_call() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![Type::Int32],
                return_type: Type::Int32,
            },
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            func: "square".to_string(),
            args: vec![Expr::Int(5)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_function_call_wrong_arg_type() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![Type::Int32],
                return_type: Type::Int32,
            },
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            func: "square".to_string(),
            args: vec![Expr::Float(5.0)],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_check_function_call_wrong_arity() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "add".to_string(),
            FunctionType {
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![Type::Int32, Type::Int32],
                return_type: Type::Int32,
            },
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            func: "add".to_string(),
            args: vec![Expr::Int(1)],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("expects 2 arguments"));
    }

    #[test]
    fn test_check_generic_function_call() {
        let mut ctx = UnifyCtx::new();
        let t_var = ctx.fresh_var();
        let t_id = if let Type::Var(id) = t_var { id } else { panic!() };

        let mut env = TypeEnv::default();
        env.functions.insert(
            "identity".to_string(),
            FunctionType {
                type_params: vec!["T".to_string()],
                type_var_ids: vec![t_id],
                params: vec![Type::Var(t_id)],
                return_type: Type::Var(t_id),
            },
        );

        // identity(42) should return Int32
        let expr = Expr::Call {
            func: "identity".to_string(),
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_generic_function_call_float() {
        let mut ctx = UnifyCtx::new();
        let t_var = ctx.fresh_var();
        let t_id = if let Type::Var(id) = t_var { id } else { panic!() };

        let mut env = TypeEnv::default();
        env.functions.insert(
            "identity".to_string(),
            FunctionType {
                type_params: vec!["T".to_string()],
                type_var_ids: vec![t_id],
                params: vec![Type::Var(t_id)],
                return_type: Type::Var(t_id),
            },
        );

        // identity(3.14) should return Float
        let expr = Expr::Call {
            func: "identity".to_string(),
            args: vec![Expr::Float(3.14)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Float);
    }

    #[test]
    fn test_check_function_def() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
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

        let result = check_function(&func, &env, &mut ctx).unwrap();
        assert_eq!(result.name, "double");
        assert_eq!(result.return_type, Type::Int32);
    }

    #[test]
    fn test_check_function_def_return_type_mismatch() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
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

        let result = check_function(&func, &env, &mut ctx);
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
                type_var_ids: vec![],
                params: vec![Type::Int32, Type::Int32],
                return_type: Type::Int32,
            },
        );

        let mut ctx = UnifyCtx::new();
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

        let result = check_function(&func, &env, &mut ctx).unwrap();
        assert_eq!(result.return_type, Type::Int32);
    }

    #[test]
    fn test_function_type_from_def() {
        let mut ctx = UnifyCtx::new();
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

        let ft = function_type_from_def(&func, &mut ctx).unwrap();
        assert_eq!(ft.params, vec![Type::Int32, Type::Int32]);
        assert_eq!(ft.return_type, Type::Int32);
    }

    #[test]
    fn test_function_type_from_def_generic() {
        let mut ctx = UnifyCtx::new();
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

        let ft = function_type_from_def(&func, &mut ctx).unwrap();
        assert_eq!(ft.type_params, vec!["T".to_string()]);
        assert_eq!(ft.type_var_ids.len(), 1);
        // Params and return type should use the same type variable
        assert!(matches!(ft.params[0], Type::Var(_)));
        assert!(matches!(ft.return_type, Type::Var(_)));
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

        let mut ctx = UnifyCtx::new();
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
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::String);
    }

    #[test]
    fn test_check_match_with_wildcard() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let mut ctx = UnifyCtx::new();
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
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_match_with_variable_binding() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let mut ctx = UnifyCtx::new();
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
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_match_pattern_type_mismatch() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".to_string())),
            arms: vec![MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::String("hello".to_string()))),
                result: Expr::Int(1),
            }],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("does not match scrutinee"));
    }

    #[test]
    fn test_check_match_arm_type_mismatch() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), Type::Int32);

        let mut ctx = UnifyCtx::new();
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
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("different types"));
    }

    #[test]
    fn test_check_method_call_len() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "len".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_method_call_is_empty() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("".to_string())),
            method: "is_empty".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_method_call_contains() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "contains".to_string(),
            args: vec![Expr::String("ell".to_string())],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_method_call_to_uppercase() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "to_uppercase".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::String);
    }

    #[test]
    fn test_check_method_call_trim() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("  hello  ".to_string())),
            method: "trim".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::String);
    }

    #[test]
    fn test_check_method_call_unknown_method() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "foo".to_string(),
            args: vec![],
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("no method 'foo'"));
    }

    #[test]
    fn test_check_method_call_on_int_error() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::Int(42)),
            method: "len".to_string(),
            args: vec![],
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("no method 'len' on type Int32"));
    }

    #[test]
    fn test_check_method_call_wrong_arg_count() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "contains".to_string(),
            args: vec![], // contains expects 1 argument
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("expects 1 argument"));
    }

    #[test]
    fn test_check_method_call_wrong_arg_type() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::String("hello".to_string())),
            method: "contains".to_string(),
            args: vec![Expr::Int(42)], // contains expects String, not Int32
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_check_chained_method_calls() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::MethodCall {
                receiver: Box::new(Expr::String("hello".to_string())),
                method: "to_uppercase".to_string(),
                args: vec![],
            }),
            method: "len".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }
}
