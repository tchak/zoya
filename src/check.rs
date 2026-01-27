use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, ListPattern, MatchArm, Pattern, Statement,
    StructDef, StructPattern, TuplePattern, TypeAnnotation, UnaryOp,
};
use crate::ir::{CheckedItem, TypedExpr, TypedFunction, TypedLetBinding, TypedMatchArm, TypedPattern};
use crate::types::{FunctionType, StructType, Type, TypeError, TypeScheme, TypeVarId};
use crate::unify::UnifyCtx;
use crate::usefulness;

/// Type environment for checking expressions
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// Function signatures
    pub functions: HashMap<String, FunctionType>,
    /// Struct type definitions
    pub structs: HashMap<String, StructType>,
    /// Local variable types (parameters in function bodies) - monomorphic
    pub locals: HashMap<String, Type>,
    /// Polymorphic let-bound variables (type schemes for let polymorphism)
    pub poly_locals: HashMap<String, TypeScheme>,
}

impl TypeEnv {
    pub fn with_locals(&self, locals: HashMap<String, Type>) -> Self {
        TypeEnv {
            functions: self.functions.clone(),
            structs: self.structs.clone(),
            locals,
            poly_locals: self.poly_locals.clone(),
        }
    }

    /// Collect all free type variables in the environment
    pub fn free_vars(&self, ctx: &UnifyCtx) -> HashSet<TypeVarId> {
        let mut set = HashSet::new();
        for ty in self.locals.values() {
            set.extend(ctx.free_vars(ty));
        }
        for scheme in self.poly_locals.values() {
            // Free vars in scheme = free vars in type - quantified vars
            let ty_vars = ctx.free_vars(&scheme.ty);
            let quantified: HashSet<_> = scheme.quantified.iter().cloned().collect();
            set.extend(ty_vars.difference(&quantified).cloned());
        }
        set
    }
}

/// Resolve a type annotation to a concrete Type.
/// `type_param_map` maps source-level type parameter names (like "T") to TypeVarIds.
/// `env` provides access to struct definitions for struct type resolution.
fn resolve_type_annotation(
    annotation: &TypeAnnotation,
    type_param_map: &HashMap<String, TypeVarId>,
    env: &TypeEnv,
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
            } else if let Some(struct_def) = env.structs.get(name) {
                // Non-generic struct reference
                if !struct_def.type_params.is_empty() {
                    return Err(TypeError {
                        message: format!(
                            "struct {} requires {} type argument(s)",
                            name,
                            struct_def.type_params.len()
                        ),
                    });
                }
                // Non-generic struct: use fields as-is
                Ok(Type::Struct {
                    name: name.clone(),
                    type_args: vec![],
                    fields: struct_def.fields.clone(),
                })
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
                let elem_type = resolve_type_annotation(&params[0], type_param_map, env)?;
                Ok(Type::List(Box::new(elem_type)))
            } else if let Some(struct_def) = env.structs.get(name) {
                // Generic struct reference
                if params.len() != struct_def.type_params.len() {
                    return Err(TypeError {
                        message: format!(
                            "struct {} expects {} type argument(s), got {}",
                            name,
                            struct_def.type_params.len(),
                            params.len()
                        ),
                    });
                }
                let type_args = params
                    .iter()
                    .map(|p| resolve_type_annotation(p, type_param_map, env))
                    .collect::<Result<Vec<_>, _>>()?;
                // Substitute type args into field types
                let mut subst = HashMap::new();
                for (id, arg) in struct_def.type_var_ids.iter().zip(type_args.iter()) {
                    subst.insert(*id, arg.clone());
                }
                let fields = struct_def
                    .fields
                    .iter()
                    .map(|(n, t)| (n.clone(), substitute_type_vars(t, &subst)))
                    .collect();
                Ok(Type::Struct {
                    name: name.clone(),
                    type_args,
                    fields,
                })
            } else {
                Err(TypeError {
                    message: format!("unknown parameterized type: {}", name),
                })
            }
        }
        TypeAnnotation::Tuple(params) => {
            let mut types = Vec::new();
            for param in params {
                types.push(resolve_type_annotation(param, type_param_map, env)?);
            }
            Ok(Type::Tuple(types))
        }
        TypeAnnotation::Function(params, ret) => {
            let mut param_types = Vec::new();
            for param in params {
                param_types.push(resolve_type_annotation(param, type_param_map, env)?);
            }
            let ret_type = resolve_type_annotation(ret, type_param_map, env)?;
            Ok(Type::Function {
                params: param_types,
                ret: Box::new(ret_type),
            })
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
        let ty = resolve_type_annotation(&param.typ, &type_param_map, env)?;
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
        let declared_return = resolve_type_annotation(annotation, &type_param_map, env)?;
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
    env: &TypeEnv,
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
        let ty = resolve_type_annotation(&param.typ, &type_param_map, env)?;
        param_types.push(ty);
    }

    let return_type = if let Some(ref annotation) = func.return_type {
        resolve_type_annotation(annotation, &type_param_map, env)?
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

        // List methods
        (Type::List(_), "len") => Some((vec![], Type::Int32)),
        (Type::List(_), "is_empty") => Some((vec![], Type::Bool)),
        (Type::List(elem_ty), "reverse") => Some((vec![], Type::List(elem_ty.clone()))),
        (Type::List(elem_ty), "push") => {
            Some((vec![*elem_ty.clone()], Type::List(elem_ty.clone())))
        }
        (Type::List(elem_ty), "concat") => Some((
            vec![Type::List(elem_ty.clone())],
            Type::List(elem_ty.clone()),
        )),

        _ => None,
    }
}

// ============================================================================
// Expression type checking helper functions
// ============================================================================

/// Check a variable reference
fn check_var(name: &str, env: &TypeEnv, ctx: &mut UnifyCtx) -> Result<TypedExpr, TypeError> {
    // First check polymorphic locals (let-bound values with type schemes)
    if let Some(scheme) = env.poly_locals.get(name) {
        let ty = ctx.instantiate(scheme);
        Ok(TypedExpr::Var {
            name: name.to_string(),
            ty: ctx.resolve(&ty),
        })
    }
    // Then check monomorphic locals (function parameters)
    else if let Some(ty) = env.locals.get(name) {
        Ok(TypedExpr::Var {
            name: name.to_string(),
            ty: ctx.resolve(ty),
        })
    } else {
        Err(TypeError {
            message: format!("unknown variable: {}", name),
        })
    }
}

/// Check a function call
fn check_call(
    func: &str,
    args: &[Expr],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // First, try to look up as a named function
    if let Some(func_type) = env.functions.get(func) {
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
        let instantiated_return = substitute_type_vars(&func_type.return_type, &instantiation);

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
            func: func.to_string(),
            args: typed_args,
            ty: return_type,
        })
    }
    // Try to look up as a lambda-bound variable
    else if let Some(func_ty) = get_callable_type(func, env, ctx) {
        // Must be a function type
        let resolved = ctx.resolve(&func_ty);
        if let Type::Function { params, ret } = resolved {
            // Check argument count
            if args.len() != params.len() {
                return Err(TypeError {
                    message: format!(
                        "'{}' expects {} arguments, got {}",
                        func,
                        params.len(),
                        args.len()
                    ),
                });
            }

            // Type check arguments and unify with parameter types
            let mut typed_args = Vec::new();
            for (arg, param_type) in args.iter().zip(params.iter()) {
                let typed_arg = check_with_env(arg, env, ctx)?;
                let arg_type = typed_arg.ty();

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

            let return_type = ctx.resolve(&ret);

            Ok(TypedExpr::Call {
                func: func.to_string(),
                args: typed_args,
                ty: return_type,
            })
        } else {
            Err(TypeError {
                message: format!("'{}' is not a function, has type {}", func, resolved),
            })
        }
    } else {
        Err(TypeError {
            message: format!("unknown function: {}", func),
        })
    }
}

/// Check a unary operation
fn check_unary_op(
    op: UnaryOp,
    expr: &Expr,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_with_env(expr, env, ctx)?;
    let ty = typed_expr.ty();
    match op {
        UnaryOp::Neg => {
            // Negation only works on numeric types
            let resolved = ctx.resolve(&ty);
            match resolved {
                Type::Int32 | Type::Int64 | Type::Float => Ok(TypedExpr::UnaryOp {
                    op,
                    expr: Box::new(typed_expr),
                    ty,
                }),
                _ => Err(TypeError {
                    message: format!(
                        "negation operator only works on numeric types, not {}",
                        resolved
                    ),
                }),
            }
        }
    }
}

/// Check a binary operation
fn check_bin_op(
    op: BinOp,
    left: &Expr,
    right: &Expr,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
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
        op,
        left: Box::new(typed_left),
        right: Box::new(typed_right),
        ty: result_ty,
    })
}

/// Check a block expression with let bindings
fn check_block(
    bindings: &[LetBinding],
    result: &Expr,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // Create new environment for block scope
    let mut block_env = env.clone();
    let mut typed_bindings = Vec::new();

    for binding in bindings {
        let typed_binding = check_let_binding(binding, &block_env, ctx)?;

        // Check if we should generalize (let polymorphism)
        // Only generalize syntactic values (lambdas, literals, variables)
        if is_syntactic_value(&binding.value) {
            // Collect fixed vars from the environment
            let fixed_vars = block_env.free_vars(ctx);
            let scheme = ctx.generalize(&typed_binding.ty, &fixed_vars);
            block_env.poly_locals.insert(binding.name.clone(), scheme);
        } else {
            // Monomorphic binding
            block_env
                .locals
                .insert(binding.name.clone(), typed_binding.ty.clone());
        }

        typed_bindings.push(typed_binding);
    }

    // Type-check the result expression with all bindings in scope
    let typed_result = check_with_env(result, &block_env, ctx)?;

    Ok(TypedExpr::Block {
        bindings: typed_bindings,
        result: Box::new(typed_result),
    })
}

/// Check a match expression
fn check_match_expr(
    scrutinee: &Expr,
    arms: &[MatchArm],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
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

    // Check exhaustiveness and usefulness for all types
    let resolved_scrutinee_ty = ctx.resolve(&scrutinee_ty);
    usefulness::check_patterns(&typed_arms, &resolved_scrutinee_ty)?;

    Ok(TypedExpr::Match {
        scrutinee: Box::new(typed_scrutinee),
        arms: typed_arms,
        ty: ctx.resolve(&result_ty.unwrap()),
    })
}

/// Check a method call expression
fn check_method_call(
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
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
        method: method.to_string(),
        args: typed_args,
        ty: return_type,
    })
}

/// Check a list literal expression
fn check_list_expr(
    elements: &[Expr],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
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

/// Check a tuple literal expression
fn check_tuple_expr(
    elements: &[Expr],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
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

/// Check a lambda expression
fn check_lambda(
    params: &[crate::ast::LambdaParam],
    return_type: &Option<TypeAnnotation>,
    body: &Expr,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // Create fresh type variables for unannotated parameters
    let mut param_types = Vec::new();
    let mut lambda_env = env.clone();

    for param in params {
        let param_ty = match &param.typ {
            Some(annotation) => resolve_type_annotation(annotation, &HashMap::new(), env)?,
            None => ctx.fresh_var(),
        };
        lambda_env.locals.insert(param.name.clone(), param_ty.clone());
        param_types.push(param_ty);
    }

    // Check the body in the extended environment
    let typed_body = check_with_env(body, &lambda_env, ctx)?;
    let body_ty = typed_body.ty();

    // If return type is annotated, unify with body type
    let resolved_return = if let Some(annotation) = return_type {
        let declared_return = resolve_type_annotation(annotation, &HashMap::new(), env)?;
        ctx.unify(&body_ty, &declared_return).map_err(|e| TypeError {
            message: format!(
                "lambda body type {} doesn't match declared return type {}: {}",
                ctx.resolve(&body_ty),
                ctx.resolve(&declared_return),
                e.message
            ),
        })?;
        ctx.resolve(&declared_return)
    } else {
        ctx.resolve(&body_ty)
    };

    // Construct the function type
    let lambda_type = Type::Function {
        params: param_types.iter().map(|t| ctx.resolve(t)).collect(),
        ret: Box::new(resolved_return.clone()),
    };

    Ok(TypedExpr::Lambda {
        params: params
            .iter()
            .zip(param_types.iter())
            .map(|(p, ty)| (p.name.clone(), ctx.resolve(ty)))
            .collect(),
        body: Box::new(typed_body),
        ty: lambda_type,
    })
}

/// Check a struct constructor expression
fn check_struct_construct(
    name: &str,
    fields: &[(String, Expr)],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // Look up the struct definition
    let struct_type = env.structs.get(name).ok_or_else(|| TypeError {
        message: format!("unknown struct: {}", name),
    })?;

    // Create fresh type variables for generic parameters
    let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
    for &old_id in &struct_type.type_var_ids {
        instantiation.insert(old_id, ctx.fresh_var());
    }

    // Check that all required fields are present and no extra fields
    let expected_field_names: HashSet<&str> = struct_type.fields.iter().map(|(n, _)| n.as_str()).collect();
    let provided_field_names: HashSet<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();

    // Check for missing fields
    for expected in &expected_field_names {
        if !provided_field_names.contains(expected) {
            return Err(TypeError {
                message: format!("missing field '{}' in struct {}", expected, name),
            });
        }
    }

    // Check for extra fields
    for provided in &provided_field_names {
        if !expected_field_names.contains(provided) {
            return Err(TypeError {
                message: format!("unknown field '{}' in struct {}", provided, name),
            });
        }
    }

    // Type-check each field value
    let mut typed_fields = Vec::new();
    for (field_name, field_expr) in fields {
        // Find the expected type for this field
        let (_, field_type) = struct_type
            .fields
            .iter()
            .find(|(n, _)| n == field_name)
            .ok_or_else(|| TypeError {
                message: format!("unknown field '{}' in struct {}", field_name, name),
            })?;

        // Substitute type variables to get the expected type for this instantiation
        let expected_type = substitute_type_vars(field_type, &instantiation);

        // Type-check the field expression
        let typed_expr = check_with_env(field_expr, env, ctx)?;
        let actual_type = typed_expr.ty();

        // Unify with expected type
        ctx.unify(&actual_type, &expected_type).map_err(|e| TypeError {
            message: format!(
                "field '{}' in struct {} expects type {} but got {}: {}",
                field_name,
                name,
                ctx.resolve(&expected_type),
                ctx.resolve(&actual_type),
                e.message
            ),
        })?;

        typed_fields.push((field_name.clone(), typed_expr));
    }

    // Build the struct type with resolved type arguments
    let type_args: Vec<Type> = struct_type
        .type_var_ids
        .iter()
        .map(|id| ctx.resolve(&instantiation[id]))
        .collect();

    // Build resolved field types for the Type::Struct
    let resolved_fields: Vec<(String, Type)> = struct_type
        .fields
        .iter()
        .map(|(name, ty)| (name.clone(), ctx.resolve(&substitute_type_vars(ty, &instantiation))))
        .collect();

    Ok(TypedExpr::StructConstruct {
        name: name.to_string(),
        fields: typed_fields,
        ty: Type::Struct {
            name: name.to_string(),
            type_args,
            fields: resolved_fields,
        },
    })
}

/// Check a field access expression
fn check_field_access(
    expr: &Expr,
    field: &str,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_with_env(expr, env, ctx)?;
    let expr_ty = ctx.resolve(&typed_expr.ty());

    match &expr_ty {
        Type::Struct { name, fields: struct_fields, .. } => {
            // Find the field directly in the resolved type
            let (_, field_type) = struct_fields
                .iter()
                .find(|(n, _)| n == field)
                .ok_or_else(|| TypeError {
                    message: format!("struct {} has no field '{}'", name, field),
                })?;

            Ok(TypedExpr::FieldAccess {
                expr: Box::new(typed_expr),
                field: field.to_string(),
                ty: field_type.clone(),
            })
        }
        _ => Err(TypeError {
            message: format!(
                "cannot access field '{}' on non-struct type {}",
                field, expr_ty
            ),
        }),
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
        Expr::Int64(n) => Ok(TypedExpr::Int64(*n)),
        Expr::Float(n) => Ok(TypedExpr::Float(*n)),
        Expr::Bool(b) => Ok(TypedExpr::Bool(*b)),
        Expr::String(s) => Ok(TypedExpr::String(s.clone())),
        Expr::Var(name) => check_var(name, env, ctx),
        Expr::Call { func, args } => check_call(func, args, env, ctx),
        Expr::UnaryOp { op, expr } => check_unary_op(*op, expr, env, ctx),
        Expr::BinOp { op, left, right } => check_bin_op(*op, left, right, env, ctx),
        Expr::Block { bindings, result } => check_block(bindings, result, env, ctx),
        Expr::Match { scrutinee, arms } => check_match_expr(scrutinee, arms, env, ctx),
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => check_method_call(receiver, method, args, env, ctx),
        Expr::List(elements) => check_list_expr(elements, env, ctx),
        Expr::Tuple(elements) => check_tuple_expr(elements, env, ctx),
        Expr::Lambda {
            params,
            return_type,
            body,
        } => check_lambda(params, return_type, body, env, ctx),
        Expr::StructConstruct { name, fields } => check_struct_construct(name, fields, env, ctx),
        Expr::FieldAccess { expr, field } => check_field_access(expr, field, env, ctx),
    }
}

/// Substitute type variables in a type using a mapping (recursive)
fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(elem) => Type::List(Box::new(substitute_type_vars(elem, mapping))),
        Type::Tuple(elems) => {
            Type::Tuple(elems.iter().map(|t| substitute_type_vars(t, mapping)).collect())
        }
        Type::Function { params, ret } => Type::Function {
            params: params.iter().map(|t| substitute_type_vars(t, mapping)).collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::Struct { name, type_args, fields } => Type::Struct {
            name: name.clone(),
            type_args: type_args.iter().map(|t| substitute_type_vars(t, mapping)).collect(),
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping))).collect(),
        },
        // Concrete types don't contain type vars
        Type::Int32 | Type::Int64 | Type::Float | Type::Bool | Type::String => ty.clone(),
    }
}

/// Get the type of a callable (lambda-bound variable), instantiating if polymorphic
fn get_callable_type(name: &str, env: &TypeEnv, ctx: &mut UnifyCtx) -> Option<Type> {
    // Check polymorphic locals first
    if let Some(scheme) = env.poly_locals.get(name) {
        return Some(ctx.instantiate(scheme));
    }
    // Then check monomorphic locals
    if let Some(ty) = env.locals.get(name) {
        return Some(ty.clone());
    }
    None
}

/// Check if an expression is a syntactic value (safe to generalize under value restriction)
fn is_syntactic_value(expr: &Expr) -> bool {
    match expr {
        Expr::Lambda { .. } => true,
        Expr::Int(_) | Expr::Int64(_) | Expr::Float(_) | Expr::Bool(_) | Expr::String(_) => true,
        Expr::List(elems) => elems.iter().all(is_syntactic_value),
        Expr::Tuple(elems) => elems.iter().all(is_syntactic_value),
        Expr::Var(_) => true,
        _ => false,
    }
}

/// Check a list of patterns against a single element type (for list patterns)
fn check_patterns_against_elem(
    patterns: &[Pattern],
    elem_ty: &Type,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(Vec<TypedPattern>, HashMap<String, Type>), TypeError> {
    let mut typed_patterns = Vec::new();
    let mut all_bindings = HashMap::new();
    for pat in patterns {
        let (typed_pat, bindings) = check_pattern(pat, elem_ty, env, ctx)?;
        typed_patterns.push(typed_pat);
        all_bindings.extend(bindings);
    }
    Ok((typed_patterns, all_bindings))
}

/// Check a list of patterns against corresponding types (for tuple patterns)
fn check_patterns_against_types(
    patterns: &[Pattern],
    types: &[Type],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(Vec<TypedPattern>, HashMap<String, Type>), TypeError> {
    let mut typed_patterns = Vec::new();
    let mut all_bindings = HashMap::new();
    for (pat, ty) in patterns.iter().zip(types.iter()) {
        let (typed_pat, bindings) = check_pattern(pat, ty, env, ctx)?;
        typed_patterns.push(typed_pat);
        all_bindings.extend(bindings);
    }
    Ok((typed_patterns, all_bindings))
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
                    let (typed_patterns, bindings) =
                        check_patterns_against_elem(patterns, &resolved_elem, env, ctx)?;
                    Ok((
                        TypedPattern::ListExact {
                            patterns: typed_patterns,
                            len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::Prefix(patterns) => {
                    let (typed_patterns, bindings) =
                        check_patterns_against_elem(patterns, &resolved_elem, env, ctx)?;
                    Ok((
                        TypedPattern::ListPrefix {
                            patterns: typed_patterns,
                            min_len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::Suffix(patterns) => {
                    let (typed_patterns, bindings) =
                        check_patterns_against_elem(patterns, &resolved_elem, env, ctx)?;
                    Ok((
                        TypedPattern::ListSuffix {
                            patterns: typed_patterns,
                            min_len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::PrefixSuffix(prefix_pats, suffix_pats) => {
                    let (prefix_typed, mut bindings) =
                        check_patterns_against_elem(prefix_pats, &resolved_elem, env, ctx)?;
                    let (suffix_typed, suffix_bindings) =
                        check_patterns_against_elem(suffix_pats, &resolved_elem, env, ctx)?;
                    bindings.extend(suffix_bindings);
                    Ok((
                        TypedPattern::ListPrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            min_len: prefix_pats.len() + suffix_pats.len(),
                        },
                        bindings,
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

                    let (typed_patterns, bindings) =
                        check_patterns_against_types(patterns, &tuple_types, env, ctx)?;
                    Ok((
                        TypedPattern::TupleExact {
                            patterns: typed_patterns,
                            len: patterns.len(),
                        },
                        bindings,
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

                    let (typed_patterns, bindings) =
                        check_patterns_against_types(patterns, &tuple_types, env, ctx)?;
                    Ok((
                        TypedPattern::TuplePrefix {
                            patterns: typed_patterns,
                            total_len: tuple_types.len(),
                        },
                        bindings,
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

                    // Suffix patterns match from the end
                    let start_idx = tuple_types.len() - patterns.len();
                    let (typed_patterns, bindings) =
                        check_patterns_against_types(patterns, &tuple_types[start_idx..], env, ctx)?;
                    Ok((
                        TypedPattern::TupleSuffix {
                            patterns: typed_patterns,
                            total_len: tuple_types.len(),
                        },
                        bindings,
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

                    // Prefix patterns match from the start
                    let (prefix_typed, mut bindings) =
                        check_patterns_against_types(prefix_pats, &tuple_types, env, ctx)?;

                    // Suffix patterns match from the end
                    let suffix_start = tuple_types.len() - suffix_pats.len();
                    let (suffix_typed, suffix_bindings) =
                        check_patterns_against_types(suffix_pats, &tuple_types[suffix_start..], env, ctx)?;
                    bindings.extend(suffix_bindings);

                    Ok((
                        TypedPattern::TuplePrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            total_len: tuple_types.len(),
                        },
                        bindings,
                    ))
                }
            }
        }

        Pattern::Struct(struct_pattern) => {
            // Get struct name and fields based on pattern variant
            let (struct_name, field_patterns, is_partial) = match struct_pattern {
                StructPattern::Exact { name, fields } => (name, fields, false),
                StructPattern::Partial { name, fields } => (name, fields, true),
            };

            // Unify scrutinee with the struct type
            let struct_type = env.structs.get(struct_name).ok_or_else(|| TypeError {
                message: format!("unknown struct in pattern: {}", struct_name),
            })?;

            // Create fresh type variables for generic parameters
            let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
            for &old_id in &struct_type.type_var_ids {
                instantiation.insert(old_id, ctx.fresh_var());
            }

            // Build the expected struct type and unify with scrutinee
            let type_args: Vec<Type> = struct_type
                .type_var_ids
                .iter()
                .map(|id| instantiation[id].clone())
                .collect();
            let resolved_fields: Vec<(String, Type)> = struct_type
                .fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, &instantiation)))
                .collect();
            let expected_struct_ty = Type::Struct {
                name: struct_name.clone(),
                type_args,
                fields: resolved_fields,
            };

            ctx.unify(scrutinee_ty, &expected_struct_ty)
                .map_err(|e| TypeError {
                    message: format!(
                        "struct pattern {} cannot match type {}: {}",
                        struct_name,
                        ctx.resolve(scrutinee_ty),
                        e.message
                    ),
                })?;

            // For exact patterns, verify all fields are covered
            if !is_partial {
                let expected_field_names: HashSet<&str> = struct_type
                    .fields
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect();
                let provided_field_names: HashSet<&str> = field_patterns
                    .iter()
                    .map(|f| f.field_name.as_str())
                    .collect();

                for expected in &expected_field_names {
                    if !provided_field_names.contains(expected) {
                        return Err(TypeError {
                            message: format!(
                                "missing field '{}' in struct pattern {} (use '..' for partial match)",
                                expected, struct_name
                            ),
                        });
                    }
                }
            }

            // Check each field pattern
            let mut all_bindings = HashMap::new();
            let mut typed_fields = Vec::new();

            for field_pattern in field_patterns {
                // Find the field type
                let (_, field_type) = struct_type
                    .fields
                    .iter()
                    .find(|(n, _)| n == &field_pattern.field_name)
                    .ok_or_else(|| TypeError {
                        message: format!(
                            "struct {} has no field '{}'",
                            struct_name, field_pattern.field_name
                        ),
                    })?;

                // Substitute type variables
                let resolved_field_type = substitute_type_vars(field_type, &instantiation);
                let resolved_field_type = ctx.resolve(&resolved_field_type);

                // Recursively check the field pattern
                let (typed_sub_pattern, sub_bindings) =
                    check_pattern(&field_pattern.pattern, &resolved_field_type, env, ctx)?;
                all_bindings.extend(sub_bindings);
                typed_fields.push((field_pattern.field_name.clone(), typed_sub_pattern));
            }

            let typed_pattern = if is_partial {
                TypedPattern::StructPartial {
                    name: struct_name.clone(),
                    fields: typed_fields,
                }
            } else {
                TypedPattern::StructExact {
                    name: struct_name.clone(),
                    fields: typed_fields,
                }
            };

            Ok((typed_pattern, all_bindings))
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
        let declared_type = resolve_type_annotation(annotation, &HashMap::new(), env)?;
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

/// Check a file's items (functions and structs), returning checked items
pub fn check_file(items: &[Item]) -> Result<Vec<CheckedItem>, TypeError> {
    let mut ctx = UnifyCtx::new();
    let mut env = TypeEnv::default();

    // Phase 1a: Register all struct names with placeholder types
    // This allows structs to reference each other
    for item in items {
        if let Item::Struct(def) = item {
            // Register with empty fields first - will fill in later
            let mut type_var_ids = Vec::new();
            for _ in &def.type_params {
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    type_var_ids.push(id);
                }
            }
            env.structs.insert(
                def.name.clone(),
                StructType {
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    fields: vec![], // placeholder
                },
            );
        }
    }

    // Phase 1b: Now resolve all struct field types
    for item in items {
        if let Item::Struct(def) = item {
            let struct_type = struct_type_from_def(def, &env, &mut ctx)?;
            env.structs.insert(def.name.clone(), struct_type);
        }
    }

    // Phase 2: Register all function signatures (now struct types are available)
    for item in items {
        if let Item::Function(func) = item {
            let func_type = function_type_from_def(func, &env, &mut ctx)?;
            env.functions.insert(func.name.clone(), func_type);
        }
    }

    // Phase 3: Type-check all items
    let mut checked_items = Vec::new();
    for item in items {
        match item {
            Item::Function(func) => {
                let typed = check_function(func, &env, &mut ctx)?;
                checked_items.push(CheckedItem::Function(typed));
            }
            Item::Struct(def) => {
                // Structs are just passed through (already registered in env)
                checked_items.push(CheckedItem::Struct(def.clone()));
            }
        }
    }

    Ok(checked_items)
}

/// Extract struct type from a struct definition (for adding to env).
fn struct_type_from_def(
    def: &StructDef,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<StructType, TypeError> {
    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    let mut type_var_ids = Vec::new();
    for name in &def.type_params {
        let var = ctx.fresh_var();
        if let Type::Var(id) = var {
            type_param_map.insert(name.clone(), id);
            type_var_ids.push(id);
        }
    }

    // Resolve field types
    let mut fields = Vec::new();
    for field in &def.fields {
        let ty = resolve_type_annotation(&field.typ, &type_param_map, env)?;
        fields.push((field.name.clone(), ty));
    }

    Ok(StructType {
        name: def.name.clone(),
        type_params: def.type_params.clone(),
        type_var_ids,
        fields,
    })
}

/// Type-checked statement result for REPL
#[derive(Debug, Clone, PartialEq)]
pub enum CheckedStatement {
    Function(TypedFunction),
    Struct(StructDef),
    Expr(TypedExpr),
    Let(TypedLetBinding),
}

/// Check REPL statements with multi-pass type checking for forward references.
///
/// This uses a multi-pass algorithm similar to `check_file`:
/// 1. Partition statements into structs, functions, and others
/// 2. Register all struct definitions first
/// 3. Register all function signatures (enables forward references)
/// 4. Type-check all function bodies (all signatures now available)
/// 5. Type-check non-item statements in order
/// 6. Sort results by original index to preserve input order
pub fn check_repl(
    statements: &[Statement],
    env: &mut TypeEnv,
) -> Result<Vec<CheckedStatement>, TypeError> {
    let mut ctx = UnifyCtx::new();

    // Phase 1: Partition statements
    let mut struct_items: Vec<(usize, &StructDef)> = Vec::new();
    let mut function_items: Vec<(usize, &FunctionDef)> = Vec::new();
    let mut other_items: Vec<(usize, &Statement)> = Vec::new();

    for (idx, statement) in statements.iter().enumerate() {
        match statement {
            Statement::Item(Item::Struct(def)) => {
                struct_items.push((idx, def));
            }
            Statement::Item(Item::Function(func)) => {
                function_items.push((idx, func));
            }
            other => {
                other_items.push((idx, other));
            }
        }
    }

    // Phase 2a: Register all struct names first (for mutual references)
    for (_, def) in &struct_items {
        let mut type_var_ids = Vec::new();
        for _ in &def.type_params {
            let var = ctx.fresh_var();
            if let Type::Var(id) = var {
                type_var_ids.push(id);
            }
        }
        env.structs.insert(
            def.name.clone(),
            StructType {
                name: def.name.clone(),
                type_params: def.type_params.clone(),
                type_var_ids,
                fields: vec![],
            },
        );
    }

    // Phase 2b: Now resolve all struct field types
    for (_, def) in &struct_items {
        let struct_type = struct_type_from_def(def, env, &mut ctx)?;
        env.structs.insert(def.name.clone(), struct_type);
    }

    // Phase 3: Register all function signatures
    for (_, func) in &function_items {
        let func_type = function_type_from_def(func, env, &mut ctx)?;
        env.functions.insert(func.name.clone(), func_type);
    }

    // Phase 4: Type-check all items
    let mut results: Vec<(usize, CheckedStatement)> = Vec::new();

    // Add struct results
    for (idx, def) in &struct_items {
        results.push((*idx, CheckedStatement::Struct((*def).clone())));
    }

    // Type-check function bodies
    for (idx, func) in &function_items {
        let typed_func = check_function(func, env, &mut ctx)?;
        results.push((*idx, CheckedStatement::Function(typed_func)));
    }

    // Phase 5: Type-check non-item statements in order
    for (idx, statement) in other_items {
        match statement {
            Statement::Expr(expr) => {
                let typed_expr = check_with_env(expr, env, &mut ctx)?;
                results.push((idx, CheckedStatement::Expr(typed_expr)));
            }
            Statement::Let(binding) => {
                let typed_binding = check_let_binding(binding, env, &mut ctx)?;

                // Apply let polymorphism (value restriction)
                if is_syntactic_value(&binding.value) {
                    let fixed_vars = env.free_vars(&ctx);
                    let scheme = ctx.generalize(&typed_binding.ty, &fixed_vars);
                    env.poly_locals.insert(binding.name.clone(), scheme);
                } else {
                    env.locals
                        .insert(binding.name.clone(), typed_binding.ty.clone());
                }

                results.push((idx, CheckedStatement::Let(typed_binding)));
            }
            Statement::Item(_) => unreachable!("Items already handled"),
        }
    }

    // Phase 6: Sort by original index to preserve input order
    results.sort_by_key(|(idx, _)| *idx);
    Ok(results.into_iter().map(|(_, stmt)| stmt).collect())
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
    fn test_check_int64() {
        let expr = Expr::Int64(42);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int64);
        assert_eq!(result, TypedExpr::Int64(42));
    }

    #[test]
    fn test_check_int64_large() {
        let expr = Expr::Int64(9_000_000_000);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int64);
        assert_eq!(result, TypedExpr::Int64(9_000_000_000));
    }

    #[test]
    fn test_check_int64_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int64(1)),
            right: Box::new(Expr::Int64(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int64);
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
    fn test_check_negate_bool_error() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Bool(true)),
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("negation"));
    }

    #[test]
    fn test_check_negate_string_error() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::String("hello".to_string())),
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("negation"));
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

        let env = TypeEnv::default();
        let ft = function_type_from_def(&func, &env, &mut ctx).unwrap();
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

        let env = TypeEnv::default();
        let ft = function_type_from_def(&func, &env, &mut ctx).unwrap();
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
        // Variable should be added to env (in poly_locals since it's a syntactic value)
        let scheme = env.poly_locals.get("x").expect("x should be in poly_locals");
        assert_eq!(scheme.ty, Type::Int32);
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
        // Variable should be in poly_locals since it's a syntactic value
        let scheme = env.poly_locals.get("x").expect("x should be in poly_locals");
        assert_eq!(scheme.ty, Type::Int32);
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
                MatchArm {
                    pattern: Pattern::Wildcard,
                    result: Expr::String("other".to_string()),
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

    // Tests for List methods

    #[test]
    fn test_check_list_len() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "len".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int32);
    }

    #[test]
    fn test_check_list_is_empty() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![])),
            method: "is_empty".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Bool);
    }

    #[test]
    fn test_check_list_reverse() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "reverse".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int32)));
    }

    #[test]
    fn test_check_list_push() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "push".to_string(),
            args: vec![Expr::Int(3)],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int32)));
    }

    #[test]
    fn test_check_list_push_type_mismatch() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "push".to_string(),
            args: vec![Expr::String("hello".to_string())],
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_check_list_concat() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "concat".to_string(),
            args: vec![Expr::List(vec![Expr::Int(3), Expr::Int(4)])],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int32)));
    }

    #[test]
    fn test_check_list_concat_type_mismatch() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "concat".to_string(),
            args: vec![Expr::List(vec![Expr::String("hello".to_string())])],
        };
        let result = check(&expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_check_list_chained_methods() {
        // [1, 2].push(3).reverse()
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::MethodCall {
                receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
                method: "push".to_string(),
                args: vec![Expr::Int(3)],
            }),
            method: "reverse".to_string(),
            args: vec![],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int32)));
    }

    // Tests for multi-pass type checking in REPL mode

    #[test]
    fn test_check_repl_forward_reference() {
        // fn caller() -> Int32 callee()
        // fn callee() -> Int32 42
        // Should succeed - caller can reference callee defined later
        let mut env = TypeEnv::default();
        let stmts = vec![
            Statement::Item(Item::Function(FunctionDef {
                name: "caller".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named("Int32".to_string())),
                body: Expr::Call {
                    func: "callee".to_string(),
                    args: vec![],
                },
            })),
            Statement::Item(Item::Function(FunctionDef {
                name: "callee".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named("Int32".to_string())),
                body: Expr::Int(42),
            })),
        ];
        let result = check_repl(&stmts, &mut env);
        assert!(result.is_ok(), "Forward reference should succeed: {:?}", result.err());
        let checked = result.unwrap();
        assert_eq!(checked.len(), 2);
        // Results should be in original order
        assert!(matches!(checked[0], CheckedStatement::Function(_)));
        assert!(matches!(checked[1], CheckedStatement::Function(_)));
    }

    #[test]
    fn test_check_repl_mutual_recursion() {
        // fn is_even(n) -> Bool { match n { 0 => true, _ => is_odd(n-1) } }
        // fn is_odd(n) -> Bool { match n { 0 => false, _ => is_even(n-1) } }
        // Should succeed - both see each other
        let mut env = TypeEnv::default();
        let stmts = vec![
            Statement::Item(Item::Function(FunctionDef {
                name: "is_even".to_string(),
                type_params: vec![],
                params: vec![Param {
                    name: "n".to_string(),
                    typ: TypeAnnotation::Named("Int32".to_string()),
                }],
                return_type: Some(TypeAnnotation::Named("Bool".to_string())),
                body: Expr::Match {
                    scrutinee: Box::new(Expr::Var("n".to_string())),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                            result: Expr::Bool(true),
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            result: Expr::Call {
                                func: "is_odd".to_string(),
                                args: vec![Expr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(Expr::Var("n".to_string())),
                                    right: Box::new(Expr::Int(1)),
                                }],
                            },
                        },
                    ],
                },
            })),
            Statement::Item(Item::Function(FunctionDef {
                name: "is_odd".to_string(),
                type_params: vec![],
                params: vec![Param {
                    name: "n".to_string(),
                    typ: TypeAnnotation::Named("Int32".to_string()),
                }],
                return_type: Some(TypeAnnotation::Named("Bool".to_string())),
                body: Expr::Match {
                    scrutinee: Box::new(Expr::Var("n".to_string())),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                            result: Expr::Bool(false),
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            result: Expr::Call {
                                func: "is_even".to_string(),
                                args: vec![Expr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(Expr::Var("n".to_string())),
                                    right: Box::new(Expr::Int(1)),
                                }],
                            },
                        },
                    ],
                },
            })),
        ];
        let result = check_repl(&stmts, &mut env);
        assert!(result.is_ok(), "Mutual recursion should succeed: {:?}", result.err());
    }

    #[test]
    fn test_check_repl_mixed_preserves_order() {
        // let x = 1
        // fn f2() -> Int32 f1()  (forward ref)
        // x + 1
        // fn f1() -> Int32 42
        // Results should be in original order: [Let, Function, Expr, Function]
        let mut env = TypeEnv::default();
        let stmts = vec![
            Statement::Let(LetBinding {
                name: "x".to_string(),
                type_annotation: None,
                value: Box::new(Expr::Int(1)),
            }),
            Statement::Item(Item::Function(FunctionDef {
                name: "f2".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named("Int32".to_string())),
                body: Expr::Call {
                    func: "f1".to_string(),
                    args: vec![],
                },
            })),
            Statement::Expr(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Int(1)),
            }),
            Statement::Item(Item::Function(FunctionDef {
                name: "f1".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named("Int32".to_string())),
                body: Expr::Int(42),
            })),
        ];
        let result = check_repl(&stmts, &mut env);
        assert!(result.is_ok(), "Mixed statements should succeed: {:?}", result.err());
        let checked = result.unwrap();
        assert_eq!(checked.len(), 4);
        // Verify order is preserved
        assert!(matches!(checked[0], CheckedStatement::Let(_)), "First should be Let");
        assert!(matches!(checked[1], CheckedStatement::Function(_)), "Second should be Function f2");
        assert!(matches!(checked[2], CheckedStatement::Expr(_)), "Third should be Expr");
        assert!(matches!(checked[3], CheckedStatement::Function(_)), "Fourth should be Function f1");
    }

    #[test]
    fn test_check_repl_let_not_visible_in_function() {
        // let x = 42
        // fn bad() -> Int32 x
        // Should fail: "undefined variable 'x'"
        let mut env = TypeEnv::default();
        let stmts = vec![
            Statement::Let(LetBinding {
                name: "x".to_string(),
                type_annotation: None,
                value: Box::new(Expr::Int(42)),
            }),
            Statement::Item(Item::Function(FunctionDef {
                name: "bad".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named("Int32".to_string())),
                body: Expr::Var("x".to_string()),
            })),
        ];
        let result = check_repl(&stmts, &mut env);
        assert!(result.is_err(), "Let should not be visible in function, but got: {:?}", result);
        let err_msg = result.unwrap_err().message;
        assert!(
            err_msg.contains("unknown variable"),
            "Expected 'unknown variable' but got: {}", err_msg
        );
    }

    #[test]
    fn test_check_repl_self_recursion() {
        // fn factorial(n) -> Int32 { match n { 0 => 1, _ => n * factorial(n-1) } }
        // Should succeed
        let mut env = TypeEnv::default();
        let stmts = vec![Statement::Item(Item::Function(FunctionDef {
            name: "factorial".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "n".to_string(),
                typ: TypeAnnotation::Named("Int32".to_string()),
            }],
            return_type: Some(TypeAnnotation::Named("Int32".to_string())),
            body: Expr::Match {
                scrutinee: Box::new(Expr::Var("n".to_string())),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                        result: Expr::Int(1),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        result: Expr::BinOp {
                            op: BinOp::Mul,
                            left: Box::new(Expr::Var("n".to_string())),
                            right: Box::new(Expr::Call {
                                func: "factorial".to_string(),
                                args: vec![Expr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(Expr::Var("n".to_string())),
                                    right: Box::new(Expr::Int(1)),
                                }],
                            }),
                        },
                    },
                ],
            },
        }))];
        let result = check_repl(&stmts, &mut env);
        assert!(result.is_ok(), "Self-recursion should succeed: {:?}", result.err());
    }

    #[test]
    fn test_substitute_type_vars_in_list() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let Type::Var(id) = var else { panic!("expected type var") };

        let mut mapping = HashMap::new();
        mapping.insert(id, Type::Int32);

        let ty = Type::List(Box::new(Type::Var(id)));
        let result = substitute_type_vars(&ty, &mapping);
        assert_eq!(result, Type::List(Box::new(Type::Int32)));
    }

    #[test]
    fn test_substitute_type_vars_in_tuple() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let Type::Var(id) = var else { panic!("expected type var") };

        let mut mapping = HashMap::new();
        mapping.insert(id, Type::String);

        let ty = Type::Tuple(vec![Type::Var(id), Type::Int32]);
        let result = substitute_type_vars(&ty, &mapping);
        assert_eq!(result, Type::Tuple(vec![Type::String, Type::Int32]));
    }

    #[test]
    fn test_substitute_type_vars_in_function() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let Type::Var(id) = var else { panic!("expected type var") };

        let mut mapping = HashMap::new();
        mapping.insert(id, Type::Bool);

        let ty = Type::Function {
            params: vec![Type::Var(id)],
            ret: Box::new(Type::Var(id)),
        };
        let result = substitute_type_vars(&ty, &mapping);
        assert_eq!(
            result,
            Type::Function {
                params: vec![Type::Bool],
                ret: Box::new(Type::Bool),
            }
        );
    }

    #[test]
    fn test_substitute_type_vars_nested() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let Type::Var(id) = var else { panic!("expected type var") };

        let mut mapping = HashMap::new();
        mapping.insert(id, Type::Float);

        // List<(T, T)> -> List<(Float, Float)>
        let ty = Type::List(Box::new(Type::Tuple(vec![Type::Var(id), Type::Var(id)])));
        let result = substitute_type_vars(&ty, &mapping);
        assert_eq!(
            result,
            Type::List(Box::new(Type::Tuple(vec![Type::Float, Type::Float])))
        );
    }
}
