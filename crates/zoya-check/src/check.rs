use std::collections::{HashMap, HashSet};

use zoya_ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, MatchArm, Path, TypeAnnotation, UnaryOp, UseDecl,
    UseTarget,
};
use zoya_ir::{
    CheckedPackage, Definition, EnumType, EnumVariantType,
    FunctionType, ModuleType, QualifiedPath, StructType, Type, TypeAliasType, TypeError,
    TypeScheme, TypeVarId, TypedEnumConstructFields, TypedExpr, TypedFunction, Visibility,
};
use zoya_package::Package;

use crate::builtin::{builtin_method, is_numeric_type};
use crate::definition::{
    enum_type_from_def, function_type_from_def, struct_type_from_def, type_alias_from_def,
};
use crate::imports::{resolve_module_imports, resolve_use_module_path, resolve_use_path, ImportTable};
use zoya_naming::{is_pascal_case, is_snake_case, to_pascal_case, to_snake_case};
use crate::pattern::{check_irrefutable, check_let_binding, check_match_arm, check_pattern};
use crate::resolution::{self, ResolvedPath};
use crate::type_resolver::resolve_type_annotation;
use crate::unify::UnifyCtx;
use crate::usefulness;

/// Type environment for checking expressions
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// All named definitions (functions, structs, enums, type aliases, modules) in a unified namespace
    pub definitions: HashMap<QualifiedPath, Definition>,
    /// Local variable types (type schemes for let polymorphism)
    pub locals: HashMap<String, TypeScheme>,
    /// Per-module import tables: module_path -> (local_name -> qualified_path)
    /// Includes both item imports and module imports.
    pub imports: HashMap<QualifiedPath, ImportTable>,
    /// Re-export path mappings: re-export_path -> original_path
    /// Used to resolve re-exports to their original definition locations for codegen.
    /// Includes both item re-exports and module re-exports.
    pub reexports: HashMap<QualifiedPath, QualifiedPath>,
}

impl TypeEnv {
    pub fn with_locals(&self, locals: HashMap<String, TypeScheme>) -> Self {
        TypeEnv {
            definitions: self.definitions.clone(),
            locals,
            imports: self.imports.clone(),
            reexports: self.reexports.clone(),
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

    /// Register a definition in the environment.
    /// Currently allows overwrites (needed for REPL redefinition).
    /// Future: add collision detection for different definition kinds.
    pub fn register(&mut self, path: QualifiedPath, def: Definition) {
        self.definitions.insert(path, def);
    }
}

/// Check a function definition and return a typed function
fn check_function(
    func: &FunctionDef,
    current_module: &QualifiedPath,
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
        let (typed_pattern, bindings) =
            check_pattern(&param.pattern, &ty, current_module, env, ctx)?;

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
        let declared_return =
            resolve_type_annotation(annotation, &type_param_map, current_module, env)?;
        // Unify body type with declared return type
        ctx.unify(&body_type, &declared_return)
            .map_err(|e| TypeError {
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

/// Check a path expression (variable or unit enum variant)
fn check_path_expr(
    path: &Path,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let resolved =
        resolution::resolve_expr_path(path, current_module, &env.locals, &env.imports, &env.definitions, &env.reexports)?;

    match resolved {
        ResolvedPath::Local { name, scheme } => {
            // Variables cannot have turbofish
            if path.type_args.is_some() {
                return Err(TypeError {
                    message: format!("cannot use turbofish on variable '{}'", name),
                });
            }
            let ty = ctx.instantiate(scheme);
            Ok(TypedExpr::Var {
                path: QualifiedPath::local(name),
                ty: ctx.resolve(&ty),
            })
        }
        ResolvedPath::Definition {
            def: Definition::EnumVariant(enum_type, variant_type),
            qualified_path,
        } => {
            // Must be a unit variant when used as a bare path
            if !matches!(variant_type, EnumVariantType::Unit) {
                return Err(TypeError {
                    message: format!("enum variant {} requires arguments", qualified_path),
                });
            }

            let enum_path = current_module.child(&enum_type.name);
            let variant_name = qualified_path.last();
            let qualified_variant_path = enum_path.child(variant_name);

            // Handle explicit type arguments (turbofish) or create fresh type variables
            let instantiation: HashMap<TypeVarId, Type> = if let Some(ref type_args) =
                path.type_args
            {
                // Validate count matches type parameters
                if type_args.len() != enum_type.type_params.len() {
                    return Err(TypeError {
                        message: format!(
                            "enum {} expects {} type argument(s), got {}",
                            enum_path,
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
                path: qualified_variant_path,
                fields: TypedEnumConstructFields::Unit,
                ty: Type::Enum {
                    name: enum_type.name.clone(),
                    type_args,
                    variants: resolved_variants,
                },
            })
        }
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => {
            // Functions, structs, enums, type aliases can't be used as values directly
            Err(TypeError {
                message: format!(
                    "{} '{}' cannot be used as a value",
                    def.kind_name(),
                    qualified_path
                ),
            })
        }
    }
}

/// Check a path call expression (function call or tuple enum variant)
fn check_path_call(
    path: &Path,
    args: &[Expr],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let resolved =
        resolution::resolve_expr_path(path, current_module, &env.locals, &env.imports, &env.definitions, &env.reexports)?;

    match resolved {
        ResolvedPath::Local { name, scheme } => {
            // Lambda call - cannot have turbofish
            if path.type_args.is_some() {
                return Err(TypeError {
                    message: format!("cannot use turbofish on lambda call '{}'", name),
                });
            }
            check_lambda_call(&name, scheme, args, current_module, env, ctx)
        }
        ResolvedPath::Definition {
            def: Definition::Function(func_type),
            qualified_path,
        } => check_function_call(path, &qualified_path, func_type, args, current_module, env, ctx),
        ResolvedPath::Definition {
            def: Definition::EnumVariant(enum_type, variant_type),
            qualified_path,
        } => match variant_type {
            EnumVariantType::Tuple(_) => check_enum_tuple_construct_resolved(
                enum_type,
                variant_type,
                qualified_path.last(),
                &path.type_args,
                args,
                current_module,
                env,
                ctx,
            ),
            EnumVariantType::Unit => Err(TypeError {
                message: format!(
                    "enum variant {} is a unit variant, doesn't take arguments",
                    qualified_path
                ),
            }),
            EnumVariantType::Struct(_) => Err(TypeError {
                message: format!(
                    "enum variant {} is a struct variant, use {{ }} syntax",
                    qualified_path
                ),
            }),
        },
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => {
            let kind = def.kind_name();
            Err(TypeError {
                message: format!("{} '{}' cannot be called", kind, qualified_path),
            })
        }
    }
}

/// Check a function call with a resolved function type
fn check_function_call(
    path: &Path,
    qualified_path: &QualifiedPath,
    func_type: &FunctionType,
    args: &[Expr],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let func_name = path
        .segments
        .last()
        .map(|s| s.as_str())
        .unwrap_or("<unknown>");

    // Check argument count
    if args.len() != func_type.params.len() {
        return Err(TypeError {
            message: format!(
                "function '{}' expects {} arguments, got {}",
                func_name,
                func_type.params.len(),
                args.len()
            ),
        });
    }

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(ref type_args) = path.type_args {
        // Validate count matches type parameters
        if type_args.len() != func_type.type_params.len() {
            return Err(TypeError {
                message: format!(
                    "function '{}' expects {} type argument(s), got {}",
                    func_name,
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
                func_name,
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
        path: qualified_path.clone(),
        args: typed_args,
        ty: return_type,
    })
}

/// Check a lambda call (calling a variable bound to a function type)
fn check_lambda_call(
    name: &str,
    scheme: &TypeScheme,
    args: &[Expr],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let func_ty = ctx.instantiate(scheme);
    let resolved = ctx.resolve(&func_ty);

    // Get function params and return type, unifying if needed
    let (params, ret) = match resolved {
        Type::Function { params, ret } => (params, *ret),
        Type::Var(_) => {
            // Create fresh type variables for params and return
            let param_types: Vec<Type> = args.iter().map(|_| ctx.fresh_var()).collect();
            let ret_type = ctx.fresh_var();

            // Unify the variable with a function type
            let func_type = Type::Function {
                params: param_types.clone(),
                ret: Box::new(ret_type.clone()),
            };
            ctx.unify(&func_ty, &func_type).map_err(|e| TypeError {
                message: format!("cannot call '{}' as a function: {}", name, e.message),
            })?;

            (param_types, ret_type)
        }
        _ => {
            return Err(TypeError {
                message: format!("'{}' is not a function, has type {}", name, resolved),
            });
        }
    };

    // Check argument count
    if args.len() != params.len() {
        return Err(TypeError {
            message: format!(
                "'{}' expects {} arguments, got {}",
                name,
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
                name,
                ctx.resolve(param_type),
                ctx.resolve(&arg_type),
                e.message
            ),
        })?;

        typed_args.push(typed_arg);
    }

    let return_type = ctx.resolve(&ret);

    Ok(TypedExpr::Call {
        path: QualifiedPath::local(name.to_string()),
        args: typed_args,
        ty: return_type,
    })
}

/// Check an enum tuple variant construction with a resolved enum type: Enum::Variant(args)
#[allow(clippy::too_many_arguments)]
fn check_enum_tuple_construct_resolved(
    enum_type: &EnumType,
    variant_type: &EnumVariantType,
    variant_name: &str,
    explicit_type_args: &Option<Vec<TypeAnnotation>>,
    args: &[Expr],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let enum_path = current_module.child(&enum_type.name);
    let qualified_variant_path = enum_path.child(variant_name);

    // Must be a tuple variant
    let expected_types = match variant_type {
        EnumVariantType::Tuple(types) => types,
        EnumVariantType::Unit => {
            return Err(TypeError {
                message: format!(
                    "enum variant {} is a unit variant, doesn't take arguments",
                    qualified_variant_path
                ),
            });
        }
        EnumVariantType::Struct(_) => {
            return Err(TypeError {
                message: format!(
                    "enum variant {} is a struct variant, use {{ }} syntax",
                    qualified_variant_path
                ),
            });
        }
    };

    if args.len() != expected_types.len() {
        return Err(TypeError {
            message: format!(
                "enum variant {} expects {} argument(s), got {}",
                qualified_variant_path,
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
                    enum_path,
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

        ctx.unify(&actual_type, &expected_type)
            .map_err(|e| TypeError {
                message: format!(
                    "in enum variant {} expected {} but got {}: {}",
                    qualified_variant_path,
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
        path: qualified_variant_path,
        fields: TypedEnumConstructFields::Tuple(typed_exprs),
        ty: Type::Enum {
            name: enum_type.name.to_string(),
            type_args,
            variants: resolved_variants,
        },
    })
}

/// Check a unary operation
fn check_unary_op(
    op: UnaryOp,
    expr: &Expr,
    current_module: &QualifiedPath,
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
    current_module: &QualifiedPath,
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
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    // Create new environment for block scope
    let mut block_env = env.clone();
    let mut typed_bindings = Vec::new();

    for binding in bindings {
        let (typed_binding, pattern_bindings) =
            check_let_binding(binding, current_module, &block_env, ctx)?;

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
    current_module: &QualifiedPath,
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
    current_module: &QualifiedPath,
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
    current_module: &QualifiedPath,
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
    current_module: &QualifiedPath,
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
    current_module: &QualifiedPath,
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
            Some(annotation) => {
                resolve_type_annotation(annotation, &HashMap::new(), current_module, env)?
            }
            None => ctx.fresh_var(),
        };

        // Type-check the pattern against the parameter type
        let (typed_pattern, bindings) =
            check_pattern(&param.pattern, &param_ty, current_module, env, ctx)?;

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
        let declared_return =
            resolve_type_annotation(annotation, &HashMap::new(), current_module, env)?;
        ctx.unify(&body_ty, &declared_return)
            .map_err(|e| TypeError {
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

/// Check a path struct expression (struct construction or enum struct variant)
fn check_path_struct(
    path: &Path,
    fields: &[(String, Expr)],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let resolved =
        resolution::resolve_expr_path(path, current_module, &env.locals, &env.imports, &env.definitions, &env.reexports)?;

    match resolved {
        ResolvedPath::Definition {
            def: Definition::Struct(struct_type),
            qualified_path,
        } => check_struct_construct_resolved(
            &qualified_path,
            struct_type,
            fields,
            current_module,
            env,
            ctx,
        ),
        ResolvedPath::Definition {
            def: Definition::EnumVariant(def, variant),
            qualified_path,
        } => match variant {
            EnumVariantType::Struct(_) => check_enum_struct_construct_resolved(
                def,
                variant,
                qualified_path.last(),
                fields,
                current_module,
                env,
                ctx,
            ),
            EnumVariantType::Unit => Err(TypeError {
                message: format!(
                    "enum variant {} is a unit variant, doesn't take fields",
                    qualified_path
                ),
            }),
            EnumVariantType::Tuple(_) => Err(TypeError {
                message: format!(
                    "enum variant {} is a tuple variant, use ( ) syntax",
                    qualified_path
                ),
            }),
        },
        ResolvedPath::Local { name, .. } => Err(TypeError {
            message: format!("'{}' is a variable, not a struct", name),
        }),
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => {
            let kind = def.kind_name();
            Err(TypeError {
                message: format!(
                    "{} '{}' cannot be constructed with struct syntax",
                    kind, qualified_path
                ),
            })
        }
    }
}

/// Check a struct construction expression with resolved struct type
fn check_struct_construct_resolved(
    qualified_path: &QualifiedPath,
    struct_type: &StructType,
    fields: &[(String, Expr)],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let name = &struct_type.name;
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
        .map(|(field_name, ty)| {
            (
                field_name.clone(),
                ctx.resolve(&substitute_type_vars(ty, &instantiation)),
            )
        })
        .collect();

    Ok(TypedExpr::StructConstruct {
        path: qualified_path.clone(),
        fields: typed_fields,
        ty: Type::Struct {
            name: name.to_string(),
            type_args,
            fields: resolved_fields,
        },
    })
}

/// Check an enum struct variant construction with resolved enum type: Enum::Variant { fields }
#[allow(clippy::too_many_arguments)]
fn check_enum_struct_construct_resolved(
    enum_type: &EnumType,
    variant_type: &EnumVariantType,
    variant_name: &str,
    provided_fields: &[(String, Expr)],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let enum_path = current_module.child(&enum_type.name);
    let qualified_variant_path = enum_path.child(variant_name);

    // Must be a struct variant
    let expected_fields = match variant_type {
        EnumVariantType::Struct(fields) => fields,
        EnumVariantType::Unit => {
            return Err(TypeError {
                message: format!(
                    "enum variant {} is a unit variant, doesn't take fields",
                    qualified_variant_path
                ),
            });
        }
        EnumVariantType::Tuple(_) => {
            return Err(TypeError {
                message: format!(
                    "enum variant {} is a tuple variant, use ( ) syntax",
                    qualified_variant_path
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
                    "missing field '{}' in enum variant {}",
                    expected, qualified_variant_path
                ),
            });
        }
    }

    for provided in &provided_names {
        if !expected_names.contains(provided) {
            return Err(TypeError {
                message: format!(
                    "unknown field '{}' in enum variant {}",
                    provided, qualified_variant_path
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
                    "field '{}' in enum variant {} expects {} but got {}: {}",
                    field_name,
                    qualified_variant_path,
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
        path: qualified_variant_path,
        fields: TypedEnumConstructFields::Struct(typed_fields),
        ty: Type::Enum {
            name: enum_type.name.to_string(),
            type_args,
            variants: resolved_variants,
        },
    })
}

/// Check a field access expression
fn check_field_access(
    expr: &Expr,
    field: &str,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_expr(expr, current_module, env, ctx)?;
    let expr_ty = ctx.resolve(&typed_expr.ty());

    match &expr_ty {
        Type::Struct {
            name,
            fields: struct_fields,
            ..
        } => {
            // Find the field directly in the resolved type
            let (_, field_type) =
                struct_fields
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
    current_module: &QualifiedPath,
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
        Expr::Match { scrutinee, arms } => {
            check_match_expr(scrutinee, arms, current_module, env, ctx)
        }
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
        Expr::FieldAccess { expr, field } => {
            check_field_access(expr, field, current_module, env, ctx)
        }
    }
}

/// Substitute type variables in a type using a mapping (recursive)
pub(crate) fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(elem) => Type::List(Box::new(substitute_type_vars(elem, mapping))),
        Type::Tuple(elems) => Type::Tuple(
            elems
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::Struct {
            name,
            type_args,
            fields,
        } => Type::Struct {
            name: name.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        },
        Type::Enum {
            name,
            type_args,
            variants,
        } => Type::Enum {
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
        EnumVariantType::Tuple(types) => EnumVariantType::Tuple(
            types
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
        ),
        EnumVariantType::Struct(fields) => EnumVariantType::Struct(
            fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        ),
    }
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
/// Create a re-exported definition by cloning and overriding visibility.
/// The module is preserved from the original definition so codegen generates
/// correct references to the actual definition location.
fn make_reexport_definition(def: &Definition) -> Definition {
    match def {
        Definition::Function(f) => Definition::Function(FunctionType {
            visibility: Visibility::Public,
            ..f.clone()
        }),
        Definition::Struct(s) => Definition::Struct(StructType {
            visibility: Visibility::Public,
            ..s.clone()
        }),
        Definition::Enum(e) => Definition::Enum(make_reexport_enum(e)),
        Definition::EnumVariant(parent_enum, variant_type) => Definition::EnumVariant(
            make_reexport_enum(parent_enum),
            variant_type.clone(),
        ),
        Definition::TypeAlias(a) => Definition::TypeAlias(TypeAliasType {
            visibility: Visibility::Public,
            ..a.clone()
        }),
        Definition::Module(m) => Definition::Module(ModuleType {
            visibility: Visibility::Public,
            ..m.clone()
        }),
    }
}

/// Create a re-exported enum type with overridden visibility.
fn make_reexport_enum(e: &EnumType) -> EnumType {
    EnumType {
        visibility: Visibility::Public,
        ..e.clone()
    }
}

/// Register a re-export: check visibility, create re-export definition, handle enum variants.
fn register_reexport(
    env: &mut TypeEnv,
    module_path: &QualifiedPath,
    local_name: &str,
    qualified: &QualifiedPath,
) -> Result<(), TypeError> {
    let def = env.definitions.get(qualified).ok_or_else(|| TypeError {
        message: format!("cannot find '{}' to re-export", qualified),
    })?;

    // Modules need special handling: create a proper Module definition
    // with the re-exporting module as parent. Module visibility is not checked
    // because re-exporting a module just creates a namespace alias.
    if matches!(def, Definition::Module(_)) {
        let reexport_path = module_path.child(local_name);
        let reexport_def = Definition::Module(ModuleType {
            visibility: Visibility::Public,
            module: module_path.clone(),
            name: local_name.to_string(),
        });
        env.register(reexport_path.clone(), reexport_def);
        env.reexports
            .insert(reexport_path, qualified.clone());
        return Ok(());
    }

    let target_visibility = def.visibility();
    if target_visibility != Visibility::Public {
        return Err(TypeError {
            message: format!("pub use cannot re-export private item '{}'", qualified),
        });
    }

    let def = def.clone();
    let reexport_path = module_path.child(local_name);

    env.register(reexport_path.clone(), make_reexport_definition(&def));
    env.reexports
        .insert(reexport_path.clone(), qualified.clone());

    // If re-exporting an enum, also re-export all its variants
    if let Definition::Enum(ref enum_type) = def {
        for (variant_name, variant_type) in &enum_type.variants {
            let variant_path = reexport_path.child(variant_name);
            let original_variant_path = qualified.child(variant_name);
            let reexported_enum = make_reexport_enum(enum_type);
            env.register(
                variant_path.clone(),
                Definition::EnumVariant(reexported_enum, variant_type.clone()),
            );
            env.reexports.insert(variant_path, original_variant_path);
        }
    }

    Ok(())
}

/// A definition is externally visible if it is public and all
/// ancestor modules between root and the definition are also public.
fn is_externally_visible(
    path: &QualifiedPath,
    def: &Definition,
    definitions: &HashMap<QualifiedPath, Definition>,
) -> bool {
    if def.visibility() != Visibility::Public {
        return false;
    }
    let segments = path.segments();
    // Check ancestors: for ["root", "a", "b", "Foo"], check ["root", "a"] and ["root", "a", "b"]
    for i in 2..segments.len() {
        let ancestor = QualifiedPath::new(segments[..i].to_vec());
        match definitions.get(&ancestor) {
            Some(ancestor_def) => {
                if ancestor_def.visibility() != Visibility::Public {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

/// Check an entire module tree, returning a checked module tree.
///
/// This performs multi-module type checking:
/// 1. Register all declarations from all modules
/// 2. Type-check all function bodies with module context for path resolution
pub fn check(pkg: &Package) -> Result<CheckedPackage, TypeError> {
    let mut env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();

    // Phase 1: Register ALL declarations from ALL modules
    // Process modules in dependency order (parents before children)
    let mut module_paths: Vec<_> = pkg.modules.keys().cloned().collect();
    module_paths.sort_by_key(|p| p.depth());

    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            register_module_declarations(&module.items, path, &mut env, &mut ctx)?;
        }
    }

    // Register modules as Definition::Module
    for path in &module_paths {
        if pkg.modules.contains_key(path) {
            let (visibility, name) = if path.segments().len() <= 1 {
                (Visibility::Public, "root".to_string())
            } else {
                let parent = path.parent().unwrap_or_else(QualifiedPath::root);
                let name = path.last().to_string();
                let vis = pkg
                    .modules
                    .get(&parent)
                    .and_then(|m| m.children.get(&name))
                    .map(|(_, v)| *v)
                    .unwrap_or(Visibility::Private);
                (vis, name)
            };
            env.register(
                path.clone(),
                Definition::Module(ModuleType {
                    visibility,
                    module: path.parent().unwrap_or_else(QualifiedPath::root),
                    name,
                }),
            );
        }
    }

    // Phase 1.5a: Register re-exports from pub use declarations
    // This must happen before import resolution so other modules can reference re-exported items.
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            for item in &module.items {
                if let Item::Use(use_decl) = item
                    && use_decl.visibility == Visibility::Public
                {
                    match &use_decl.path.target {
                        UseTarget::Single { .. } => {
                            let qualified = resolve_use_path(use_decl, path)?;
                            let local_name = use_decl.path.segments.last().unwrap();
                            register_reexport(&mut env, path, local_name, &qualified)?;
                        }
                        UseTarget::Glob => {
                            let target_path = resolve_use_module_path(use_decl, path)?;

                            // Determine container type and follow re-export chain
                            let def = env.definitions.get(&target_path).cloned();
                            let mut resolved = target_path.clone();
                            while let Some(real) = env.reexports.get(&resolved) {
                                resolved = real.clone();
                            }

                            match def {
                                Some(Definition::Module(_)) => {
                                    let module_segments = resolved.segments().to_vec();

                                    // Collect public items to re-export (skip enum variants)
                                    let items_to_reexport: Vec<(String, QualifiedPath)> = env
                                        .definitions
                                        .iter()
                                        .filter(|(qpath, def)| {
                                            qpath.len() == module_segments.len() + 1
                                                && qpath.segments()[..module_segments.len()]
                                                    == module_segments[..]
                                                && !matches!(def, Definition::EnumVariant(..))
                                                && {
                                                    let vis = match def {
                                                        Definition::Function(f) => f.visibility,
                                                        Definition::Struct(s) => s.visibility,
                                                        Definition::Enum(e) => e.visibility,
                                                        Definition::TypeAlias(a) => a.visibility,
                                                        Definition::EnumVariant(parent, _) => {
                                                            parent.visibility
                                                        }
                                                        Definition::Module(m) => m.visibility,
                                                    };
                                                    vis == Visibility::Public
                                                }
                                        })
                                        .map(|(qpath, _)| {
                                            let name = qpath.last().to_string();
                                            (name, qpath.clone())
                                        })
                                        .collect();

                                    for (local_name, qualified) in items_to_reexport {
                                        register_reexport(
                                            &mut env,
                                            path,
                                            &local_name,
                                            &qualified,
                                        )?;
                                    }
                                }
                                Some(Definition::Enum(_)) => {
                                    let enum_segments = resolved.segments().to_vec();

                                    // Collect all variants of this enum
                                    let variants_to_reexport: Vec<(String, QualifiedPath)> = env
                                        .definitions
                                        .iter()
                                        .filter(|(qpath, def)| {
                                            qpath.len() == enum_segments.len() + 1
                                                && qpath.segments()[..enum_segments.len()]
                                                    == enum_segments[..]
                                                && matches!(def, Definition::EnumVariant(..))
                                        })
                                        .map(|(qpath, _)| {
                                            let name = qpath.last().to_string();
                                            (name, qpath.clone())
                                        })
                                        .collect();

                                    for (variant_name, qualified) in variants_to_reexport {
                                        register_reexport(
                                            &mut env,
                                            path,
                                            &variant_name,
                                            &qualified,
                                        )?;
                                    }
                                }
                                _ => {
                                    return Err(TypeError {
                                        message: format!(
                                            "cannot find module or enum '{}' for glob re-export",
                                            target_path
                                        ),
                                    });
                                }
                            }
                        }
                        UseTarget::Group(items) => {
                            let target_path = resolve_use_module_path(use_decl, path)?;

                            // Verify target is a module or enum, follow re-export chain
                            match env.definitions.get(&target_path) {
                                Some(Definition::Module(_) | Definition::Enum(_)) => {}
                                _ => {
                                    return Err(TypeError {
                                        message: format!(
                                            "cannot find module or enum '{}' for group re-export",
                                            target_path
                                        ),
                                    });
                                }
                            }
                            let mut resolved = target_path;
                            while let Some(real) = env.reexports.get(&resolved) {
                                resolved = real.clone();
                            }

                            for group_item in items {
                                let qualified = resolved.child(&group_item.name);
                                let local_name =
                                    group_item.alias.as_deref().unwrap_or(&group_item.name);
                                register_reexport(&mut env, path, local_name, &qualified)?;
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 1.5b: Resolve imports for all modules
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            let uses: Vec<UseDecl> = module.items.iter().filter_map(|item| {
                if let Item::Use(u) = item { Some(u.clone()) } else { None }
            }).collect();
            let item_imports = resolve_module_imports(&uses, path, &env.definitions, &env.reexports)?;
            env.imports.insert(path.clone(), item_imports);
        }
    }

    // Phase 2: Type-check ALL function bodies
    let mut checked_items = HashMap::new();
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            let functions = check_module_bodies(&module.items, path, &env, &mut ctx)?;
            for func in functions {
                let func_path = path.child(&func.name);
                checked_items.insert(func_path, func);
            }
        }
    }

    let external_definitions = env
        .definitions
        .iter()
        .filter(|(path, def)| is_externally_visible(path, def, &env.definitions))
        .map(|(path, def)| (path.clone(), def.clone()))
        .collect();

    let external_reexports = env
        .reexports
        .iter()
        .filter(|(path, _)| {
            env.definitions
                .get(path)
                .is_some_and(|def| is_externally_visible(path, def, &env.definitions))
        })
        .map(|(path, target)| (path.clone(), target.clone()))
        .collect();

    Ok(CheckedPackage {
        name: pkg.name.clone(),
        output: pkg.output.clone(),
        items: checked_items,
        definitions: external_definitions,
        reexports: external_reexports,
    })
}

/// Register declarations from a single module into the type environment.
/// Uses fully qualified paths (e.g., root::utils::foo).
fn register_module_declarations(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(), TypeError> {
    // Phase 1a: Register all struct names with placeholder types
    for item in items {
        if let Item::Struct(def) = item {
            let qualified_path = current_module.child(&def.name);
            let mut type_var_ids = Vec::new();
            for _ in &def.type_params {
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    type_var_ids.push(id);
                }
            }
            env.register(
                qualified_path,
                Definition::Struct(StructType {
                    visibility: def.visibility,
                    module: current_module.clone(),
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    fields: vec![],
                }),
            );
        }
        if let Item::Enum(def) = item {
            let qualified_path = current_module.child(&def.name);
            let mut type_var_ids = Vec::new();
            for _ in &def.type_params {
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    type_var_ids.push(id);
                }
            }
            env.register(
                qualified_path,
                Definition::Enum(EnumType {
                    visibility: def.visibility,
                    module: current_module.clone(),
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
            let qualified_path = current_module.child(&def.name);
            let struct_type = struct_type_from_def(def, current_module, env, ctx)?;
            env.register(qualified_path, Definition::Struct(struct_type));
        }
    }

    // Phase 1c: Resolve all enum variant types
    for item in items {
        if let Item::Enum(def) = item {
            let qualified_path = current_module.child(&def.name);
            let enum_type = enum_type_from_def(def, current_module, env, ctx)?;
            env.register(qualified_path.clone(), Definition::Enum(enum_type.clone()));
            for (variant_name, variant) in &enum_type.variants {
                let variant_qualified_path = qualified_path.child(variant_name);
                env.register(
                    variant_qualified_path,
                    Definition::EnumVariant(enum_type.clone(), variant.clone()),
                );
            }
        }
    }

    // Phase 1d: Register all type aliases
    for item in items {
        if let Item::TypeAlias(def) = item {
            let qualified_path = current_module.child(&def.name);
            let alias_type = type_alias_from_def(def, current_module, env, ctx)?;
            env.register(qualified_path, Definition::TypeAlias(alias_type));
        }
    }

    // Phase 2: Register all function signatures
    for item in items {
        if let Item::Function(func) = item {
            let qualified_path = current_module.child(&func.name);
            let func_type = function_type_from_def(func, current_module, env, ctx)?;
            env.register(qualified_path, Definition::Function(func_type));
        }
    }

    Ok(())
}

/// Type-check function bodies from a single module.
fn check_module_bodies(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<Vec<TypedFunction>, TypeError> {
    let mut checked_items = Vec::new();

    for item in items {
        if let Item::Function(func) = item {
            let typed = check_function_in_module(func, current_module, env, ctx)?;
            checked_items.push(typed);
        }
    }

    Ok(checked_items)
}

/// Check a function definition within a specific module context.
fn check_function_in_module(
    func: &FunctionDef,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedFunction, TypeError> {
    check_function(func, current_module, env, ctx)
}

#[cfg(test)]
mod tests;
