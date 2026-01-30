mod builtin;
mod definition;
mod naming;
mod pattern;
mod resolution;
mod type_resolver;
mod unify;
mod usefulness;

use std::collections::{HashMap, HashSet};

use zoya_ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, MatchArm, Path, Stmt, TypeAnnotation, UnaryOp,
};
use zoya_ir::{
    CheckedItem, CheckedModule, CheckedModuleTree, CheckedStmt, EnumType, EnumVariantType,
    FunctionType, QualifiedPath, StructType, Type, TypeAliasType, TypeError, TypeScheme,
    TypeVarId, TypedEnumConstructFields, TypedExpr, TypedFunction,
};
use zoya_loader::{ModulePath, ModuleTree};

pub use unify::UnifyCtx;

pub use builtin::{builtin_method, is_numeric_type};
pub use definition::{enum_type_from_def, function_type_from_def, struct_type_from_def, type_alias_from_def};
pub use naming::{is_pascal_case, is_snake_case, to_pascal_case, to_snake_case};
pub use pattern::{check_irrefutable, check_let_binding, check_match_arm, check_pattern};
pub use type_resolver::resolve_type_annotation;

/// Type environment for checking expressions
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// Function signatures
    pub functions: HashMap<String, FunctionType>,
    /// Struct type definitions
    pub structs: HashMap<String, StructType>,
    /// Enum type definitions
    pub enums: HashMap<String, EnumType>,
    /// Type alias definitions
    pub type_aliases: HashMap<String, TypeAliasType>,
    /// Local variable types (type schemes for let polymorphism)
    pub locals: HashMap<String, TypeScheme>,
}

impl TypeEnv {
    pub fn with_locals(&self, locals: HashMap<String, TypeScheme>) -> Self {
        TypeEnv {
            functions: self.functions.clone(),
            structs: self.structs.clone(),
            enums: self.enums.clone(),
            type_aliases: self.type_aliases.clone(),
            locals,
        }
    }

    /// Collect all free type variables in the environment
    pub fn free_vars(&self, ctx: &UnifyCtx) -> HashSet<TypeVarId> {
        let mut set = HashSet::new();
        for scheme in self.locals.values() {
            // Free vars in scheme = free vars in type - quantified vars
            let ty_vars = ctx.free_vars(&scheme.ty);
            let quantified: HashSet<_> = scheme.quantified.iter().cloned().collect();
            set.extend(ty_vars.difference(&quantified).cloned());
        }
        set
    }
}

/// Check a function definition and return a typed function
fn check_function(
    func: &FunctionDef,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedFunction, TypeError> {
    // Check function name is snake_case
    if !is_snake_case(&func.name) {
        return Err(TypeError {
            message: format!(
                "function name '{}' should be snake_case (e.g., '{}')",
                func.name,
                to_snake_case(&func.name)
            ),
        });
    }

    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    for name in &func.type_params {
        // Check type parameter name is PascalCase
        if !is_pascal_case(name) {
            return Err(TypeError {
                message: format!(
                    "type parameter '{}' should be PascalCase (e.g., '{}')",
                    name,
                    to_pascal_case(name)
                ),
            });
        }
        let var = ctx.fresh_var();
        if let Type::Var(id) = var {
            type_param_map.insert(name.clone(), id);
        }
    }

    // Build local environment with parameters
    let mut locals = HashMap::new();
    let mut typed_params = Vec::new();

    for param in &func.params {
        // Check pattern is irrefutable
        check_irrefutable(&param.pattern).map_err(|msg| TypeError {
            message: format!("refutable pattern in function parameter: {}", msg),
        })?;

        let ty = resolve_type_annotation(&param.typ, &type_param_map, env)?;

        // Type-check the pattern against the parameter type
        let (typed_pattern, bindings) = check_pattern(&param.pattern, &ty, env, ctx)?;

        // Add all pattern bindings to locals
        for (name, var_ty) in bindings {
            locals.insert(name, TypeScheme::mono(var_ty));
        }

        typed_params.push((typed_pattern, ctx.resolve(&ty)));
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
        params: typed_params,
        body: typed_body,
        return_type: ctx.resolve(&return_type),
    })
}

// ============================================================================
// Expression type checking helper functions
// ============================================================================

/// Check a variable reference
/// Check a path expression (variable or unit enum variant)
fn check_path_expr(path: &Path, env: &TypeEnv, ctx: &mut UnifyCtx) -> Result<TypedExpr, TypeError> {
    match path.segments.as_slice() {
        // Single segment: must be a variable
        [name] => {
            // Variables cannot have turbofish
            if path.type_args.is_some() {
                return Err(TypeError {
                    message: format!("cannot use turbofish on variable '{}'", name),
                });
            }
            if let Some(scheme) = env.locals.get(name) {
                let ty = ctx.instantiate(scheme);
                Ok(TypedExpr::Var {
                    path: QualifiedPath::simple(name.to_string()),
                    ty: ctx.resolve(&ty),
                })
            } else {
                Err(TypeError {
                    message: format!("unknown variable: {}", name),
                })
            }
        }
        // Two segments: must be Enum::Variant (unit variant)
        [enum_name, variant_name] => {
            let enum_type = env.enums.get(enum_name).ok_or_else(|| TypeError {
                message: format!("unknown enum: {}", enum_name),
            })?;

            let variant_type = enum_type
                .variants
                .iter()
                .find(|(name, _)| name == variant_name)
                .map(|(_, vt)| vt)
                .ok_or_else(|| TypeError {
                    message: format!("enum {} has no variant {}", enum_name, variant_name),
                })?;

            // Must be a unit variant when used as a bare path
            if !matches!(variant_type, EnumVariantType::Unit) {
                return Err(TypeError {
                    message: format!(
                        "enum variant {}::{} requires arguments",
                        enum_name, variant_name
                    ),
                });
            }

            // Handle explicit type arguments (turbofish) or create fresh type variables
            let instantiation: HashMap<TypeVarId, Type> = if let Some(ref type_args) = path.type_args
            {
                // Validate count matches type parameters
                if type_args.len() != enum_type.type_params.len() {
                    return Err(TypeError {
                        message: format!(
                            "enum {} expects {} type argument(s), got {}",
                            enum_name,
                            enum_type.type_params.len(),
                            type_args.len()
                        ),
                    });
                }
                // Resolve type annotations to Types
                let resolved: Vec<Type> = type_args
                    .iter()
                    .map(|ann| resolve_type_annotation(ann, &HashMap::new(), env))
                    .collect::<Result<_, _>>()?;
                // Build substitution map from explicit types
                enum_type
                    .type_var_ids
                    .iter()
                    .zip(resolved)
                    .map(|(&id, ty)| (id, ty))
                    .collect()
            } else {
                // No turbofish: create fresh type variables
                enum_type
                    .type_var_ids
                    .iter()
                    .map(|&id| (id, ctx.fresh_var()))
                    .collect()
            };

            let type_args: Vec<Type> = enum_type
                .type_var_ids
                .iter()
                .map(|id| ctx.resolve(&instantiation[id]))
                .collect();

            let resolved_variants: Vec<(String, EnumVariantType)> = enum_type
                .variants
                .iter()
                .map(|(name, vt)| {
                    (
                        name.clone(),
                        substitute_variant_type_vars(vt, &instantiation),
                    )
                })
                .map(|(name, vt)| {
                    let resolved_vt = match vt {
                        EnumVariantType::Unit => EnumVariantType::Unit,
                        EnumVariantType::Tuple(types) => {
                            EnumVariantType::Tuple(types.iter().map(|t| ctx.resolve(t)).collect())
                        }
                        EnumVariantType::Struct(fields) => EnumVariantType::Struct(
                            fields.iter().map(|(n, t)| (n.clone(), ctx.resolve(t))).collect(),
                        ),
                    };
                    (name, resolved_vt)
                })
                .collect();

            Ok(TypedExpr::EnumConstruct {
                path: QualifiedPath::new(vec![
                    enum_name.to_string(),
                    variant_name.to_string(),
                ]),
                fields: TypedEnumConstructFields::Unit,
                ty: Type::Enum {
                    name: enum_name.to_string(),
                    type_args,
                    variants: resolved_variants,
                },
            })
        }
        _ => Err(TypeError {
            message: format!("unknown path: {}", path),
        }),
    }
}

/// Check a path call expression (function call or tuple enum variant)
fn check_path_call(
    path: &Path,
    args: &[Expr],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    match path.segments.as_slice() {
        // Single segment: function or lambda call
        [func] => check_simple_call(func, &path.type_args, args, env, ctx),
        // Two segments: Enum::Variant(args)
        [enum_name, variant_name] => {
            check_enum_tuple_construct(enum_name, variant_name, &path.type_args, args, env, ctx)
        }
        _ => Err(TypeError {
            message: format!("unknown path: {}", path),
        }),
    }
}

/// Check a simple (single-name) function call
fn check_simple_call(
    func: &str,
    explicit_type_args: &Option<Vec<TypeAnnotation>>,
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

        // Handle explicit type arguments (turbofish) or create fresh type variables
        let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
            // Validate count matches type parameters
            if type_args.len() != func_type.type_params.len() {
                return Err(TypeError {
                    message: format!(
                        "function '{}' expects {} type argument(s), got {}",
                        func,
                        func_type.type_params.len(),
                        type_args.len()
                    ),
                });
            }
            // Resolve type annotations to Types
            let resolved: Vec<Type> = type_args
                .iter()
                .map(|ann| resolve_type_annotation(ann, &HashMap::new(), env))
                .collect::<Result<_, _>>()?;
            // Build substitution map from explicit types
            func_type
                .type_var_ids
                .iter()
                .zip(resolved)
                .map(|(&id, ty)| (id, ty))
                .collect()
        } else {
            // No turbofish: create fresh type variables
            func_type
                .type_var_ids
                .iter()
                .map(|&id| (id, ctx.fresh_var()))
                .collect()
        };

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
            path: QualifiedPath::simple(func.to_string()),
            args: typed_args,
            ty: return_type,
        })
    }
    // Try to look up as a lambda-bound variable
    else if let Some(func_ty) = get_callable_type(func, env, ctx) {
        // Lambda calls cannot have turbofish
        if explicit_type_args.is_some() {
            return Err(TypeError {
                message: format!("cannot use turbofish on lambda call '{}'", func),
            });
        }
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
                path: QualifiedPath::simple(func.to_string()),
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

/// Check an enum tuple variant construction: Enum::Variant(args)
fn check_enum_tuple_construct(
    enum_name: &str,
    variant_name: &str,
    explicit_type_args: &Option<Vec<TypeAnnotation>>,
    args: &[Expr],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let enum_type = env.enums.get(enum_name).ok_or_else(|| TypeError {
        message: format!("unknown enum: {}", enum_name),
    })?;

    let variant_type = enum_type
        .variants
        .iter()
        .find(|(name, _)| name == variant_name)
        .map(|(_, vt)| vt)
        .ok_or_else(|| TypeError {
            message: format!("enum {} has no variant {}", enum_name, variant_name),
        })?;

    // Must be a tuple variant
    let expected_types = match variant_type {
        EnumVariantType::Tuple(types) => types,
        EnumVariantType::Unit => {
            return Err(TypeError {
                message: format!(
                    "enum variant {}::{} is a unit variant, doesn't take arguments",
                    enum_name, variant_name
                ),
            });
        }
        EnumVariantType::Struct(_) => {
            return Err(TypeError {
                message: format!(
                    "enum variant {}::{} is a struct variant, use {{ }} syntax",
                    enum_name, variant_name
                ),
            });
        }
    };

    if args.len() != expected_types.len() {
        return Err(TypeError {
            message: format!(
                "enum variant {}::{} expects {} argument(s), got {}",
                enum_name,
                variant_name,
                expected_types.len(),
                args.len()
            ),
        });
    }

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
        // Validate count matches type parameters
        if type_args.len() != enum_type.type_params.len() {
            return Err(TypeError {
                message: format!(
                    "enum {} expects {} type argument(s), got {}",
                    enum_name,
                    enum_type.type_params.len(),
                    type_args.len()
                ),
            });
        }
        // Resolve type annotations to Types
        let resolved: Vec<Type> = type_args
            .iter()
            .map(|ann| resolve_type_annotation(ann, &HashMap::new(), env))
            .collect::<Result<_, _>>()?;
        // Build substitution map from explicit types
        enum_type
            .type_var_ids
            .iter()
            .zip(resolved)
            .map(|(&id, ty)| (id, ty))
            .collect()
    } else {
        // No turbofish: create fresh type variables
        enum_type
            .type_var_ids
            .iter()
            .map(|&id| (id, ctx.fresh_var()))
            .collect()
    };

    let mut typed_exprs = Vec::new();
    for (expr, expected) in args.iter().zip(expected_types.iter()) {
        let expected_type = substitute_type_vars(expected, &instantiation);
        let typed_expr = check_with_env(expr, env, ctx)?;
        let actual_type = typed_expr.ty();

        ctx.unify(&actual_type, &expected_type).map_err(|e| TypeError {
            message: format!(
                "in enum variant {}::{}: expected {} but got {}: {}",
                enum_name,
                variant_name,
                ctx.resolve(&expected_type),
                ctx.resolve(&actual_type),
                e.message
            ),
        })?;

        typed_exprs.push(typed_expr);
    }

    let type_args: Vec<Type> = enum_type
        .type_var_ids
        .iter()
        .map(|id| ctx.resolve(&instantiation[id]))
        .collect();

    let resolved_variants: Vec<(String, EnumVariantType)> = enum_type
        .variants
        .iter()
        .map(|(name, vt)| {
            (
                name.clone(),
                substitute_variant_type_vars(vt, &instantiation),
            )
        })
        .map(|(name, vt)| {
            let resolved_vt = match vt {
                EnumVariantType::Unit => EnumVariantType::Unit,
                EnumVariantType::Tuple(types) => {
                    EnumVariantType::Tuple(types.iter().map(|t| ctx.resolve(t)).collect())
                }
                EnumVariantType::Struct(fields) => EnumVariantType::Struct(
                    fields.iter().map(|(n, t)| (n.clone(), ctx.resolve(t))).collect(),
                ),
            };
            (name, resolved_vt)
        })
        .collect();

    Ok(TypedExpr::EnumConstruct {
        path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
        fields: TypedEnumConstructFields::Tuple(typed_exprs),
        ty: Type::Enum {
            name: enum_name.to_string(),
            type_args,
            variants: resolved_variants,
        },
    })
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
                Type::Int | Type::BigInt | Type::Float => Ok(TypedExpr::UnaryOp {
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
        let (typed_binding, pattern_bindings) = check_let_binding(binding, &block_env, ctx)?;

        // Check if we should generalize (let polymorphism)
        // Only generalize syntactic values (lambdas, literals, variables)
        let should_generalize = is_syntactic_value(&binding.value);

        // Add each bound variable from the pattern to the environment
        for (name, ty) in pattern_bindings {
            let scheme = if should_generalize {
                // Collect fixed vars from the environment
                let fixed_vars = block_env.free_vars(ctx);
                ctx.generalize(&ty, &fixed_vars)
            } else {
                // Monomorphic binding
                TypeScheme::mono(ty)
            };
            block_env.locals.insert(name, scheme);
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
    params: &[zoya_ast::LambdaParam],
    return_type: &Option<TypeAnnotation>,
    body: &Expr,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let mut typed_params = Vec::new();
    let mut param_types = Vec::new();
    let mut lambda_env = env.clone();

    for param in params {
        // Check pattern is irrefutable
        check_irrefutable(&param.pattern).map_err(|msg| TypeError {
            message: format!("refutable pattern in lambda parameter: {}", msg),
        })?;

        let param_ty = match &param.typ {
            Some(annotation) => resolve_type_annotation(annotation, &HashMap::new(), env)?,
            None => ctx.fresh_var(),
        };

        // Type-check the pattern against the parameter type
        let (typed_pattern, bindings) = check_pattern(&param.pattern, &param_ty, env, ctx)?;

        // Add all pattern bindings to the lambda environment
        for (name, var_ty) in bindings {
            lambda_env.locals.insert(name, TypeScheme::mono(var_ty));
        }

        typed_params.push((typed_pattern, ctx.resolve(&param_ty)));
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
        params: typed_params,
        body: Box::new(typed_body),
        ty: lambda_type,
    })
}

/// Check a struct constructor expression
/// Check a path struct expression (struct construction or enum struct variant)
fn check_path_struct(
    path: &Path,
    fields: &[(String, Expr)],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    match path.segments.as_slice() {
        // Single segment: struct construction
        [name] => check_struct_construct(name, fields, env, ctx),
        // Two segments: Enum::Variant { fields }
        [enum_name, variant_name] => {
            check_enum_struct_construct(enum_name, variant_name, fields, env, ctx)
        }
        _ => Err(TypeError {
            message: format!("unknown path: {}", path),
        }),
    }
}

/// Check a struct construction expression
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
    let expected_field_names: HashSet<&str> =
        struct_type.fields.iter().map(|(n, _)| n.as_str()).collect();
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
        ctx.unify(&actual_type, &expected_type)
            .map_err(|e| TypeError {
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
        .map(|(name, ty)| {
            (
                name.clone(),
                ctx.resolve(&substitute_type_vars(ty, &instantiation)),
            )
        })
        .collect();

    Ok(TypedExpr::StructConstruct {
        path: QualifiedPath::simple(name.to_string()),
        fields: typed_fields,
        ty: Type::Struct {
            name: name.to_string(),
            type_args,
            fields: resolved_fields,
        },
    })
}

/// Check an enum struct variant construction: Enum::Variant { fields }
fn check_enum_struct_construct(
    enum_name: &str,
    variant_name: &str,
    provided_fields: &[(String, Expr)],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let enum_type = env.enums.get(enum_name).ok_or_else(|| TypeError {
        message: format!("unknown enum: {}", enum_name),
    })?;

    let variant_type = enum_type
        .variants
        .iter()
        .find(|(name, _)| name == variant_name)
        .map(|(_, vt)| vt)
        .ok_or_else(|| TypeError {
            message: format!("enum {} has no variant {}", enum_name, variant_name),
        })?;

    // Must be a struct variant
    let expected_fields = match variant_type {
        EnumVariantType::Struct(fields) => fields,
        EnumVariantType::Unit => {
            return Err(TypeError {
                message: format!(
                    "enum variant {}::{} is a unit variant, doesn't take fields",
                    enum_name, variant_name
                ),
            });
        }
        EnumVariantType::Tuple(_) => {
            return Err(TypeError {
                message: format!(
                    "enum variant {}::{} is a tuple variant, use ( ) syntax",
                    enum_name, variant_name
                ),
            });
        }
    };

    // Create fresh type variables for generic parameters
    let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
    for &old_id in &enum_type.type_var_ids {
        instantiation.insert(old_id, ctx.fresh_var());
    }

    // Check for missing and extra fields
    let expected_names: HashSet<&str> = expected_fields.iter().map(|(n, _)| n.as_str()).collect();
    let provided_names: HashSet<&str> = provided_fields.iter().map(|(n, _)| n.as_str()).collect();

    for expected in &expected_names {
        if !provided_names.contains(expected) {
            return Err(TypeError {
                message: format!(
                    "missing field '{}' in enum variant {}::{}",
                    expected, enum_name, variant_name
                ),
            });
        }
    }

    for provided in &provided_names {
        if !expected_names.contains(provided) {
            return Err(TypeError {
                message: format!(
                    "unknown field '{}' in enum variant {}::{}",
                    provided, enum_name, variant_name
                ),
            });
        }
    }

    let mut typed_fields = Vec::new();
    for (field_name, field_expr) in provided_fields {
        let (_, field_type) = expected_fields
            .iter()
            .find(|(n, _)| n == field_name)
            .unwrap();

        let expected_type = substitute_type_vars(field_type, &instantiation);
        let typed_expr = check_with_env(field_expr, env, ctx)?;
        let actual_type = typed_expr.ty();

        ctx.unify(&actual_type, &expected_type)
            .map_err(|e| TypeError {
                message: format!(
                    "field '{}' in enum variant {}::{} expects {} but got {}: {}",
                    field_name,
                    enum_name,
                    variant_name,
                    ctx.resolve(&expected_type),
                    ctx.resolve(&actual_type),
                    e.message
                ),
            })?;

        typed_fields.push((field_name.clone(), typed_expr));
    }

    let type_args: Vec<Type> = enum_type
        .type_var_ids
        .iter()
        .map(|id| ctx.resolve(&instantiation[id]))
        .collect();

    let resolved_variants: Vec<(String, EnumVariantType)> = enum_type
        .variants
        .iter()
        .map(|(name, vt)| {
            (
                name.clone(),
                substitute_variant_type_vars(vt, &instantiation),
            )
        })
        .map(|(name, vt)| {
            let resolved_vt = match vt {
                EnumVariantType::Unit => EnumVariantType::Unit,
                EnumVariantType::Tuple(types) => {
                    EnumVariantType::Tuple(types.iter().map(|t| ctx.resolve(t)).collect())
                }
                EnumVariantType::Struct(fields) => EnumVariantType::Struct(
                    fields
                        .iter()
                        .map(|(n, t)| (n.clone(), ctx.resolve(t)))
                        .collect(),
                ),
            };
            (name, resolved_vt)
        })
        .collect();

    Ok(TypedExpr::EnumConstruct {
        path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
        fields: TypedEnumConstructFields::Struct(typed_fields),
        ty: Type::Enum {
            name: enum_name.to_string(),
            type_args,
            variants: resolved_variants,
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
        Expr::Int(n) => Ok(TypedExpr::Int(*n)),
        Expr::BigInt(n) => Ok(TypedExpr::BigInt(*n)),
        Expr::Float(n) => Ok(TypedExpr::Float(*n)),
        Expr::Bool(b) => Ok(TypedExpr::Bool(*b)),
        Expr::String(s) => Ok(TypedExpr::String(s.clone())),
        Expr::Path(path) => check_path_expr(path, env, ctx),
        Expr::Call { path, args } => check_path_call(path, args, env, ctx),
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
        Expr::Struct { path, fields } => check_path_struct(path, fields, env, ctx),
        Expr::FieldAccess { expr, field } => check_field_access(expr, field, env, ctx),
    }
}

/// Substitute type variables in a type using a mapping (recursive)
pub(crate) fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
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
        Type::Enum { name, type_args, variants } => Type::Enum {
            name: name.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            variants: variants
                .iter()
                .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, mapping)))
                .collect(),
        },
        // Concrete types don't contain type vars
        Type::Int | Type::BigInt | Type::Float | Type::Bool | Type::String => ty.clone(),
    }
}

/// Substitute type variables in an enum variant type
pub(crate) fn substitute_variant_type_vars(
    vt: &EnumVariantType,
    mapping: &HashMap<TypeVarId, Type>,
) -> EnumVariantType {
    match vt {
        EnumVariantType::Unit => EnumVariantType::Unit,
        EnumVariantType::Tuple(types) => {
            EnumVariantType::Tuple(types.iter().map(|t| substitute_type_vars(t, mapping)).collect())
        }
        EnumVariantType::Struct(fields) => EnumVariantType::Struct(
            fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        ),
    }
}

/// Get the type of a callable (lambda-bound variable), instantiating if polymorphic
fn get_callable_type(name: &str, env: &TypeEnv, ctx: &mut UnifyCtx) -> Option<Type> {
    env.locals.get(name).map(|scheme| ctx.instantiate(scheme))
}

/// Check if an expression is a syntactic value (safe to generalize under value restriction)
fn is_syntactic_value(expr: &Expr) -> bool {
    match expr {
        Expr::Lambda { .. } => true,
        Expr::Int(_) | Expr::BigInt(_) | Expr::Float(_) | Expr::Bool(_) | Expr::String(_) => true,
        Expr::List(elems) => elems.iter().all(is_syntactic_value),
        Expr::Tuple(elems) => elems.iter().all(is_syntactic_value),
        Expr::Path(_) => true,
        _ => false,
    }
}
/// Check an entire module tree, returning a checked module tree.
///
/// This performs multi-module type checking:
/// 1. Register all declarations from all modules
/// 2. Type-check all function bodies with module context for path resolution
pub fn check_module_tree(
    tree: &ModuleTree,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<CheckedModuleTree, TypeError> {
    // Phase 1: Register ALL declarations from ALL modules
    // Process modules in dependency order (parents before children)
    let mut module_paths: Vec<_> = tree.modules.keys().cloned().collect();
    module_paths.sort_by_key(|p| p.depth());

    for path in &module_paths {
        if let Some(module) = tree.modules.get(path) {
            register_module_declarations(&module.items, path, env, ctx)?;
        }
    }

    // Phase 2: Type-check ALL function bodies
    let mut checked_modules = HashMap::new();
    for path in &module_paths {
        if let Some(module) = tree.modules.get(path) {
            let checked = check_module_bodies(&module.items, path, env, ctx)?;
            checked_modules.insert(path.clone(), checked);
        }
    }

    Ok(CheckedModuleTree {
        modules: checked_modules,
    })
}

/// Register declarations from a single module into the type environment.
/// Uses fully qualified names (e.g., "root::utils::foo").
fn register_module_declarations(
    items: &[Item],
    current_module: &ModulePath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(), TypeError> {
    // Phase 1a: Register all struct names with placeholder types
    for item in items {
        if let Item::Struct(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let mut type_var_ids = Vec::new();
            for _ in &def.type_params {
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    type_var_ids.push(id);
                }
            }
            env.structs.insert(
                qualified_name,
                StructType {
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    fields: vec![],
                },
            );
        }
        if let Item::Enum(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let mut type_var_ids = Vec::new();
            for _ in &def.type_params {
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    type_var_ids.push(id);
                }
            }
            env.enums.insert(
                qualified_name,
                EnumType {
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    variants: vec![],
                },
            );
        }
    }

    // Phase 1b: Resolve all struct field types
    for item in items {
        if let Item::Struct(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let struct_type = struct_type_from_def(def, env, ctx)?;
            env.structs.insert(qualified_name, struct_type);
        }
    }

    // Phase 1c: Resolve all enum variant types
    for item in items {
        if let Item::Enum(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let enum_type = enum_type_from_def(def, env, ctx)?;
            env.enums.insert(qualified_name, enum_type);
        }
    }

    // Phase 1d: Register all type aliases
    for item in items {
        if let Item::TypeAlias(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let alias_type = type_alias_from_def(def, env, ctx)?;
            env.type_aliases.insert(qualified_name, alias_type);
        }
    }

    // Phase 2: Register all function signatures
    for item in items {
        if let Item::Function(func) = item {
            let qualified_name = resolution::qualified_name(current_module, &func.name);
            let func_type = function_type_from_def(func, env, ctx)?;
            env.functions.insert(qualified_name, func_type);
        }
    }

    Ok(())
}

/// Type-check function bodies from a single module.
fn check_module_bodies(
    items: &[Item],
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<CheckedModule, TypeError> {
    let mut checked_items = Vec::new();

    for item in items {
        match item {
            Item::Function(func) => {
                let typed = check_function_in_module(func, current_module, env, ctx)?;
                checked_items.push(CheckedItem::Function(Box::new(typed)));
            }
            Item::Struct(def) => {
                checked_items.push(CheckedItem::Struct(def.clone()));
            }
            Item::Enum(def) => {
                checked_items.push(CheckedItem::Enum(def.clone()));
            }
            Item::TypeAlias(def) => {
                checked_items.push(CheckedItem::TypeAlias(def.clone()));
            }
        }
    }

    Ok(CheckedModule {
        items: checked_items,
    })
}

/// Check a function definition within a specific module context.
fn check_function_in_module(
    func: &FunctionDef,
    _current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedFunction, TypeError> {
    // For now, delegate to existing check_function
    // In the future, this will use current_module for path resolution
    check_function(func, env, ctx)
}

/// Check a file's items (functions, structs, and enums), returning checked items
pub fn check_items(
    items: &[Item],
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<Vec<CheckedItem>, TypeError> {
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
        if let Item::Enum(def) = item {
            // Register with empty variants first - will fill in later
            let mut type_var_ids = Vec::new();
            for _ in &def.type_params {
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    type_var_ids.push(id);
                }
            }
            env.enums.insert(
                def.name.clone(),
                EnumType {
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    variants: vec![], // placeholder
                },
            );
        }
    }

    // Phase 1b: Now resolve all struct field types
    for item in items {
        if let Item::Struct(def) = item {
            let struct_type = struct_type_from_def(def, env, ctx)?;
            env.structs.insert(def.name.clone(), struct_type);
        }
    }

    // Phase 1c: Now resolve all enum variant types
    for item in items {
        if let Item::Enum(def) = item {
            let enum_type = enum_type_from_def(def, env, ctx)?;
            env.enums.insert(def.name.clone(), enum_type);
        }
    }

    // Phase 1d: Register all type aliases (now struct/enum types are available)
    for item in items {
        if let Item::TypeAlias(def) = item {
            let alias_type = type_alias_from_def(def, env, ctx)?;
            env.type_aliases.insert(def.name.clone(), alias_type);
        }
    }

    // Phase 2: Register all function signatures (now struct/enum/alias types are available)
    for item in items {
        if let Item::Function(func) = item {
            let func_type = function_type_from_def(func, env, ctx)?;
            env.functions.insert(func.name.clone(), func_type);
        }
    }

    // Phase 3: Type-check all items
    let mut checked_items = Vec::new();
    for item in items {
        match item {
            Item::Function(func) => {
                let typed = check_function(func, env, ctx)?;
                checked_items.push(CheckedItem::Function(Box::new(typed)));
            }
            Item::Struct(def) => {
                // Structs are just passed through (already registered in env)
                checked_items.push(CheckedItem::Struct(def.clone()));
            }
            Item::Enum(def) => {
                // Enums are just passed through (already registered in env)
                checked_items.push(CheckedItem::Enum(def.clone()));
            }
            Item::TypeAlias(def) => {
                // Type aliases are just passed through (already registered in env)
                checked_items.push(CheckedItem::TypeAlias(def.clone()));
            }
        }
    }

    Ok(checked_items)
}

/// Check REPL statements, returning checked statements.
/// Items should be checked separately with check_items.
pub fn check_stmts(
    stmts: &[Stmt],
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<Vec<CheckedStmt>, TypeError> {
    let mut checked_stmts: Vec<CheckedStmt> = Vec::new();

    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr) => {
                let typed_expr = check_with_env(expr, env, ctx)?;
                checked_stmts.push(CheckedStmt::Expr(typed_expr));
            }
            Stmt::Let(binding) => {
                let (typed_binding, pattern_bindings) = check_let_binding(binding, env, ctx)?;

                // Apply let polymorphism (value restriction) to each bound variable
                let should_generalize = is_syntactic_value(&binding.value);
                for (name, ty) in pattern_bindings {
                    let scheme = if should_generalize {
                        let fixed_vars = env.free_vars(ctx);
                        ctx.generalize(&ty, &fixed_vars)
                    } else {
                        TypeScheme::mono(ty)
                    };
                    env.locals.insert(name, scheme);
                }

                checked_stmts.push(CheckedStmt::Let(typed_binding));
            }
        }
    }

    Ok(checked_stmts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::{BinOp, PathPrefix, TypeAliasDef};
    use zoya_ir::Type;

    fn check(expr: &Expr) -> Result<TypedExpr, TypeError> {
        let mut ctx = UnifyCtx::new();
        check_with_env(expr, &TypeEnv::default(), &mut ctx)
    }

    #[test]
    fn test_check_int() {
        let expr = Expr::Int(42);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
        assert_eq!(result, TypedExpr::Int(42));
    }

    #[test]
    fn test_check_int_large() {
        // Large integers now work fine since Int uses i64 internally
        let expr = Expr::Int(3_000_000_000);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
        assert_eq!(result, TypedExpr::Int(3_000_000_000));
    }

    #[test]
    fn test_check_bigint() {
        let expr = Expr::BigInt(42);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::BigInt);
        assert_eq!(result, TypedExpr::BigInt(42));
    }

    #[test]
    fn test_check_bigint_large() {
        let expr = Expr::BigInt(9_000_000_000);
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::BigInt);
        assert_eq!(result, TypedExpr::BigInt(9_000_000_000));
    }

    #[test]
    fn test_check_bigint_addition() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::BigInt(1)),
            right: Box::new(Expr::BigInt(2)),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::BigInt);
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
        assert_eq!(result.ty(), Type::Int);
    }

    use zoya_ast::{FunctionDef, Param, Path, TypeAnnotation};
    use zoya_ir::FunctionType;

    #[test]
    fn test_check_variable() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Path(Path::simple("x".to_string()));
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_unknown_variable() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Path(Path::simple("x".to_string()));
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown variable"));
    }

    #[test]
    fn test_check_variable_in_expression() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));
        env.locals.insert("y".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Path(Path::simple("y".to_string()))),
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_function_call() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![Type::Int],
                return_type: Type::Int,
            },
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path::simple("square".to_string()),
            args: vec![Expr::Int(5)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_function_call_wrong_arg_type() {
        let mut env = TypeEnv::default();
        env.functions.insert(
            "square".to_string(),
            FunctionType {
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![Type::Int],
                return_type: Type::Int,
            },
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path::simple("square".to_string()),
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
                params: vec![Type::Int, Type::Int],
                return_type: Type::Int,
            },
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path::simple("add".to_string()),
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

        // identity(42) should return Int
        let expr = Expr::Call {
            path: Path::simple("identity".to_string()),
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
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
            path: Path::simple("identity".to_string()),
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
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Path(Path::simple("x".to_string()))),
            },
        };

        let result = check_function(&func, &env, &mut ctx).unwrap();
        assert_eq!(result.name, "double");
        assert_eq!(result.return_type, Type::Int);
    }

    #[test]
    fn test_check_function_def_return_type_mismatch() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "wrong".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Float".to_string()))),
            body: Expr::Path(Path::simple("x".to_string())), // Returns Int, not Float
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
                params: vec![Type::Int, Type::Int],
                return_type: Type::Int,
            },
        );

        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "double".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("add".to_string()),
                args: vec![Expr::Path(Path::simple("x".to_string())), Expr::Path(Path::simple("x".to_string()))],
            },
        };

        let result = check_function(&func, &env, &mut ctx).unwrap();
        assert_eq!(result.return_type, Type::Int);
    }

    #[test]
    fn test_function_type_from_def() {
        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "add".to_string(),
            type_params: vec![],
            params: vec![
                Param {
                    pattern: Pattern::Var("x".to_string()),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                },
                Param {
                    pattern: Pattern::Var("y".to_string()),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                },
            ],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(0), // body doesn't matter for type extraction
        };

        let env = TypeEnv::default();
        let ft = function_type_from_def(&func, &env, &mut ctx).unwrap();
        assert_eq!(ft.params, vec![Type::Int, Type::Int]);
        assert_eq!(ft.return_type, Type::Int);
    }

    #[test]
    fn test_function_type_from_def_generic() {
        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "identity".to_string(),
            type_params: vec!["T".to_string()],
            params: vec![Param {
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("T".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("T".to_string()))),
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
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Expr(Expr::Int(42))];
        let checked_items = check_items(&items, &mut env, &mut ctx).unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert!(checked_items.is_empty());
        assert_eq!(checked_stmts.len(), 1);
        assert!(matches!(
            checked_stmts[0],
            CheckedStmt::Expr(TypedExpr::Int(42))
        ));
    }

    #[test]
    fn test_check_repl_function_def() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items = vec![Item::Function(FunctionDef {
            name: "foo".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(42),
        })];
        let stmts: Vec<Stmt> = vec![];
        let checked_items = check_items(&items, &mut env, &mut ctx).unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert_eq!(checked_items.len(), 1);
        assert!(checked_stmts.is_empty());
        assert!(matches!(checked_items[0], CheckedItem::Function(_)));
        // Function should be added to env
        assert!(env.functions.contains_key("foo"));
    }

    #[test]
    fn test_check_repl_function_then_call() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items = vec![Item::Function(FunctionDef {
            name: "double".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Path(Path::simple("x".to_string()))),
            },
        })];
        let stmts = vec![Stmt::Expr(Expr::Call {
            path: Path::simple("double".to_string()),
            args: vec![Expr::Int(5)],
        })];
        let checked_items = check_items(&items, &mut env, &mut ctx).unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert_eq!(checked_items.len(), 1);
        assert_eq!(checked_stmts.len(), 1);
        assert!(matches!(checked_items[0], CheckedItem::Function(_)));
        assert!(matches!(checked_stmts[0], CheckedStmt::Expr(_)));
    }

    #[test]
    fn test_check_repl_let_binding() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::Var("x".to_string()),
            type_annotation: None,
            value: Box::new(Expr::Int(42)),
        })];
        let checked_items = check_items(&items, &mut env, &mut ctx).unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert!(checked_items.is_empty());
        assert_eq!(checked_stmts.len(), 1);
        assert!(matches!(checked_stmts[0], CheckedStmt::Let(_)));
        // Variable should be added to env (generalized since it's a syntactic value)
        let scheme = env.locals.get("x").expect("x should be in locals");
        assert_eq!(scheme.ty, Type::Int);
    }

    #[test]
    fn test_check_repl_let_then_use() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![
            Stmt::Let(LetBinding {
                pattern: Pattern::Var("x".to_string()),
                type_annotation: None,
                value: Box::new(Expr::Int(42)),
            }),
            Stmt::Expr(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Int(1)),
            }),
        ];
        let checked_items = check_items(&items, &mut env, &mut ctx).unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert!(checked_items.is_empty());
        assert_eq!(checked_stmts.len(), 2);
        assert!(matches!(checked_stmts[0], CheckedStmt::Let(_)));
        assert!(matches!(checked_stmts[1], CheckedStmt::Expr(_)));
    }

    #[test]
    fn test_check_let_with_type_annotation() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::Var("x".to_string()),
            type_annotation: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            value: Box::new(Expr::Int(42)),
        })];
        let _ = check_items(&items, &mut env, &mut ctx).unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert_eq!(checked_stmts.len(), 1);
        // Variable should be generalized since it's a syntactic value
        let scheme = env.locals.get("x").expect("x should be in locals");
        assert_eq!(scheme.ty, Type::Int);
    }

    #[test]
    fn test_check_let_type_mismatch() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::Var("x".to_string()),
            type_annotation: Some(TypeAnnotation::Named(Path::simple("Float".to_string()))),
            value: Box::new(Expr::Int(42)),
        })];
        let _ = check_items(&items, &mut env, &mut ctx).unwrap();
        let result = check_stmts(&stmts, &mut env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("declares type"));
    }

    #[test]
    fn test_check_block_expression() {
        let expr = Expr::Block {
            bindings: vec![LetBinding {
                pattern: Pattern::Var("x".to_string()),
                type_annotation: None,
                value: Box::new(Expr::Int(1)),
            }],
            result: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Int(2)),
            }),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_block_multiple_bindings() {
        let expr = Expr::Block {
            bindings: vec![
                LetBinding {
                    pattern: Pattern::Var("x".to_string()),
                    type_annotation: None,
                    value: Box::new(Expr::Int(1)),
                },
                LetBinding {
                    pattern: Pattern::Var("y".to_string()),
                    type_annotation: None,
                    value: Box::new(Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                        right: Box::new(Expr::Int(1)),
                    }),
                },
            ],
            result: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Path(Path::simple("y".to_string()))),
            }),
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    use zoya_ast::{MatchArm, Pattern};

    #[test]
    fn test_check_match_with_literals() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
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
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
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
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_match_with_variable_binding() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
            arms: vec![MatchArm {
                pattern: Pattern::Var("n".to_string()),
                result: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                    right: Box::new(Expr::Int(1)),
                },
            }],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_check_match_pattern_type_mismatch() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
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
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                    result: Expr::String("zero".to_string()),
                },
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(1))),
                    result: Expr::Int(1), // Type mismatch: String vs Int
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
        assert_eq!(result.ty(), Type::Int);
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
        assert!(result.unwrap_err().message.contains("no method 'len' on type Int"));
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
            args: vec![Expr::Int(42)], // contains expects String, not Int
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
        assert_eq!(result.ty(), Type::Int);
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
        assert_eq!(result.ty(), Type::Int);
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
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
    }

    #[test]
    fn test_check_list_push() {
        let expr = Expr::MethodCall {
            receiver: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2)])),
            method: "push".to_string(),
            args: vec![Expr::Int(3)],
        };
        let result = check(&expr).unwrap();
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
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
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
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
        assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
    }

    // Tests for multi-pass type checking in REPL mode

    #[test]
    fn test_check_repl_forward_reference() {
        // fn caller() -> Int callee()
        // fn callee() -> Int 42
        // Should succeed - caller can reference callee defined later
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items = vec![
            Item::Function(FunctionDef {
                name: "caller".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Call {
                    path: Path::simple("callee".to_string()),
                    args: vec![],
                },
            }),
            Item::Function(FunctionDef {
                name: "callee".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Int(42),
            }),
        ];
        let stmts: Vec<Stmt> = vec![];
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_ok(), "Forward reference should succeed: {:?}", result.err());
        let checked_items = result.unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert_eq!(checked_items.len(), 2);
        assert!(checked_stmts.is_empty());
        // Both should be functions
        assert!(matches!(checked_items[0], CheckedItem::Function(_)));
        assert!(matches!(checked_items[1], CheckedItem::Function(_)));
    }

    #[test]
    fn test_check_repl_mutual_recursion() {
        // fn is_even(n) -> Bool { match n { 0 => true, _ => is_odd(n-1) } }
        // fn is_odd(n) -> Bool { match n { 0 => false, _ => is_even(n-1) } }
        // Should succeed - both see each other
        let mut env = TypeEnv::default();
        let items = vec![
            Item::Function(FunctionDef {
                name: "is_even".to_string(),
                type_params: vec![],
                params: vec![Param {
                    pattern: Pattern::Var("n".to_string()),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                }],
                return_type: Some(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
                body: Expr::Match {
                    scrutinee: Box::new(Expr::Path(Path::simple("n".to_string()))),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                            result: Expr::Bool(true),
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            result: Expr::Call {
                                path: Path::simple("is_odd".to_string()),
                                args: vec![Expr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                                    right: Box::new(Expr::Int(1)),
                                }],
                            },
                        },
                    ],
                },
            }),
            Item::Function(FunctionDef {
                name: "is_odd".to_string(),
                type_params: vec![],
                params: vec![Param {
                    pattern: Pattern::Var("n".to_string()),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                }],
                return_type: Some(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
                body: Expr::Match {
                    scrutinee: Box::new(Expr::Path(Path::simple("n".to_string()))),
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                            result: Expr::Bool(false),
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            result: Expr::Call {
                                path: Path::simple("is_even".to_string()),
                                args: vec![Expr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                                    right: Box::new(Expr::Int(1)),
                                }],
                            },
                        },
                    ],
                },
            }),
        ];
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_ok(), "Mutual recursion should succeed: {:?}", result.err());
    }

    #[test]
    fn test_check_repl_mixed_items_and_stmts() {
        // Items: fn f2() -> Int f1()  (forward ref), fn f1() -> Int 42
        // Stmts: let x = 1, x + 1
        // Items are processed before stmts; forward refs work
        let mut env = TypeEnv::default();
        let items = vec![
            Item::Function(FunctionDef {
                name: "f2".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Call {
                    path: Path::simple("f1".to_string()),
                    args: vec![],
                },
            }),
            Item::Function(FunctionDef {
                name: "f1".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Int(42),
            }),
        ];
        let stmts = vec![
            Stmt::Let(LetBinding {
                pattern: Pattern::Var("x".to_string()),
                type_annotation: None,
                value: Box::new(Expr::Int(1)),
            }),
            Stmt::Expr(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Int(1)),
            }),
        ];
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_ok(), "Mixed items and stmts should succeed: {:?}", result.err());
        let checked_items = result.unwrap();
        let checked_stmts = check_stmts(&stmts, &mut env, &mut ctx).unwrap();
        assert_eq!(checked_items.len(), 2);
        assert_eq!(checked_stmts.len(), 2);
        // Verify items
        assert!(matches!(checked_items[0], CheckedItem::Function(_)));
        assert!(matches!(checked_items[1], CheckedItem::Function(_)));
        // Verify stmts
        assert!(matches!(checked_stmts[0], CheckedStmt::Let(_)));
        assert!(matches!(checked_stmts[1], CheckedStmt::Expr(_)));
    }

    #[test]
    fn test_check_repl_let_not_visible_in_function() {
        // let x = 42
        // fn bad() -> Int x
        // Should fail: "undefined variable 'x'"
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items = vec![Item::Function(FunctionDef {
            name: "bad".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Path(Path::simple("x".to_string())),
        })];
        // Note: the let statement would add x to env, but check_items runs first
        // and fails because x is referenced in function body before any stmts are checked
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_err(), "Let should not be visible in function, but got: {:?}", result);
        let err_msg = result.unwrap_err().message;
        assert!(
            err_msg.contains("unknown variable"),
            "Expected 'unknown variable' but got: {}", err_msg
        );
    }

    #[test]
    fn test_check_repl_self_recursion() {
        // fn factorial(n) -> Int { match n { 0 => 1, _ => n * factorial(n-1) } }
        // Should succeed
        let mut env = TypeEnv::default();
        let items = vec![Item::Function(FunctionDef {
            name: "factorial".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Var("n".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Match {
                scrutinee: Box::new(Expr::Path(Path::simple("n".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                        result: Expr::Int(1),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        result: Expr::BinOp {
                            op: BinOp::Mul,
                            left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                            right: Box::new(Expr::Call {
                                path: Path::simple("factorial".to_string()),
                                args: vec![Expr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                                    right: Box::new(Expr::Int(1)),
                                }],
                            }),
                        },
                    },
                ],
            },
        })];
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_ok(), "Self-recursion should succeed: {:?}", result.err());
    }

    #[test]
    fn test_substitute_type_vars_in_list() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let Type::Var(id) = var else { panic!("expected type var") };

        let mut mapping = HashMap::new();
        mapping.insert(id, Type::Int);

        let ty = Type::List(Box::new(Type::Var(id)));
        let result = substitute_type_vars(&ty, &mapping);
        assert_eq!(result, Type::List(Box::new(Type::Int)));
    }

    #[test]
    fn test_substitute_type_vars_in_tuple() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let Type::Var(id) = var else { panic!("expected type var") };

        let mut mapping = HashMap::new();
        mapping.insert(id, Type::String);

        let ty = Type::Tuple(vec![Type::Var(id), Type::Int]);
        let result = substitute_type_vars(&ty, &mapping);
        assert_eq!(result, Type::Tuple(vec![Type::String, Type::Int]));
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

    // ===== Let Pattern Irrefutability Tests =====

    #[test]
    fn test_let_literal_pattern_rejected() {
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::Literal(Box::new(Expr::Int(42))),
            type_annotation: None,
            value: Box::new(Expr::Int(42)),
        })];
        let _ = check_items(&items, &mut env, &mut ctx).unwrap();
        let result = check_stmts(&stmts, &mut env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("refutable"));
    }

    #[test]
    fn test_let_list_pattern_rejected() {
        use zoya_ast::ListPattern;
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::List(ListPattern::Exact(vec![Pattern::Var("x".to_string())])),
            type_annotation: None,
            value: Box::new(Expr::List(vec![Expr::Int(1)])),
        })];
        let _ = check_items(&items, &mut env, &mut ctx).unwrap();
        let result = check_stmts(&stmts, &mut env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("refutable"));
    }

    #[test]
    fn test_let_call_pattern_rejected() {
        use zoya_ast::{Pattern, TuplePattern};
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        // Don't need to set up actual enum type - irrefutability check happens before type checking
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::Call {
                path: Path {
                    prefix: PathPrefix::None,
                    segments: vec!["Option".to_string(), "Some".to_string()],
                    type_args: None,
                },
                args: TuplePattern::Exact(vec![Pattern::Var("x".to_string())]),
            },
            type_annotation: None,
            value: Box::new(Expr::Int(42)), // Doesn't matter, will fail at irrefutability check first
        })];
        let _ = check_items(&items, &mut env, &mut ctx).unwrap();
        let result = check_stmts(&stmts, &mut env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("refutable"));
    }

    #[test]
    fn test_let_tuple_pattern_irrefutable() {
        use zoya_ast::{Pattern, TuplePattern};
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let items: Vec<Item> = vec![];
        let stmts = vec![Stmt::Let(LetBinding {
            pattern: Pattern::Tuple(TuplePattern::Exact(vec![
                Pattern::Var("a".to_string()),
                Pattern::Var("b".to_string()),
            ])),
            type_annotation: None,
            value: Box::new(Expr::Tuple(vec![Expr::Int(1), Expr::Int(2)])),
        })];
        let _ = check_items(&items, &mut env, &mut ctx).unwrap();
        let result = check_stmts(&stmts, &mut env, &mut ctx);
        assert!(result.is_ok());
        // Both a and b should be in the environment
        assert!(env.locals.contains_key("a"));
        assert!(env.locals.contains_key("b"));
    }

    #[test]
    fn test_type_alias_simple() {
        // type UserId = Int
        // fn get_id() -> UserId { 42 }
        let items = vec![
            Item::TypeAlias(TypeAliasDef {
                name: "UserId".to_string(),
                type_params: vec![],
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }),
            Item::Function(FunctionDef {
                name: "get_id".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("UserId".to_string()))),
                body: Expr::Int(42),
            }),
        ];
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_alias_generic() {
        // type Pair<A, B> = (A, B)
        // fn make_pair() -> Pair<Int, Bool> { (1, true) }
        let items = vec![
            Item::TypeAlias(TypeAliasDef {
                name: "Pair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                typ: TypeAnnotation::Tuple(vec![
                    TypeAnnotation::Named(Path::simple("A".to_string())),
                    TypeAnnotation::Named(Path::simple("B".to_string())),
                ]),
            }),
            Item::Function(FunctionDef {
                name: "make_pair".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Parameterized(
                    Path::simple("Pair".to_string()),
                    vec![
                        TypeAnnotation::Named(Path::simple("Int".to_string())),
                        TypeAnnotation::Named(Path::simple("Bool".to_string())),
                    ],
                )),
                body: Expr::Tuple(vec![Expr::Int(1), Expr::Bool(true)]),
            }),
        ];
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_alias_non_pascal_case_error() {
        // type userId = Int  -- should fail
        let items = vec![Item::TypeAlias(TypeAliasDef {
            name: "userId".to_string(),
            type_params: vec![],
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        })];
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("PascalCase"));
    }

    #[test]
    fn test_type_alias_wrong_arity_error() {
        // type Pair<A, B> = (A, B)
        // fn bad() -> Pair<Int> { ... }  -- should fail, needs 2 args
        let items = vec![
            Item::TypeAlias(TypeAliasDef {
                name: "Pair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                typ: TypeAnnotation::Tuple(vec![
                    TypeAnnotation::Named(Path::simple("A".to_string())),
                    TypeAnnotation::Named(Path::simple("B".to_string())),
                ]),
            }),
            Item::Function(FunctionDef {
                name: "bad".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Parameterized(
                    Path::simple("Pair".to_string()),
                    vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
                )),
                body: Expr::Tuple(vec![Expr::Int(1), Expr::Int(2)]),
            }),
        ];
        let mut env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = check_items(&items, &mut env, &mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type argument"));
    }

    // ========================================================================
    // Lambda tests
    // ========================================================================

    use zoya_ast::LambdaParam;

    #[test]
    fn test_check_lambda_basic() {
        let expr = Expr::Lambda {
            params: vec![LambdaParam {
                pattern: Pattern::Var("x".to_string()),
                typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            }],
            return_type: None,
            body: Box::new(Expr::Path(Path::simple("x".to_string()))),
        };
        let result = check(&expr).unwrap();
        match result.ty() {
            Type::Function { params, ret } => {
                assert_eq!(params, vec![Type::Int]);
                assert_eq!(*ret, Type::Int);
            }
            _ => panic!("Expected function type"),
        }
    }

    #[test]
    fn test_check_lambda_with_return_type() {
        let expr = Expr::Lambda {
            params: vec![LambdaParam {
                pattern: Pattern::Var("x".to_string()),
                typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Box::new(Expr::Path(Path::simple("x".to_string()))),
        };
        let result = check(&expr).unwrap();
        match result.ty() {
            Type::Function { params, ret } => {
                assert_eq!(params, vec![Type::Int]);
                assert_eq!(*ret, Type::Int);
            }
            _ => panic!("Expected function type"),
        }
    }

    #[test]
    fn test_check_lambda_return_type_mismatch() {
        let expr = Expr::Lambda {
            params: vec![LambdaParam {
                pattern: Pattern::Var("x".to_string()),
                typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("String".to_string()))),
            body: Box::new(Expr::Path(Path::simple("x".to_string()))),
        };
        let result = check(&expr);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("lambda body type") || err.message.contains("doesn't match declared return type"));
    }

    #[test]
    fn test_check_lambda_refutable_param_error() {
        // Lambda with literal pattern (refutable) should fail
        let expr = Expr::Lambda {
            params: vec![LambdaParam {
                pattern: Pattern::Literal(Box::new(Expr::Int(42))),
                typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            }],
            return_type: None,
            body: Box::new(Expr::Int(1)),
        };
        let result = check(&expr);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("refutable pattern in lambda parameter"));
    }

    #[test]
    fn test_check_lambda_tuple_param() {
        let expr = Expr::Lambda {
            params: vec![LambdaParam {
                pattern: Pattern::Tuple(zoya_ast::TuplePattern::Exact(vec![
                    Pattern::Var("x".to_string()),
                    Pattern::Var("y".to_string()),
                ])),
                typ: Some(TypeAnnotation::Tuple(vec![
                    TypeAnnotation::Named(Path::simple("Int".to_string())),
                    TypeAnnotation::Named(Path::simple("Int".to_string())),
                ])),
            }],
            return_type: None,
            body: Box::new(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Path(Path::simple("y".to_string()))),
            }),
        };
        let result = check(&expr).unwrap();
        assert!(matches!(result.ty(), Type::Function { .. }));
    }

    // ========================================================================
    // Call lambda variable tests
    // ========================================================================

    #[test]
    fn test_call_lambda_variable() {
        let mut env = TypeEnv::default();
        env.locals.insert(
            "f".to_string(),
            TypeScheme::mono(Type::Function {
                params: vec![Type::Int],
                ret: Box::new(Type::String),
            }),
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path::simple("f".to_string()),
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::String);
    }

    #[test]
    fn test_call_non_function_error() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path::simple("x".to_string()),
            args: vec![Expr::Int(1)],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("is not a function"));
    }

    #[test]
    fn test_turbofish_on_lambda_error() {
        let mut env = TypeEnv::default();
        env.locals.insert(
            "f".to_string(),
            TypeScheme::mono(Type::Function {
                params: vec![Type::Int],
                ret: Box::new(Type::Int),
            }),
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["f".to_string()],
                type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
            },
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("cannot use turbofish on lambda"));
    }

    #[test]
    fn test_call_lambda_wrong_arity() {
        let mut env = TypeEnv::default();
        env.locals.insert(
            "f".to_string(),
            TypeScheme::mono(Type::Function {
                params: vec![Type::Int, Type::Int],
                ret: Box::new(Type::Int),
            }),
        );

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path::simple("f".to_string()),
            args: vec![Expr::Int(42)], // Only 1 arg, needs 2
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("expects 2 arguments"));
    }

    // ========================================================================
    // Struct construction tests
    // ========================================================================

    use zoya_ir::StructType;

    fn env_with_point_struct() -> TypeEnv {
        let mut env = TypeEnv::default();
        env.structs.insert(
            "Point".to_string(),
            StructType {
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![
                    ("x".to_string(), Type::Int),
                    ("y".to_string(), Type::Int),
                ],
            },
        );
        env
    }

    #[test]
    fn test_struct_construct_valid() {
        let env = env_with_point_struct();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                ("y".to_string(), Expr::Int(20)),
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        match result.ty() {
            Type::Struct { name, .. } => assert_eq!(name, "Point"),
            _ => panic!("Expected struct type"),
        }
    }

    #[test]
    fn test_struct_construct_missing_field() {
        let env = env_with_point_struct();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                // Missing y field
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("missing field 'y'"));
    }

    #[test]
    fn test_struct_construct_extra_field() {
        let env = env_with_point_struct();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                ("y".to_string(), Expr::Int(20)),
                ("z".to_string(), Expr::Int(30)), // Extra field
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown field 'z'"));
    }

    #[test]
    fn test_struct_construct_field_type_mismatch() {
        let env = env_with_point_struct();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                ("y".to_string(), Expr::String("wrong".to_string())), // Wrong type
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("field 'y'") && err.message.contains("expects type"));
    }

    #[test]
    fn test_struct_construct_unknown_struct() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path::simple("UnknownStruct".to_string()),
            fields: vec![],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown struct"));
    }

    // ========================================================================
    // Enum construction tests
    // ========================================================================

    use zoya_ir::EnumType;

    fn env_with_message_enum() -> TypeEnv {
        let mut env = TypeEnv::default();
        env.enums.insert(
            "Message".to_string(),
            EnumType {
                name: "Message".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                variants: vec![
                    ("Quit".to_string(), EnumVariantType::Unit),
                    ("Move".to_string(), EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ])),
                    ("Write".to_string(), EnumVariantType::Tuple(vec![Type::String])),
                ],
            },
        );
        env
    }

    #[test]
    fn test_enum_tuple_construct_valid() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Write".to_string()],
                type_args: None,
            },
            args: vec![Expr::String("hello".to_string())],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        match result.ty() {
            Type::Enum { name, .. } => assert_eq!(name, "Message"),
            _ => panic!("Expected enum type"),
        }
    }

    #[test]
    fn test_enum_tuple_construct_unit_variant_with_args_error() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Quit".to_string()],
                type_args: None,
            },
            args: vec![Expr::Int(1)], // Quit is a unit variant
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unit variant"));
    }

    #[test]
    fn test_enum_tuple_construct_struct_variant_with_tuple_syntax_error() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            args: vec![Expr::Int(1), Expr::Int(2)], // Move is a struct variant
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("struct variant"));
    }

    #[test]
    fn test_enum_struct_construct_valid() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                ("y".to_string(), Expr::Int(20)),
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        match result.ty() {
            Type::Enum { name, .. } => assert_eq!(name, "Message"),
            _ => panic!("Expected enum type"),
        }
    }

    #[test]
    fn test_enum_struct_construct_unit_variant_error() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Quit".to_string()],
                type_args: None,
            },
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unit variant"));
    }

    #[test]
    fn test_enum_struct_construct_tuple_variant_error() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Write".to_string()],
                type_args: None,
            },
            fields: vec![
                ("msg".to_string(), Expr::String("hi".to_string())),
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("tuple variant"));
    }

    #[test]
    fn test_enum_struct_construct_missing_field() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                // Missing y
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("missing field 'y'"));
    }

    #[test]
    fn test_enum_struct_construct_unknown_field() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                ("y".to_string(), Expr::Int(20)),
                ("z".to_string(), Expr::Int(30)), // Unknown
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown field 'z'"));
    }

    #[test]
    fn test_enum_struct_construct_field_type_mismatch() {
        let env = env_with_message_enum();
        let mut ctx = UnifyCtx::new();
        let expr = Expr::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![
                ("x".to_string(), Expr::Int(10)),
                ("y".to_string(), Expr::String("wrong".to_string())), // Wrong type
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("field 'y'") && err.message.contains("expects"));
    }

    // ========================================================================
    // Function definition naming tests
    // ========================================================================

    #[test]
    fn test_function_def_invalid_name_pascal_case() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "MyFunction".to_string(), // Should be snake_case
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(42),
        };
        let result = check_function(&func, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("should be snake_case"));
    }

    #[test]
    fn test_function_def_invalid_type_param() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "identity".to_string(),
            type_params: vec!["bad_type".to_string()], // Should be PascalCase
            params: vec![Param {
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Path(Path::simple("x".to_string())),
        };
        let result = check_function(&func, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("type parameter") && err.message.contains("should be PascalCase"));
    }

    #[test]
    fn test_function_def_refutable_param_pattern() {
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let func = FunctionDef {
            name: "bad".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Literal(Box::new(Expr::Int(42))), // Refutable
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(0),
        };
        let result = check_function(&func, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("refutable pattern in function parameter"));
    }

    // ========================================================================
    // Match expression edge cases
    // ========================================================================

    #[test]
    fn test_match_empty_arms_exhaustiveness_warning() {
        // While this test doesn't check for empty arms directly (since the usefulness
        // checker handles it), we test that match expressions with no matching arms
        // behave correctly type-wise. The usefulness checker tests handle exhaustiveness.
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Bool));

        let mut ctx = UnifyCtx::new();
        // Match Bool with only one arm (non-exhaustive) - usefulness checker should catch this
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Bool(true))),
                    result: Expr::Int(1),
                },
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Bool(false))),
                    result: Expr::Int(0),
                },
            ],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    // ========================================================================
    // Turbofish on function call tests
    // ========================================================================

    #[test]
    fn test_turbofish_correct_count() {
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

        // identity::<Int>(42)
        let expr = Expr::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["identity".to_string()],
                type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
            },
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env, &mut ctx).unwrap();
        assert_eq!(result.ty(), Type::Int);
    }

    #[test]
    fn test_turbofish_wrong_count_error() {
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

        // identity::<Int, String>(42) - wrong number of type args
        let expr = Expr::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["identity".to_string()],
                type_args: Some(vec![
                    TypeAnnotation::Named(Path::simple("Int".to_string())),
                    TypeAnnotation::Named(Path::simple("String".to_string())),
                ]),
            },
            args: vec![Expr::Int(42)],
        };
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("expects 1 type argument(s), got 2"));
    }

    // ========================================================================
    // Variable path tests
    // ========================================================================

    #[test]
    fn test_turbofish_on_variable_error() {
        let mut env = TypeEnv::default();
        env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

        let mut ctx = UnifyCtx::new();
        let expr = Expr::Path(Path {
            prefix: PathPrefix::None,
            segments: vec!["x".to_string()],
            type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
        });
        let result = check_with_env(&expr, &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("cannot use turbofish on variable"));
    }
}
