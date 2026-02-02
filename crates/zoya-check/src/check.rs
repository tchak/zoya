use std::collections::{HashMap, HashSet};

use zoya_ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, MatchArm, Path, TypeAnnotation, UnaryOp,
};
use zoya_ir::{
    CheckedItem, CheckedModule, CheckedModuleTree, Definition, EnumType,
    EnumVariantType, FunctionType, QualifiedPath, StructType, Type, TypeAliasType, TypeError,
    TypeScheme, TypeVarId, TypedEnumConstructFields, TypedExpr, TypedFunction,
};
use zoya_module::{ModulePath, ModuleTree};

use crate::builtin::{builtin_method, is_numeric_type};
use crate::definition::{enum_type_from_def, function_type_from_def, struct_type_from_def, type_alias_from_def};
use crate::naming::{is_pascal_case, is_snake_case, to_pascal_case, to_snake_case};
use crate::pattern::{check_irrefutable, check_let_binding, check_match_arm, check_pattern};
use crate::resolution;
use crate::type_resolver::resolve_type_annotation;
use crate::unify::UnifyCtx;
use crate::usefulness;

/// Type environment for checking expressions
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// All named definitions (functions, structs, enums, type aliases) in a unified namespace
    pub definitions: HashMap<String, Definition>,
    /// Local variable types (type schemes for let polymorphism)
    pub locals: HashMap<String, TypeScheme>,
}

impl TypeEnv {
    pub fn with_locals(&self, locals: HashMap<String, TypeScheme>) -> Self {
        TypeEnv {
            definitions: self.definitions.clone(),
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

    /// Generic lookup for any definition by name
    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.definitions.get(name)
    }

    /// Register a definition in the environment.
    /// Currently allows overwrites (needed for REPL redefinition).
    /// Future: add collision detection for different definition kinds.
    pub fn register(&mut self, name: String, def: Definition) {
        self.definitions.insert(name, def);
    }

    /// Look up a function by name
    pub fn get_function(&self, name: &str) -> Option<&FunctionType> {
        self.get(name).and_then(Definition::as_function)
    }

    /// Look up a struct by name
    pub fn get_struct(&self, name: &str) -> Option<&StructType> {
        self.get(name).and_then(Definition::as_struct)
    }

    /// Look up an enum by name
    pub fn get_enum(&self, name: &str) -> Option<&EnumType> {
        self.get(name).and_then(Definition::as_enum)
    }

    /// Look up a type alias by name
    pub fn get_type_alias(&self, name: &str) -> Option<&TypeAliasType> {
        self.get(name).and_then(Definition::as_type_alias)
    }
}

/// Check a function definition and return a typed function
fn check_function(
    func: &FunctionDef,
    current_module: &ModulePath,
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

        let ty = resolve_type_annotation(&param.typ, &type_param_map, current_module, env)?;

        // Type-check the pattern against the parameter type
        let (typed_pattern, bindings) = check_pattern(&param.pattern, &ty, current_module, env, ctx)?;

        // Add all pattern bindings to locals
        for (name, var_ty) in bindings {
            locals.insert(name, TypeScheme::mono(var_ty));
        }

        typed_params.push((typed_pattern, ctx.resolve(&ty)));
    }

    // Create environment with locals for checking body
    let body_env = env.with_locals(locals);

    // Check the body
    let typed_body = check_expr(&func.body, current_module, &body_env, ctx)?;
    let body_type = ctx.resolve(&typed_body.ty());

    // Determine return type
    let return_type = if let Some(ref annotation) = func.return_type {
        let declared_return = resolve_type_annotation(annotation, &type_param_map, current_module, env)?;
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
fn check_path_expr(path: &Path, current_module: &ModulePath, env: &TypeEnv, ctx: &mut UnifyCtx) -> Result<TypedExpr, TypeError> {
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
            let qualified = resolution::qualified_name(current_module, enum_name);
            let enum_type = env.get_enum(&qualified).ok_or_else(|| TypeError {
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
                    .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    match path.segments.as_slice() {
        // Single segment: function or lambda call
        [func] => check_simple_call(func, &path.type_args, args, current_module, env, ctx),
        // Two segments: Enum::Variant(args)
        [enum_name, variant_name] => {
            check_enum_tuple_construct(enum_name, variant_name, &path.type_args, args, current_module, env, ctx)
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // First, try to look up as a named function
    let qualified = resolution::qualified_name(current_module, func);
    if let Some(func_type) = env.get_function(&qualified) {
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
                .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
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
            let typed_arg = check_expr(arg, current_module, env, ctx)?;
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
                let typed_arg = check_expr(arg, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let qualified = resolution::qualified_name(current_module, enum_name);
    let enum_type = env.get_enum(&qualified).ok_or_else(|| TypeError {
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
            .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
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
        let typed_expr = check_expr(expr, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_expr(expr, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_left = check_expr(left, current_module, env, ctx)?;
    let typed_right = check_expr(right, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // Create new environment for block scope
    let mut block_env = env.clone();
    let mut typed_bindings = Vec::new();

    for binding in bindings {
        let (typed_binding, pattern_bindings) = check_let_binding(binding, current_module, &block_env, ctx)?;

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
    let typed_result = check_expr(result, current_module, &block_env, ctx)?;

    Ok(TypedExpr::Block {
        bindings: typed_bindings,
        result: Box::new(typed_result),
    })
}

/// Check a match expression
fn check_match_expr(
    scrutinee: &Expr,
    arms: &[MatchArm],
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_scrutinee = check_expr(scrutinee, current_module, env, ctx)?;
    let scrutinee_ty = typed_scrutinee.ty();

    if arms.is_empty() {
        return Err(TypeError {
            message: "match expression must have at least one arm".to_string(),
        });
    }

    let mut typed_arms = Vec::new();
    let mut result_ty: Option<Type> = None;

    for arm in arms {
        let typed_arm = check_match_arm(arm, &scrutinee_ty, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_receiver = check_expr(receiver, current_module, env, ctx)?;
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
        let typed_arg = check_expr(arg, current_module, env, ctx)?;
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
    current_module: &ModulePath,
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
        let first_typed = check_expr(&elements[0], current_module, env, ctx)?;
        let elem_ty = first_typed.ty();
        let mut typed_elements = vec![first_typed];

        // Check remaining elements unify with first element's type
        for elem in &elements[1..] {
            let typed = check_expr(elem, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let mut typed_elements = Vec::new();
    let mut element_types = Vec::new();

    for elem in elements {
        let typed = check_expr(elem, current_module, env, ctx)?;
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
    current_module: &ModulePath,
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
            Some(annotation) => resolve_type_annotation(annotation, &HashMap::new(), current_module, env)?,
            None => ctx.fresh_var(),
        };

        // Type-check the pattern against the parameter type
        let (typed_pattern, bindings) = check_pattern(&param.pattern, &param_ty, current_module, env, ctx)?;

        // Add all pattern bindings to the lambda environment
        for (name, var_ty) in bindings {
            lambda_env.locals.insert(name, TypeScheme::mono(var_ty));
        }

        typed_params.push((typed_pattern, ctx.resolve(&param_ty)));
        param_types.push(param_ty);
    }

    // Check the body in the extended environment
    let typed_body = check_expr(body, current_module, &lambda_env, ctx)?;
    let body_ty = typed_body.ty();

    // If return type is annotated, unify with body type
    let resolved_return = if let Some(annotation) = return_type {
        let declared_return = resolve_type_annotation(annotation, &HashMap::new(), current_module, env)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    match path.segments.as_slice() {
        // Single segment: struct construction
        [name] => check_struct_construct(name, fields, current_module, env, ctx),
        // Two segments: Enum::Variant { fields }
        [enum_name, variant_name] => {
            check_enum_struct_construct(enum_name, variant_name, fields, current_module, env, ctx)
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // Look up the struct definition
    let qualified = resolution::qualified_name(current_module, name);
    let struct_type = env.get_struct(&qualified).ok_or_else(|| TypeError {
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
        let typed_expr = check_expr(field_expr, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let qualified = resolution::qualified_name(current_module, enum_name);
    let enum_type = env.get_enum(&qualified).ok_or_else(|| TypeError {
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
        let typed_expr = check_expr(field_expr, current_module, env, ctx)?;
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_expr(expr, current_module, env, ctx)?;
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
pub(crate) fn check_expr(
    expr: &Expr,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    match expr {
        Expr::Int(n) => Ok(TypedExpr::Int(*n)),
        Expr::BigInt(n) => Ok(TypedExpr::BigInt(*n)),
        Expr::Float(n) => Ok(TypedExpr::Float(*n)),
        Expr::Bool(b) => Ok(TypedExpr::Bool(*b)),
        Expr::String(s) => Ok(TypedExpr::String(s.clone())),
        Expr::Path(path) => check_path_expr(path, current_module, env, ctx),
        Expr::Call { path, args } => check_path_call(path, args, current_module, env, ctx),
        Expr::UnaryOp { op, expr } => check_unary_op(*op, expr, current_module, env, ctx),
        Expr::BinOp { op, left, right } => check_bin_op(*op, left, right, current_module, env, ctx),
        Expr::Block { bindings, result } => check_block(bindings, result, current_module, env, ctx),
        Expr::Match { scrutinee, arms } => check_match_expr(scrutinee, arms, current_module, env, ctx),
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => check_method_call(receiver, method, args, current_module, env, ctx),
        Expr::List(elements) => check_list_expr(elements, current_module, env, ctx),
        Expr::Tuple(elements) => check_tuple_expr(elements, current_module, env, ctx),
        Expr::Lambda {
            params,
            return_type,
            body,
        } => check_lambda(params, return_type, body, current_module, env, ctx),
        Expr::Struct { path, fields } => check_path_struct(path, fields, current_module, env, ctx),
        Expr::FieldAccess { expr, field } => check_field_access(expr, field, current_module, env, ctx),
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
pub fn check(tree: &ModuleTree) -> Result<CheckedModuleTree, TypeError> {
    let mut env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();

    // Phase 1: Register ALL declarations from ALL modules
    // Process modules in dependency order (parents before children)
    let mut module_paths: Vec<_> = tree.modules.keys().cloned().collect();
    module_paths.sort_by_key(|p| p.depth());

    for path in &module_paths {
        if let Some(module) = tree.modules.get(path) {
            register_module_declarations(&module.items, path, &mut env, &mut ctx)?;
        }
    }

    // Phase 2: Type-check ALL function bodies
    let mut checked_modules = HashMap::new();
    for path in &module_paths {
        if let Some(module) = tree.modules.get(path) {
            let checked = check_module_bodies(&module.items, path, &env, &mut ctx)?;
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
            env.register(
                qualified_name,
                Definition::Struct(StructType {
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    fields: vec![],
                }),
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
            env.register(
                qualified_name,
                Definition::Enum(EnumType {
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    variants: vec![],
                }),
            );
        }
    }

    // Phase 1b: Resolve all struct field types
    for item in items {
        if let Item::Struct(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let struct_type = struct_type_from_def(def, current_module, env, ctx)?;
            env.register(qualified_name, Definition::Struct(struct_type));
        }
    }

    // Phase 1c: Resolve all enum variant types
    for item in items {
        if let Item::Enum(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let enum_type = enum_type_from_def(def, current_module, env, ctx)?;
            env.register(qualified_name, Definition::Enum(enum_type));
        }
    }

    // Phase 1d: Register all type aliases
    for item in items {
        if let Item::TypeAlias(def) = item {
            let qualified_name = resolution::qualified_name(current_module, &def.name);
            let alias_type = type_alias_from_def(def, current_module, env, ctx)?;
            env.register(qualified_name, Definition::TypeAlias(alias_type));
        }
    }

    // Phase 2: Register all function signatures
    for item in items {
        if let Item::Function(func) = item {
            let qualified_name = resolution::qualified_name(current_module, &func.name);
            let func_type = function_type_from_def(func, current_module, env, ctx)?;
            env.register(qualified_name, Definition::Function(func_type));
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
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedFunction, TypeError> {
    check_function(func, current_module, env, ctx)
}

#[cfg(test)]
mod tests;
