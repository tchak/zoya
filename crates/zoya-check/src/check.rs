use std::collections::{HashMap, HashSet};

use zoya_ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, ListElement, MatchArm, Path, StructKind,
    TupleElement, TypeAnnotation, UnaryOp, UseDecl, UseTarget,
};
use zoya_ir::{
    CheckedPackage, Definition, EnumType, EnumVariantType, FunctionType, ModuleType, QualifiedPath,
    StructType, StructTypeKind, Type, TypeAliasType, TypeError, TypeScheme, TypeVarId,
    TypedEnumConstructFields, TypedExpr, TypedFunction, Visibility,
};
use zoya_package::Package;

use crate::builtin::{builtin_method, is_numeric_type};
use crate::definition::{
    enum_type_from_def, function_type_from_def, struct_type_from_def, type_alias_from_def,
};
use crate::imports::{
    ImportTable, resolve_module_imports, resolve_use_module_path, resolve_use_path,
};
use crate::pattern::{check_irrefutable, check_let_binding, check_match_arm, check_pattern};
use crate::resolution::{self, ResolvedPath};
use crate::type_resolver::resolve_type_annotation;
use crate::unify::{UnifyCtx, substitute_type_vars, substitute_variant_type_vars};
use crate::usefulness;
use zoya_naming::{is_pascal_case, is_snake_case, to_pascal_case, to_snake_case};

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
    package_name: &str,
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

    // Check for #[builtin] attribute
    let is_builtin = func.attributes.iter().any(|a| a.name == "builtin");

    if is_builtin {
        // Validate: only allowed in std package
        if package_name != "std" {
            return Err(TypeError {
                message: "the #[builtin] attribute can only be used in the standard library"
                    .to_string(),
            });
        }
        // Validate: must have explicit return type
        if func.return_type.is_none() {
            return Err(TypeError {
                message: "#[builtin] functions must have an explicit return type".to_string(),
            });
        }
        // Validate: body must be unit `()`
        if func.body != Expr::Tuple(vec![]) {
            return Err(TypeError {
                message: "#[builtin] functions must have a unit body ()".to_string(),
            });
        }

        let declared_return = resolve_type_annotation(
            func.return_type.as_ref().unwrap(),
            &type_param_map,
            current_module,
            env,
        )?;

        // Create environment with locals for checking body (still needed for typed_body)
        let body_env = env.with_locals(locals);
        let typed_body = check_expr(&func.body, current_module, &body_env, ctx)?;

        return Ok(TypedFunction {
            name: func.name.clone(),
            params: typed_params,
            body: typed_body,
            return_type: ctx.resolve(&declared_return),
            is_builtin: true,
        });
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
        is_builtin: false,
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
    let resolved = resolution::resolve_expr_path(
        path,
        current_module,
        &env.locals,
        &env.imports,
        &env.definitions,
        &env.reexports,
    )?;

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

            let resolved_variants = resolve_enum_variants(&enum_type.variants, &instantiation, ctx);

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
            def: Definition::Struct(struct_type),
        } if struct_type.kind == StructTypeKind::Unit => {
            // Unit struct used as a bare path: `Empty`
            Ok(TypedExpr::StructConstruct {
                path: qualified_path.clone(),
                fields: vec![],
                spread: None,
                ty: Type::Struct {
                    name: struct_type.name.clone(),
                    type_args: vec![],
                    fields: vec![],
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
    let resolved = resolution::resolve_expr_path(
        path,
        current_module,
        &env.locals,
        &env.imports,
        &env.definitions,
        &env.reexports,
    )?;

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
        } => check_function_call(
            path,
            &qualified_path,
            func_type,
            args,
            current_module,
            env,
            ctx,
        ),
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
            def: Definition::Struct(struct_type),
            qualified_path,
        } if struct_type.kind == StructTypeKind::Tuple => check_struct_tuple_construct_resolved(
            struct_type,
            &qualified_path,
            &path.type_args,
            args,
            current_module,
            env,
            ctx,
        ),
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

    let resolved_variants = resolve_enum_variants(&enum_type.variants, &instantiation, ctx);

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

/// Check a tuple struct construction with a resolved struct type: Struct(args)
#[allow(clippy::too_many_arguments)]
fn check_struct_tuple_construct_resolved(
    struct_type: &StructType,
    qualified_path: &QualifiedPath,
    explicit_type_args: &Option<Vec<TypeAnnotation>>,
    args: &[Expr],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let name = &struct_type.name;

    // Check argument count
    if args.len() != struct_type.fields.len() {
        return Err(TypeError {
            message: format!(
                "tuple struct {} expects {} argument(s), got {}",
                name,
                struct_type.fields.len(),
                args.len()
            ),
        });
    }

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
        if type_args.len() != struct_type.type_params.len() {
            return Err(TypeError {
                message: format!(
                    "struct {} expects {} type argument(s), got {}",
                    name,
                    struct_type.type_params.len(),
                    type_args.len()
                ),
            });
        }
        let resolved: Vec<Type> = type_args
            .iter()
            .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
            .collect::<Result<_, _>>()?;
        struct_type
            .type_var_ids
            .iter()
            .zip(resolved)
            .map(|(&id, ty)| (id, ty))
            .collect()
    } else {
        struct_type
            .type_var_ids
            .iter()
            .map(|&id| (id, ctx.fresh_var()))
            .collect()
    };

    // Type check arguments and unify with field types
    let mut typed_args = Vec::new();
    for (arg, (_, field_type)) in args.iter().zip(struct_type.fields.iter()) {
        let expected_type = substitute_type_vars(field_type, &instantiation);
        let typed_arg = check_expr(arg, current_module, env, ctx)?;
        let actual_type = typed_arg.ty();

        ctx.unify(&actual_type, &expected_type)
            .map_err(|e| TypeError {
                message: format!(
                    "in tuple struct {} expected {} but got {}: {}",
                    name,
                    ctx.resolve(&expected_type),
                    ctx.resolve(&actual_type),
                    e.message
                ),
            })?;

        typed_args.push(typed_arg);
    }

    // Build the struct type with resolved type arguments
    let type_args: Vec<Type> = struct_type
        .type_var_ids
        .iter()
        .map(|id| ctx.resolve(&instantiation[id]))
        .collect();

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

    Ok(TypedExpr::StructTupleConstruct {
        path: qualified_path.clone(),
        args: typed_args,
        ty: Type::Struct {
            name: name.to_string(),
            type_args,
            fields: resolved_fields,
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
            // Type variables are allowed through (they may resolve to numeric types later)
            let resolved = ctx.resolve(&ty);
            match resolved {
                Type::Int | Type::BigInt | Type::Float | Type::Var(_) => Ok(TypedExpr::UnaryOp {
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
    let op_symbol = match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
    };
    ctx.unify(&left_ty, &right_ty).map_err(|e| TypeError {
        message: format!(
            "binary operator '{}' requires operands of the same type, but got {} and {}: {}",
            op_symbol,
            ctx.resolve(&left_ty),
            ctx.resolve(&right_ty),
            e.message
        ),
    })?;

    let resolved_ty = ctx.resolve(&left_ty);

    // Determine result type based on operator
    let result_ty = match op {
        // Arithmetic operators: only work on numeric types, result has same type as operands
        // Type variables are allowed through (they may resolve to numeric types later via unification)
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
            if !is_numeric_type(&resolved_ty) && !matches!(resolved_ty, Type::Var(_)) {
                return Err(TypeError {
                    message: format!(
                        "arithmetic operators only work on numeric types, not {}",
                        resolved_ty
                    ),
                });
            }
            resolved_ty
        }

        // Equality operators: work on any type, result is Bool
        BinOp::Eq | BinOp::Ne => Type::Bool,

        // Ordering operators: only work on numeric types, result is Bool
        // Type variables are allowed through (they may resolve to numeric types later via unification)
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
            if !is_numeric_type(&resolved_ty) && !matches!(resolved_ty, Type::Var(_)) {
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
    let def_lookup = usefulness::DefinitionLookup::from_definitions(&env.definitions);
    usefulness::check_patterns(&typed_arms, &resolved_scrutinee_ty, &def_lookup)?;

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
    elements: &[ListElement],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    if elements.iter().any(|e| matches!(e, ListElement::Spread(_))) {
        todo!("spread in list expressions")
    }

    // Extract inner expressions (all are Item at this point)
    let exprs: Vec<&Expr> = elements
        .iter()
        .map(|e| match e {
            ListElement::Item(expr) => expr,
            ListElement::Spread(_) => unreachable!(),
        })
        .collect();

    if exprs.is_empty() {
        // Empty list: create fresh type variable for element type
        let elem_ty = ctx.fresh_var();
        Ok(TypedExpr::List {
            elements: vec![],
            ty: Type::List(Box::new(elem_ty)),
        })
    } else {
        // Non-empty list: infer element type from first element
        let first_typed = check_expr(exprs[0], current_module, env, ctx)?;
        let elem_ty = first_typed.ty();
        let mut typed_elements = vec![first_typed];

        // Check remaining elements unify with first element's type
        for elem in &exprs[1..] {
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
    elements: &[TupleElement],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    if elements
        .iter()
        .any(|e| matches!(e, TupleElement::Spread(_)))
    {
        todo!("spread in tuple expressions")
    }

    let mut typed_elements = Vec::new();
    let mut element_types = Vec::new();

    for elem in elements {
        let expr = match elem {
            TupleElement::Item(expr) => expr,
            TupleElement::Spread(_) => unreachable!(),
        };
        let typed = check_expr(expr, current_module, env, ctx)?;
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
    spread: &Option<Box<Expr>>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let resolved = resolution::resolve_expr_path(
        path,
        current_module,
        &env.locals,
        &env.imports,
        &env.definitions,
        &env.reexports,
    )?;

    match resolved {
        ResolvedPath::Definition {
            def: Definition::Struct(struct_type),
            qualified_path,
        } if struct_type.kind == StructTypeKind::Tuple => Err(TypeError {
            message: format!(
                "tuple struct '{}' must be constructed with () syntax, not {{}}",
                qualified_path
            ),
        }),
        ResolvedPath::Definition {
            def: Definition::Struct(struct_type),
            qualified_path,
        } => check_struct_construct_resolved(
            &qualified_path,
            struct_type,
            &path.type_args,
            fields,
            spread,
            current_module,
            env,
            ctx,
        ),
        ResolvedPath::Definition {
            def: Definition::EnumVariant(def, variant),
            qualified_path,
        } => match variant {
            EnumVariantType::Struct(_) => {
                if spread.is_some() {
                    return Err(TypeError {
                        message: "spread is not supported for enum struct variants".to_string(),
                    });
                }
                check_enum_struct_construct_resolved(
                    def,
                    variant,
                    qualified_path.last(),
                    &path.type_args,
                    fields,
                    current_module,
                    env,
                    ctx,
                )
            }
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
#[allow(clippy::too_many_arguments)]
fn check_struct_construct_resolved(
    qualified_path: &QualifiedPath,
    struct_type: &StructType,
    explicit_type_args: &Option<Vec<TypeAnnotation>>,
    fields: &[(String, Expr)],
    spread: &Option<Box<Expr>>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let name = &struct_type.name;
    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
        if type_args.len() != struct_type.type_params.len() {
            return Err(TypeError {
                message: format!(
                    "struct {} expects {} type argument(s), got {}",
                    name,
                    struct_type.type_params.len(),
                    type_args.len()
                ),
            });
        }
        let resolved: Vec<Type> = type_args
            .iter()
            .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
            .collect::<Result<_, _>>()?;
        struct_type
            .type_var_ids
            .iter()
            .zip(resolved)
            .map(|(&id, ty)| (id, ty))
            .collect()
    } else {
        struct_type
            .type_var_ids
            .iter()
            .map(|&id| (id, ctx.fresh_var()))
            .collect()
    };

    // Check that all required fields are present and no extra fields
    let expected_field_names: HashSet<&str> =
        struct_type.fields.iter().map(|(n, _)| n.as_str()).collect();
    let provided_field_names: HashSet<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();

    // Check for missing fields (skip if spread is present — spread fills remaining fields)
    if spread.is_none() {
        for expected in &expected_field_names {
            if !provided_field_names.contains(expected) {
                return Err(TypeError {
                    message: format!("missing field '{}' in struct {}", expected, name),
                });
            }
        }
    }

    // Check for extra/unknown fields (always check, even with spread)
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

    // Type-check spread expression if present
    let typed_spread = if let Some(spread_expr) = spread {
        let typed_spread_expr = check_expr(spread_expr, current_module, env, ctx)?;
        let spread_type = typed_spread_expr.ty();

        // Build the expected struct type for unification
        let expected_struct_type = Type::Struct {
            name: name.to_string(),
            type_args: struct_type
                .type_var_ids
                .iter()
                .map(|id| instantiation[id].clone())
                .collect(),
            fields: struct_type
                .fields
                .iter()
                .map(|(field_name, ty)| {
                    (field_name.clone(), substitute_type_vars(ty, &instantiation))
                })
                .collect(),
        };

        ctx.unify(&spread_type, &expected_struct_type)
            .map_err(|e| TypeError {
                message: format!(
                    "spread expression in struct {} has type {} but expected {}: {}",
                    name,
                    ctx.resolve(&spread_type),
                    ctx.resolve(&expected_struct_type),
                    e.message
                ),
            })?;

        Some(Box::new(typed_spread_expr))
    } else {
        None
    };

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
        spread: typed_spread,
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
    explicit_type_args: &Option<Vec<TypeAnnotation>>,
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

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
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
        let resolved: Vec<Type> = type_args
            .iter()
            .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
            .collect::<Result<_, _>>()?;
        enum_type
            .type_var_ids
            .iter()
            .zip(resolved)
            .map(|(&id, ty)| (id, ty))
            .collect()
    } else {
        enum_type
            .type_var_ids
            .iter()
            .map(|&id| (id, ctx.fresh_var()))
            .collect()
    };

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

    let resolved_variants = resolve_enum_variants(&enum_type.variants, &instantiation, ctx);

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
            type_args,
            ..
        } => {
            // If fields are empty (recursive type stub), look up real fields from definitions
            let actual_fields: Vec<(String, Type)> = if struct_fields.is_empty() {
                lookup_struct_fields(name, type_args, &env.definitions)
                    .unwrap_or_else(|| struct_fields.clone())
            } else {
                struct_fields.clone()
            };

            let (_, field_type) =
                actual_fields
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

/// Check a tuple index expression: `tuple.0`, `pair.1`
fn check_tuple_index(
    expr: &Expr,
    index: u64,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_expr(expr, current_module, env, ctx)?;
    let expr_ty = ctx.resolve(&typed_expr.ty());
    let index_usize = index as usize;

    match &expr_ty {
        Type::Tuple(elements) => {
            if index_usize >= elements.len() {
                return Err(TypeError {
                    message: format!(
                        "tuple index {} is out of bounds for tuple with {} elements",
                        index,
                        elements.len()
                    ),
                });
            }
            Ok(TypedExpr::TupleIndex {
                expr: Box::new(typed_expr),
                index: index_usize,
                ty: elements[index_usize].clone(),
            })
        }
        Type::Struct {
            name,
            fields: struct_fields,
            type_args,
            ..
        } => {
            let actual_fields: Vec<(String, Type)> = if struct_fields.is_empty() {
                lookup_struct_fields(name, type_args, &env.definitions)
                    .unwrap_or_else(|| struct_fields.clone())
            } else {
                struct_fields.clone()
            };

            let field_name = format!("${}", index);
            let (_, field_type) = actual_fields
                .iter()
                .find(|(n, _)| n == &field_name)
                .ok_or_else(|| TypeError {
                    message: format!("cannot use tuple index {} on struct {}", index, name),
                })?;

            Ok(TypedExpr::TupleIndex {
                expr: Box::new(typed_expr),
                index: index_usize,
                ty: field_type.clone(),
            })
        }
        _ => Err(TypeError {
            message: format!("cannot use tuple index on type {}", expr_ty),
        }),
    }
}

/// Check a list index expression: `list[index]` -> Option<T>
fn check_list_index(
    expr: &Expr,
    index: &Expr,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let typed_expr = check_expr(expr, current_module, env, ctx)?;
    let expr_ty = ctx.resolve(&typed_expr.ty());

    // Verify receiver is a List<T>
    let elem_ty = match &expr_ty {
        Type::List(elem) => *elem.clone(),
        _ => {
            return Err(TypeError {
                message: format!("cannot index into non-list type {}", expr_ty),
            });
        }
    };

    // Type-check the index and verify it's Int
    let typed_index = check_expr(index, current_module, env, ctx)?;
    let index_ty = ctx.resolve(&typed_index.ty());
    ctx.unify(&index_ty, &Type::Int).map_err(|_| TypeError {
        message: format!("list index must be Int, got {}", index_ty),
    })?;

    // Return type is Option<T>
    let option_ty = Type::Enum {
        name: "Option".to_string(),
        type_args: vec![elem_ty.clone()],
        variants: vec![
            ("None".to_string(), EnumVariantType::Unit),
            ("Some".to_string(), EnumVariantType::Tuple(vec![elem_ty])),
        ],
    };

    Ok(TypedExpr::ListIndex {
        expr: Box::new(typed_expr),
        index: Box::new(typed_index),
        ty: option_ty,
    })
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
        Expr::Struct {
            path,
            fields,
            spread,
        } => check_path_struct(path, fields, spread, current_module, env, ctx),
        Expr::FieldAccess { expr, field } => {
            check_field_access(expr, field, current_module, env, ctx)
        }
        Expr::TupleIndex { expr, index } => {
            check_tuple_index(expr, *index, current_module, env, ctx)
        }
        Expr::ListIndex { expr, index } => check_list_index(expr, index, current_module, env, ctx),
    }
}

/// Substitute and resolve enum variants using an instantiation mapping.
///
/// Applies type variable substitution to each variant, then resolves
/// any remaining type variables through the unification context.
fn resolve_enum_variants(
    variants: &[(String, EnumVariantType)],
    instantiation: &HashMap<TypeVarId, Type>,
    ctx: &UnifyCtx,
) -> Vec<(String, EnumVariantType)> {
    variants
        .iter()
        .map(|(name, vt)| {
            let substituted = substitute_variant_type_vars(vt, instantiation);
            let resolved = match substituted {
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
            (name.clone(), resolved)
        })
        .collect()
}

/// Create fresh type variables for an enum type's type parameters and build
/// the instantiated `Type::Enum`. Returns the instantiation map and the type.
pub(crate) fn instantiate_enum_type(
    enum_type: &EnumType,
    ctx: &mut UnifyCtx,
) -> (HashMap<TypeVarId, Type>, Type) {
    let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
    for &old_id in &enum_type.type_var_ids {
        instantiation.insert(old_id, ctx.fresh_var());
    }
    let type_args: Vec<Type> = enum_type
        .type_var_ids
        .iter()
        .map(|id| instantiation[id].clone())
        .collect();
    let resolved_variants: Vec<(String, EnumVariantType)> = enum_type
        .variants
        .iter()
        .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, &instantiation)))
        .collect();
    let ty = Type::Enum {
        name: enum_type.name.clone(),
        type_args,
        variants: resolved_variants,
    };
    (instantiation, ty)
}

/// Create fresh type variables for a struct type's type parameters and build
/// the instantiated `Type::Struct`. Returns the instantiation map and the type.
pub(crate) fn instantiate_struct_type(
    struct_type: &StructType,
    ctx: &mut UnifyCtx,
) -> (HashMap<TypeVarId, Type>, Type) {
    let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
    for &old_id in &struct_type.type_var_ids {
        instantiation.insert(old_id, ctx.fresh_var());
    }
    let type_args: Vec<Type> = struct_type
        .type_var_ids
        .iter()
        .map(|id| instantiation[id].clone())
        .collect();
    let resolved_fields: Vec<(String, Type)> = struct_type
        .fields
        .iter()
        .map(|(n, ty)| (n.clone(), substitute_type_vars(ty, &instantiation)))
        .collect();
    let ty = Type::Struct {
        name: struct_type.name.clone(),
        type_args,
        fields: resolved_fields,
    };
    (instantiation, ty)
}

/// Look up struct fields from definitions when the type carries empty fields (recursive type stub).
fn lookup_struct_fields(
    name: &str,
    type_args: &[Type],
    definitions: &HashMap<QualifiedPath, Definition>,
) -> Option<Vec<(String, Type)>> {
    for def in definitions.values() {
        if let Definition::Struct(struct_type) = def
            && struct_type.name == name
            && struct_type.kind == StructTypeKind::Named
        {
            if type_args.is_empty() || struct_type.type_var_ids.is_empty() {
                return Some(struct_type.fields.clone());
            }
            // Build substitution: type_var_ids -> type_args
            let mapping: HashMap<TypeVarId, Type> = struct_type
                .type_var_ids
                .iter()
                .zip(type_args.iter())
                .map(|(id, ty)| (*id, ty.clone()))
                .collect();
            return Some(
                struct_type
                    .fields
                    .iter()
                    .map(|(n, t)| (n.clone(), substitute_type_vars(t, &mapping)))
                    .collect(),
            );
        }
    }
    None
}

/// Check if an expression is a syntactic value (safe to generalize under value restriction)
fn is_syntactic_value(expr: &Expr) -> bool {
    match expr {
        Expr::Lambda { .. } => true,
        Expr::Int(_) | Expr::BigInt(_) | Expr::Float(_) | Expr::Bool(_) | Expr::String(_) => true,
        Expr::List(elems) => elems.iter().all(|e| match e {
            ListElement::Item(expr) => is_syntactic_value(expr),
            ListElement::Spread(_) => false,
        }),
        Expr::Tuple(elems) => elems.iter().all(|e| match e {
            TupleElement::Item(expr) => is_syntactic_value(expr),
            TupleElement::Spread(_) => false,
        }),
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
        Definition::EnumVariant(parent_enum, variant_type) => {
            Definition::EnumVariant(make_reexport_enum(parent_enum), variant_type.clone())
        }
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

/// Process all `pub use` re-export declarations across all modules in a single pass.
/// Returns Ok(()) after registering any new re-exports found.
/// Called in a fixpoint loop so that cascading re-exports between same-depth modules
/// are resolved regardless of processing order.
fn process_reexports(
    module_paths: &[QualifiedPath],
    pkg: &Package,
    env: &mut TypeEnv,
) -> Result<(), TypeError> {
    for path in module_paths {
        if let Some(module) = pkg.modules.get(path) {
            for item in &module.items {
                if let Item::Use(use_decl) = item
                    && use_decl.visibility == Visibility::Public
                {
                    match &use_decl.path.target {
                        UseTarget::Single { .. } => {
                            let qualified = resolve_use_path(use_decl, path)?;
                            let local_name = use_decl.path.segments.last().unwrap();
                            register_reexport(env, path, local_name, &qualified)?;
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

                                    // Collect public items to re-export (skip enum variants
                                    // unless they were re-exported to module level)
                                    let items_to_reexport: Vec<(String, QualifiedPath)> = env
                                        .definitions
                                        .iter()
                                        .filter(|(qpath, def)| {
                                            qpath.len() == module_segments.len() + 1
                                                && qpath.segments()[..module_segments.len()]
                                                    == module_segments[..]
                                                && (!matches!(def, Definition::EnumVariant(..))
                                                    || env.reexports.contains_key(qpath))
                                                && def.visibility() == Visibility::Public
                                        })
                                        .map(|(qpath, _)| {
                                            let name = qpath.last().to_string();
                                            (name, qpath.clone())
                                        })
                                        .collect();

                                    for (local_name, qualified) in items_to_reexport {
                                        register_reexport(env, path, &local_name, &qualified)?;
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
                                        register_reexport(env, path, &variant_name, &qualified)?;
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
                                register_reexport(env, path, local_name, &qualified)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
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
        env.reexports.insert(reexport_path, qualified.clone());
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
pub fn check(pkg: &Package, deps: &[&CheckedPackage]) -> Result<CheckedPackage, TypeError> {
    let mut env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();

    // Phase 0: Inject dependency definitions
    for dep in deps {
        for (qpath, def) in &dep.definitions {
            let remapped = qpath.with_root(&dep.name);
            env.register(remapped, def.clone().with_root(&dep.name));
        }
        for (reexport, original) in &dep.reexports {
            let remapped_reexport = reexport.with_root(&dep.name);
            let remapped_original = original.with_root(&dep.name);
            env.reexports.insert(remapped_reexport, remapped_original);
        }
    }

    // Phase 0.5: Pre-resolve package imports so they're available during declaration registration.
    // Package-prefix imports (e.g., `use std::option::Option`) reference dep definitions
    // already injected in Phase 0, so they can be resolved before Phase 1.
    let mut module_paths: Vec<_> = pkg.modules.keys().cloned().collect();
    module_paths.sort_by_key(|p| p.depth());

    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            let pkg_uses: Vec<UseDecl> = module
                .items
                .iter()
                .filter_map(|item| {
                    if let Item::Use(u) = item
                        && matches!(u.path.prefix, zoya_ast::PathPrefix::Package(_))
                    {
                        Some(u.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if !pkg_uses.is_empty() {
                let item_imports =
                    resolve_module_imports(&pkg_uses, path, &env.definitions, &env.reexports)?;
                env.imports
                    .entry(path.clone())
                    .or_default()
                    .extend(item_imports);
            }
        }
    }

    // Phase 0.6: Inject prelude imports for non-std packages
    // Prelude names (Option, Some, None, Result, Ok, Err) are auto-imported into
    // every module so they can be used without explicit imports, including in
    // function signatures resolved during Phase 1.
    if pkg.name != "std" {
        let prelude_path = QualifiedPath::new(vec!["std".into(), "prelude".into()]);
        if env.definitions.contains_key(&prelude_path) {
            let prelude_items: Vec<(String, QualifiedPath)> = env
                .definitions
                .iter()
                .filter(|(qpath, def)| {
                    qpath.len() == 3
                        && qpath.segments()[0] == "std"
                        && qpath.segments()[1] == "prelude"
                        && def.visibility() == Visibility::Public
                        && !matches!(def, Definition::Module(_))
                })
                .map(|(qpath, _)| (qpath.last().to_string(), qpath.clone()))
                .collect();

            for path in &module_paths {
                let module_imports = env.imports.entry(path.clone()).or_default();
                for (name, qpath) in &prelude_items {
                    module_imports
                        .entry(name.clone())
                        .or_insert_with(|| qpath.clone());
                }
            }
        }
    }

    // Phase 1: Register ALL declarations from ALL modules
    // Split into 3 global passes so all type names exist before any function
    // signature is resolved. This ensures cross-module references between
    // same-depth sibling modules work regardless of iteration order.

    // Pass 1: Register all struct/enum names with placeholder types
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            register_type_names(&module.items, path, &mut env, &mut ctx)?;
        }
    }

    // Pass 2: Resolve struct fields, enum variants, type aliases
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            resolve_type_definitions(&module.items, path, &mut env, &mut ctx)?;
        }
    }

    // Early import pass: resolve internal imports that target types.
    // This runs before function signatures are registered (Pass 3) so that
    // `use super::result::Result` etc. are available in type annotations.
    // Each use is resolved individually; imports that fail (e.g., targeting
    // not-yet-registered functions) are silently skipped — they will be
    // resolved in the full import pass after Pass 3.
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            for item in &module.items {
                if let Item::Use(u) = item
                    && !matches!(u.path.prefix, zoya_ast::PathPrefix::Package(_))
                    && let Ok(item_imports) = resolve_module_imports(
                        std::slice::from_ref(u),
                        path,
                        &env.definitions,
                        &env.reexports,
                    )
                {
                    env.imports
                        .entry(path.clone())
                        .or_default()
                        .extend(item_imports);
                }
            }
        }
    }

    // Pass 3: Register function signatures (all types and imports now available)
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            register_function_signatures(&module.items, path, &mut env, &mut ctx)?;
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

    // Register re-exports from pub use declarations (fixpoint)
    // Re-exports may depend on other re-exports at the same module depth (e.g., prelude
    // re-exports from option's re-exports). Since module_paths order within the same depth
    // is non-deterministic (HashMap iteration), we iterate until no new definitions are
    // registered, ensuring all cascading re-exports are resolved.
    loop {
        let prev_count = env.definitions.len();
        process_reexports(&module_paths, pkg, &mut env)?;
        if env.definitions.len() == prev_count {
            break;
        }
    }

    // Full import pass: resolve all internal imports (root::, super::, self::)
    // now that functions, modules, and re-exports are all registered.
    // Skip package-prefix imports (already resolved in Phase 0.5).
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            let uses: Vec<UseDecl> = module
                .items
                .iter()
                .filter_map(|item| {
                    if let Item::Use(u) = item
                        && !matches!(u.path.prefix, zoya_ast::PathPrefix::Package(_))
                    {
                        Some(u.clone())
                    } else {
                        None
                    }
                })
                .collect();
            let item_imports =
                resolve_module_imports(&uses, path, &env.definitions, &env.reexports)?;
            env.imports
                .entry(path.clone())
                .or_default()
                .extend(item_imports);
        }
    }

    // Phase 2: Type-check ALL function bodies
    // check_module_bodies updates each function's definition return type immediately
    // after checking its body, so subsequent functions see resolved concrete types
    // instead of the Phase 1 unresolved Type::Var.
    let mut checked_items = HashMap::new();
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            let functions =
                check_module_bodies(&module.items, path, &mut env, &mut ctx, &pkg.name)?;
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

    // Build imports map: for each dep, collect function definition paths that were injected
    let mut imports: HashMap<String, Vec<QualifiedPath>> = HashMap::new();
    for dep in deps {
        let mut fn_paths: Vec<QualifiedPath> = dep
            .definitions
            .iter()
            .filter(|(_, def)| def.as_function().is_some())
            .map(|(qpath, _)| qpath.with_root(&dep.name))
            .collect();
        fn_paths.sort_by_key(|a| a.to_string());
        if !fn_paths.is_empty() {
            imports.insert(dep.name.clone(), fn_paths);
        }
    }

    Ok(CheckedPackage {
        name: pkg.name.clone(),
        output: pkg.output.clone(),
        items: checked_items,
        definitions: external_definitions,
        reexports: external_reexports,
        imports,
    })
}

/// Pass 1: Register all struct/enum names with placeholder types.
/// This ensures type names are available for cross-module references.
fn register_type_names(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(), TypeError> {
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
            let stub_kind = match &def.kind {
                StructKind::Unit => StructTypeKind::Unit,
                StructKind::Tuple(_) => StructTypeKind::Tuple,
                StructKind::Named(_) => StructTypeKind::Named,
            };
            env.register(
                qualified_path,
                Definition::Struct(StructType {
                    visibility: def.visibility,
                    module: current_module.clone(),
                    name: def.name.clone(),
                    type_params: def.type_params.clone(),
                    type_var_ids,
                    kind: stub_kind,
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
    Ok(())
}

/// Pass 2: Resolve struct fields, enum variants, and type aliases.
/// All type names are registered from Pass 1, so cross-module references work.
fn resolve_type_definitions(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(), TypeError> {
    // Resolve struct field types
    for item in items {
        if let Item::Struct(def) = item {
            let qualified_path = current_module.child(&def.name);
            let struct_type = struct_type_from_def(def, current_module, env, ctx)?;
            env.register(qualified_path, Definition::Struct(struct_type));
        }
    }

    // Resolve enum variant types
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

    // Register type aliases
    for item in items {
        if let Item::TypeAlias(def) = item {
            let qualified_path = current_module.child(&def.name);
            let alias_type = type_alias_from_def(def, current_module, env, ctx)?;
            env.register(qualified_path, Definition::TypeAlias(alias_type));
        }
    }

    Ok(())
}

/// Pass 3: Register all function signatures.
/// All types (structs, enums, aliases) are fully resolved, so function
/// signatures can reference types from any module.
fn register_function_signatures(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(), TypeError> {
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
/// After each function body is checked, its definition's return type is updated
/// with the resolved type so that subsequent functions see concrete types.
fn check_module_bodies(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
    package_name: &str,
) -> Result<Vec<TypedFunction>, TypeError> {
    let mut checked_items = Vec::new();

    for item in items {
        if let Item::Function(func) = item {
            let typed = check_function_in_module(func, current_module, env, ctx, package_name)?;
            // Update the definition's return type immediately so subsequent
            // functions that call this one see the resolved concrete type
            // instead of the Phase 1 unresolved Type::Var.
            let func_path = current_module.child(&func.name);
            if let Some(Definition::Function(ft)) = env.definitions.get_mut(&func_path) {
                ft.return_type = typed.return_type.clone();
            }
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
    package_name: &str,
) -> Result<TypedFunction, TypeError> {
    check_function(func, current_module, env, ctx, package_name)
}

#[cfg(test)]
mod tests;
