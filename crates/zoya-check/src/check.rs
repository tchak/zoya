use std::collections::{HashMap, HashSet};

use zoya_ast::{
    AttributeArg, BinOp, Expr, FunctionDef, ImplBlock, ImplMethod, Item, LetBinding, ListElement,
    MatchArm, Path, StringPart, StructKind, TupleElement, TypeAnnotation, UnaryOp, UseDecl,
    UseTarget,
};
use zoya_ir::{
    CheckedPackage, Definition, EnumType, EnumVariantType, FunctionKind, FunctionType, HttpMethod,
    ImplMethodType, ModuleType, Pathname, QualifiedPath, StructType, StructTypeKind, Type,
    TypeAliasType, TypeError, TypeScheme, TypeVarId, TypedEnumConstructFields, TypedExpr,
    TypedFunction, TypedListElement, TypedPattern, TypedStringPart, Visibility,
};
use zoya_package::Package;

use crate::builtin::{is_numeric_type, primitive_method_module, primitive_module_for_name};
use crate::definition::{
    enum_type_from_def, function_type_from_def, struct_type_from_def, type_alias_from_def,
};
use crate::imports::{
    ImportTable, resolve_module_imports, resolve_use_module_path, resolve_use_path,
};
use crate::pattern::{check_irrefutable, check_let_binding, check_match_arm, check_pattern};
use crate::resolution::{self, ResolvedPath};
use crate::type_resolver::{resolve_type_annotation, resolve_type_annotation_with_self};
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
        return Err(TypeError::NamingConvention {
            kind: "function name".to_string(),
            name: func.name.clone(),
            convention: "snake_case".to_string(),
            suggestion: to_snake_case(&func.name),
        });
    }

    // Create type variables for type parameters
    // Check type parameter names are PascalCase
    for name in &func.type_params {
        if !is_pascal_case(name) {
            return Err(TypeError::NamingConvention {
                kind: "type parameter".to_string(),
                name: name.clone(),
                convention: "PascalCase".to_string(),
                suggestion: to_pascal_case(name),
            });
        }
    }
    let func_path = current_module.child(&func.name);
    let mut type_param_map = HashMap::new();
    if let Some(Definition::Function(stored_ft)) = env.definitions.get(&func_path) {
        // Reuse Phase 1 TypeVarIds for consistency with stored type_var_ids
        for (name, &id) in func.type_params.iter().zip(stored_ft.type_var_ids.iter()) {
            type_param_map.insert(name.clone(), id);
        }
    } else {
        // Fallback (e.g. tests that don't pre-register)
        for name in &func.type_params {
            let var = ctx.fresh_var();
            if let Type::Var(id) = var {
                type_param_map.insert(name.clone(), id);
            }
        }
    }

    // Build local environment with parameters
    let mut locals = HashMap::new();
    let mut typed_params = Vec::new();

    for param in &func.params {
        // Check pattern is irrefutable
        check_irrefutable(&param.pattern).map_err(|msg| TypeError::RefutablePattern {
            context: "function parameter".to_string(),
            detail: msg,
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

    // Check for #[builtin], #[test], #[job], and HTTP method attributes
    let kind = {
        let is_builtin = func.attributes.iter().any(|a| a.name == "builtin");
        let is_test = func.attributes.iter().any(|a| a.name == "test");
        let is_job = func.attributes.iter().any(|a| a.name == "job");
        let http_attr = func
            .attributes
            .iter()
            .find_map(|a| HttpMethod::from_attr_name(&a.name).map(|method| (method, a)));

        let mut special_names: Vec<&str> = Vec::new();
        if is_builtin {
            special_names.push("builtin");
        }
        if is_test {
            special_names.push("test");
        }
        if is_job {
            special_names.push("job");
        }
        if let Some((method, _)) = &http_attr {
            special_names.push(method.attr_name());
        }

        if special_names.len() > 1 {
            return Err(TypeError::InvalidAttribute {
                message: format!(
                    "a function cannot have both #[{}] and #[{}] attributes",
                    special_names[0], special_names[1]
                ),
            });
        }

        if is_builtin {
            FunctionKind::Builtin
        } else if is_test {
            FunctionKind::Test
        } else if is_job {
            FunctionKind::Job
        } else if let Some((method, attr)) = http_attr {
            let pathname_str = match &attr.args {
                Some(args) if args.len() == 1 => match &args[0] {
                    AttributeArg::String(s) => s.clone(),
                    AttributeArg::Identifier(_) => {
                        return Err(TypeError::InvalidAttribute {
                            message: format!(
                                "#[{}] attribute requires a string path argument, e.g., #[{}(\"/path\")]",
                                method.attr_name(),
                                method.attr_name()
                            ),
                        });
                    }
                },
                Some(_) => {
                    return Err(TypeError::InvalidAttribute {
                        message: format!(
                            "#[{}] attribute requires exactly one string path argument",
                            method.attr_name()
                        ),
                    });
                }
                None => {
                    return Err(TypeError::InvalidAttribute {
                        message: format!(
                            "#[{}] attribute requires a path argument, e.g., #[{}(\"/path\")]",
                            method.attr_name(),
                            method.attr_name()
                        ),
                    });
                }
            };

            let pathname =
                Pathname::new(&pathname_str).map_err(|e| TypeError::InvalidAttribute {
                    message: format!("#[{}] attribute: {}", method.attr_name(), e),
                })?;

            FunctionKind::Http(method, pathname)
        } else {
            FunctionKind::Regular
        }
    };

    // Validate: #[test] functions cannot have parameters
    if kind == FunctionKind::Test && !func.params.is_empty() {
        return Err(TypeError::InvalidAttribute {
            message: format!("#[test] function '{}' cannot have parameters", func.name),
        });
    }

    // Validate: HTTP route functions can have at most 1 parameter of type Request
    if let FunctionKind::Http(ref method, _) = kind {
        if func.params.len() > 1 {
            return Err(TypeError::InvalidAttribute {
                message: format!(
                    "#[{}] function '{}' can have at most 1 parameter (of type Request)",
                    method.attr_name(),
                    func.name
                ),
            });
        }
        if func.params.len() == 1 {
            let param_type = &typed_params[0].1;
            let resolved_param = ctx.resolve(param_type);
            let is_request = matches!(
                &resolved_param,
                Type::Struct { module, name, .. } if name == "Request" && module.to_string() == "std::http"
            );
            if !is_request {
                return Err(TypeError::InvalidAttribute {
                    message: format!(
                        "#[{}] function '{}' parameter must be of type Request (from std::http), but got {}",
                        method.attr_name(),
                        func.name,
                        resolved_param
                    ),
                });
            }
        }
    }

    if kind == FunctionKind::Builtin {
        // Validate: only allowed in std package
        if package_name != "std" {
            return Err(TypeError::InvalidAttribute {
                message: "the #[builtin] attribute can only be used in the standard library"
                    .to_string(),
            });
        }
        // Validate: must have explicit return type
        if func.return_type.is_none() {
            return Err(TypeError::InvalidAttribute {
                message: "#[builtin] functions must have an explicit return type".to_string(),
            });
        }
        // Validate: body must be unit `()`
        if func.body != Expr::Tuple(vec![]) {
            return Err(TypeError::InvalidAttribute {
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
            kind: FunctionKind::Builtin,
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
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("function '{}'", func.name),
                expected: ctx.resolve(&declared_return).to_string(),
                actual: body_type.to_string(),
                detail: e.to_string(),
            })?;
        ctx.resolve(&declared_return)
    } else {
        // Infer return type from body
        body_type
    };

    // Validate: #[test] functions must return (), Result, Task<()>, or Task<Result>
    if kind == FunctionKind::Test {
        let resolved = ctx.resolve(&return_type);
        let valid = match &resolved {
            Type::Tuple(elems) if elems.is_empty() => true,
            Type::Enum { name, .. } if name == "Result" => true,
            Type::Task(inner) => match inner.as_ref() {
                Type::Tuple(elems) if elems.is_empty() => true,
                Type::Enum { name, .. } if name == "Result" => true,
                _ => false,
            },
            _ => false,
        };
        if !valid {
            return Err(TypeError::InvalidAttribute {
                message: format!(
                    "#[test] function '{}' must return (), Result, Task<()>, or Task<Result>, but returns {}",
                    func.name, resolved
                ),
            });
        }
    }

    // Validate: #[job] functions must return (), Result<(), E>, Task<()>, or Task<Result<(), E>>
    if kind == FunctionKind::Job {
        let resolved = ctx.resolve(&return_type);
        let is_unit = |ty: &Type| matches!(ty, Type::Tuple(elems) if elems.is_empty());
        let is_result_unit = |ty: &Type| {
            matches!(ty, Type::Enum { name, type_args, .. }
                if name == "Result" && type_args.first().is_some_and(is_unit))
        };
        let valid = match &resolved {
            ty if is_unit(ty) => true,
            ty if is_result_unit(ty) => true,
            Type::Task(inner) if is_unit(inner) => true,
            Type::Task(inner) if is_result_unit(inner) => true,
            _ => false,
        };
        if !valid {
            return Err(TypeError::InvalidAttribute {
                message: format!(
                    "#[job] function '{}' must return (), Result<(), E>, Task<()>, or Task<Result<(), E>>, but returns {}",
                    func.name, resolved
                ),
            });
        }
    }

    // Validate: HTTP route functions must return Response
    if let FunctionKind::Http(ref method, _) = kind {
        let resolved = ctx.resolve(&return_type);
        let is_response = matches!(
            &resolved,
            Type::Struct { module, name, .. } if name == "Response" && module.to_string() == "std::http"
        );
        if !is_response {
            return Err(TypeError::InvalidAttribute {
                message: format!(
                    "#[{}] function '{}' must return Response (from std::http), but returns {}",
                    method.attr_name(),
                    func.name,
                    resolved
                ),
            });
        }
    }

    Ok(TypedFunction {
        name: func.name.clone(),
        params: typed_params,
        body: typed_body,
        return_type: ctx.resolve(&return_type),
        kind,
    })
}

/// Check an impl method body. Similar to check_function but handles self parameter
/// and Self type resolution.
fn check_impl_method_body(
    method: &ImplMethod,
    impl_block: &ImplBlock,
    type_qpath: &QualifiedPath,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
    package_name: &str,
) -> Result<TypedFunction, TypeError> {
    // Create type variables for impl type params and method type params
    let method_qpath = type_qpath.child(&method.name);
    let mut type_param_map = HashMap::new();
    if let Some(Definition::ImplMethod(stored_imt)) = env.definitions.get(&method_qpath) {
        // Reuse Phase 1 TypeVarIds for consistency with stored type_var_ids
        for (name, &id) in impl_block
            .type_params
            .iter()
            .zip(stored_imt.impl_type_var_ids.iter())
        {
            type_param_map.insert(name.clone(), id);
        }
    } else {
        // Fallback (e.g. tests that don't pre-register)
        for name in &impl_block.type_params {
            let var = ctx.fresh_var();
            if let Type::Var(id) = var {
                type_param_map.insert(name.clone(), id);
            }
        }
    }

    // Build the Self type from the target type annotation
    let self_type = resolve_type_annotation(
        &impl_block.target_type,
        &type_param_map,
        current_module,
        env,
    )?;

    // Add method's own type params
    if let Some(Definition::ImplMethod(stored_imt)) = env.definitions.get(&method_qpath) {
        for (name, &id) in method
            .type_params
            .iter()
            .zip(stored_imt.type_var_ids.iter())
        {
            type_param_map.insert(name.clone(), id);
        }
    } else {
        for name in &method.type_params {
            let var = ctx.fresh_var();
            if let Type::Var(id) = var {
                type_param_map.insert(name.clone(), id);
            }
        }
    }

    // Build local environment with parameters
    let mut locals = HashMap::new();
    let mut typed_params = Vec::new();

    // If has_self, add self to locals
    if method.has_self {
        locals.insert("self".to_string(), TypeScheme::mono(self_type.clone()));
        typed_params.push((
            zoya_ir::TypedPattern::Var {
                name: "self".to_string(),
                ty: self_type.clone(),
            },
            self_type.clone(),
        ));
    }

    for param in &method.params {
        check_irrefutable(&param.pattern).map_err(|msg| TypeError::RefutablePattern {
            context: "method parameter".to_string(),
            detail: msg,
        })?;

        let ty = resolve_type_annotation_with_self(
            &param.typ,
            &type_param_map,
            current_module,
            env,
            &self_type,
        )?;

        let (typed_pattern, bindings) =
            check_pattern(&param.pattern, &ty, current_module, env, ctx)?;

        for (name, var_ty) in bindings {
            locals.insert(name, TypeScheme::mono(var_ty));
        }

        typed_params.push((typed_pattern, ctx.resolve(&ty)));
    }

    // Check for #[builtin] attribute
    let is_builtin = method.attributes.iter().any(|a| a.name == "builtin");

    if is_builtin {
        if package_name != "std" {
            return Err(TypeError::InvalidAttribute {
                message: "the #[builtin] attribute can only be used in the standard library"
                    .to_string(),
            });
        }
        if method.return_type.is_none() {
            return Err(TypeError::InvalidAttribute {
                message: "#[builtin] methods must have an explicit return type".to_string(),
            });
        }
        if method.body != Expr::Tuple(vec![]) {
            return Err(TypeError::InvalidAttribute {
                message: "#[builtin] methods must have a unit body ()".to_string(),
            });
        }

        let declared_return = resolve_type_annotation_with_self(
            method.return_type.as_ref().unwrap(),
            &type_param_map,
            current_module,
            env,
            &self_type,
        )?;

        let body_env = env.with_locals(locals);
        let typed_body = check_expr(&method.body, current_module, &body_env, ctx)?;

        let method_name = format!("{}::{}", type_qpath.last(), method.name);

        return Ok(TypedFunction {
            name: method_name,
            params: typed_params,
            body: typed_body,
            return_type: ctx.resolve(&declared_return),
            kind: FunctionKind::Builtin,
        });
    }

    // Check the body
    let body_env = env.with_locals(locals);
    let typed_body = check_expr(&method.body, current_module, &body_env, ctx)?;
    let body_type = ctx.resolve(&typed_body.ty());

    // Determine return type
    let return_type = if let Some(ref annotation) = method.return_type {
        let declared_return = resolve_type_annotation_with_self(
            annotation,
            &type_param_map,
            current_module,
            env,
            &self_type,
        )?;
        ctx.unify(&body_type, &declared_return)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("method '{}' on '{}'", method.name, type_qpath.last()),
                expected: ctx.resolve(&declared_return).to_string(),
                actual: body_type.to_string(),
                detail: e.to_string(),
            })?;
        ctx.resolve(&declared_return)
    } else {
        body_type
    };

    // Use the qualified method name as the function name
    let method_name = format!("{}::{}", type_qpath.last(), method.name);

    Ok(TypedFunction {
        name: method_name,
        params: typed_params,
        body: typed_body,
        return_type: ctx.resolve(&return_type),
        kind: FunctionKind::Regular,
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
                return Err(TypeError::KindMisuse {
                    kind: "variable".to_string(),
                    name: name.to_string(),
                    problem: "cannot use turbofish".to_string(),
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
                return Err(TypeError::VariantMismatch {
                    variant: qualified_path.to_string(),
                    problem: "requires arguments".to_string(),
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
                    return Err(TypeError::TypeArgCount {
                        kind: "enum".to_string(),
                        name: enum_path.to_string(),
                        expected: enum_type.type_params.len(),
                        actual: type_args.len(),
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
                    module: enum_type.module.clone(),
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
                    module: struct_type.module.clone(),
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
            Err(TypeError::KindMisuse {
                kind: def.kind_name().to_string(),
                name: qualified_path.to_string(),
                problem: "cannot be used as a value".to_string(),
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
                return Err(TypeError::KindMisuse {
                    kind: "lambda".to_string(),
                    name: name.to_string(),
                    problem: "cannot use turbofish".to_string(),
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
            EnumVariantType::Unit => Err(TypeError::VariantMismatch {
                variant: qualified_path.to_string(),
                problem: "is a unit variant, doesn't take arguments".to_string(),
            }),
            EnumVariantType::Struct(_) => Err(TypeError::VariantMismatch {
                variant: qualified_path.to_string(),
                problem: "is a struct variant, use { } syntax".to_string(),
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
            def: Definition::ImplMethod(imt),
            qualified_path,
        } => {
            check_impl_method_path_call(path, &qualified_path, imt, args, current_module, env, ctx)
        }
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => Err(TypeError::KindMisuse {
            kind: def.kind_name().to_string(),
            name: qualified_path.to_string(),
            problem: "cannot be called".to_string(),
        }),
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
        return Err(TypeError::ArityMismatch {
            kind: "function".to_string(),
            name: func_name.to_string(),
            expected: func_type.params.len(),
            actual: args.len(),
            what: "arguments".to_string(),
        });
    }

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(ref type_args) = path.type_args {
        // Validate count matches type parameters
        if type_args.len() != func_type.type_params.len() {
            return Err(TypeError::TypeArgCount {
                kind: "function".to_string(),
                name: func_name.to_string(),
                expected: func_type.type_params.len(),
                actual: type_args.len(),
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
        let typed_arg = check_expr_with_expected(arg, Some(param_type), current_module, env, ctx)?;
        let arg_type = typed_arg.ty();

        // Unify argument type with parameter type
        ctx.unify(&arg_type, param_type)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("argument in call to '{}'", func_name),
                expected: ctx.resolve(param_type).to_string(),
                actual: ctx.resolve(&arg_type).to_string(),
                detail: e.to_string(),
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

/// Check an impl method or associated function call via path syntax (e.g., `Point::origin()`)
fn check_impl_method_path_call(
    path: &Path,
    qualified_path: &QualifiedPath,
    imt: &ImplMethodType,
    args: &[Expr],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let method_name = path
        .segments
        .last()
        .map(|s| s.as_str())
        .unwrap_or("<unknown>");

    // All type vars: impl type vars + method type vars
    let all_type_params: Vec<&str> = imt
        .impl_type_params
        .iter()
        .chain(imt.type_params.iter())
        .map(|s| s.as_str())
        .collect();
    let all_type_var_ids: Vec<TypeVarId> = imt
        .impl_type_var_ids
        .iter()
        .chain(imt.type_var_ids.iter())
        .copied()
        .collect();

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(ref type_args) = path.type_args {
        if type_args.len() != all_type_params.len() {
            return Err(TypeError::TypeArgCount {
                kind: "impl method".to_string(),
                name: method_name.to_string(),
                expected: all_type_params.len(),
                actual: type_args.len(),
            });
        }
        let resolved: Vec<Type> = type_args
            .iter()
            .map(|ann| resolve_type_annotation(ann, &HashMap::new(), current_module, env))
            .collect::<Result<_, _>>()?;
        all_type_var_ids
            .iter()
            .zip(resolved)
            .map(|(&id, ty)| (id, ty))
            .collect()
    } else {
        all_type_var_ids
            .iter()
            .map(|&id| (id, ctx.fresh_var()))
            .collect()
    };

    // For methods with self, the first param is self and should be provided as first arg
    // For associated functions, all params come from args
    let expected_args = if imt.has_self {
        // With self: first arg is the receiver
        imt.params.len()
    } else {
        imt.params.len()
    };

    if args.len() != expected_args {
        return Err(TypeError::ArityMismatch {
            kind: "impl method".to_string(),
            name: method_name.to_string(),
            expected: expected_args,
            actual: args.len(),
            what: "argument(s)".to_string(),
        });
    }

    let instantiated_params: Vec<Type> = imt
        .params
        .iter()
        .map(|t| substitute_type_vars(t, &instantiation))
        .collect();
    let instantiated_return = substitute_type_vars(&imt.return_type, &instantiation);

    let mut typed_args = Vec::new();
    for (arg, param_type) in args.iter().zip(instantiated_params.iter()) {
        let typed_arg = check_expr_with_expected(arg, Some(param_type), current_module, env, ctx)?;
        let arg_type = typed_arg.ty();

        ctx.unify(&arg_type, param_type)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("argument in call to '{}'", method_name),
                expected: ctx.resolve(param_type).to_string(),
                actual: ctx.resolve(&arg_type).to_string(),
                detail: e.to_string(),
            })?;

        typed_args.push(typed_arg);
    }

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
            ctx.unify(&func_ty, &func_type)
                .map_err(|e| TypeError::KindMisuse {
                    kind: "expression".to_string(),
                    name: name.to_string(),
                    problem: format!("cannot be called as a function: {}", e),
                })?;

            (param_types, ret_type)
        }
        _ => {
            return Err(TypeError::KindMisuse {
                kind: "expression".to_string(),
                name: name.to_string(),
                problem: format!("is not a function, has type {}", resolved),
            });
        }
    };

    // Check argument count
    if args.len() != params.len() {
        return Err(TypeError::ArityMismatch {
            kind: "function".to_string(),
            name: name.to_string(),
            expected: params.len(),
            actual: args.len(),
            what: "arguments".to_string(),
        });
    }

    // Type check arguments and unify with parameter types
    let mut typed_args = Vec::new();
    for (arg, param_type) in args.iter().zip(params.iter()) {
        let typed_arg = check_expr_with_expected(arg, Some(param_type), current_module, env, ctx)?;
        let arg_type = typed_arg.ty();

        ctx.unify(&arg_type, param_type)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("argument in call to '{}'", name),
                expected: ctx.resolve(param_type).to_string(),
                actual: ctx.resolve(&arg_type).to_string(),
                detail: e.to_string(),
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
            return Err(TypeError::VariantMismatch {
                variant: qualified_variant_path.to_string(),
                problem: "is a unit variant, doesn't take arguments".to_string(),
            });
        }
        EnumVariantType::Struct(_) => {
            return Err(TypeError::VariantMismatch {
                variant: qualified_variant_path.to_string(),
                problem: "is a struct variant, use { } syntax".to_string(),
            });
        }
    };

    if args.len() != expected_types.len() {
        return Err(TypeError::ArityMismatch {
            kind: "enum variant".to_string(),
            name: qualified_variant_path.to_string(),
            expected: expected_types.len(),
            actual: args.len(),
            what: "argument(s)".to_string(),
        });
    }

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
        // Validate count matches type parameters
        if type_args.len() != enum_type.type_params.len() {
            return Err(TypeError::TypeArgCount {
                kind: "enum".to_string(),
                name: enum_path.to_string(),
                expected: enum_type.type_params.len(),
                actual: type_args.len(),
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
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("enum variant {}", qualified_variant_path),
                expected: ctx.resolve(&expected_type).to_string(),
                actual: ctx.resolve(&actual_type).to_string(),
                detail: e.to_string(),
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
            module: enum_type.module.clone(),
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
        return Err(TypeError::ArityMismatch {
            kind: "tuple struct".to_string(),
            name: name.to_string(),
            expected: struct_type.fields.len(),
            actual: args.len(),
            what: "argument(s)".to_string(),
        });
    }

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
        if type_args.len() != struct_type.type_params.len() {
            return Err(TypeError::TypeArgCount {
                kind: "struct".to_string(),
                name: name.to_string(),
                expected: struct_type.type_params.len(),
                actual: type_args.len(),
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
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("tuple struct {}", name),
                expected: ctx.resolve(&expected_type).to_string(),
                actual: ctx.resolve(&actual_type).to_string(),
                detail: e.to_string(),
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
            module: struct_type.module.clone(),
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
                _ => Err(TypeError::InvalidOperatorType {
                    operator: "negation operator".to_string(),
                    expected_types: "numeric types".to_string(),
                    actual_type: resolved.to_string(),
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
        BinOp::Mod => "%",
        BinOp::Pow => "**",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
    };
    ctx.unify(&left_ty, &right_ty)
        .map_err(|e| TypeError::TypeMismatchIn {
            context: format!("binary operator '{}'", op_symbol),
            expected: ctx.resolve(&left_ty).to_string(),
            actual: ctx.resolve(&right_ty).to_string(),
            detail: e.to_string(),
        })?;

    let resolved_ty = ctx.resolve(&left_ty);

    // Determine result type based on operator
    let result_ty = match op {
        // Arithmetic operators: only work on numeric types, result has same type as operands
        // Type variables are allowed through (they may resolve to numeric types later via unification)
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow => {
            if !is_numeric_type(&resolved_ty) && !matches!(resolved_ty, Type::Var(_)) {
                return Err(TypeError::InvalidOperatorType {
                    operator: "arithmetic operators".to_string(),
                    expected_types: "numeric types".to_string(),
                    actual_type: resolved_ty.to_string(),
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
                return Err(TypeError::InvalidOperatorType {
                    operator: "ordering operators".to_string(),
                    expected_types: "numeric types".to_string(),
                    actual_type: resolved_ty.to_string(),
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
        return Err(TypeError::EmptyMatch);
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
                ctx.unify(ty, &arm_ty)
                    .map_err(|e| TypeError::TypeMismatchIn {
                        context: "match arms".to_string(),
                        expected: ctx.resolve(ty).to_string(),
                        actual: ctx.resolve(&arm_ty).to_string(),
                        detail: e.to_string(),
                    })?;
            }
        }

        typed_arms.push(typed_arm);
    }

    // Check exhaustiveness and usefulness for all types
    let resolved_scrutinee_ty = ctx.resolve(&scrutinee_ty);
    let def_lookup = zoya_ir::DefinitionLookup::from_definitions(&env.definitions);
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

    // Check impl methods (both user-defined and std primitive methods)
    if let Some((method_qpath, imt)) = find_impl_method(&receiver_ty, method, &env.definitions) {
        if !imt.has_self {
            return Err(TypeError::AssociatedFunctionAsMethod {
                name: method.to_string(),
                on_type: imt.target_type_name.clone(),
            });
        }

        // Build instantiation: map impl type vars to the receiver's actual type args
        let receiver_type_args = match &receiver_ty {
            Type::Struct { type_args, .. } | Type::Enum { type_args, .. } => type_args.clone(),
            Type::List(elem) => vec![*elem.clone()],
            Type::Set(elem) => vec![*elem.clone()],
            Type::Task(elem) => vec![*elem.clone()],
            Type::Dict(key, val) => vec![*key.clone(), *val.clone()],
            _ => vec![],
        };
        let mut instantiation: HashMap<TypeVarId, Type> = imt
            .impl_type_var_ids
            .iter()
            .zip(receiver_type_args.iter())
            .map(|(&id, ty)| (id, ty.clone()))
            .collect();

        // Fresh vars for method's own type params
        for &id in &imt.type_var_ids {
            instantiation.insert(id, ctx.fresh_var());
        }

        // The first param in imt.params is self — skip it for argument matching
        let method_params = &imt.params[1..]; // skip self
        if args.len() != method_params.len() {
            return Err(TypeError::ArityMismatch {
                kind: "method".to_string(),
                name: method.to_string(),
                expected: method_params.len(),
                actual: args.len(),
                what: "argument(s)".to_string(),
            });
        }

        // Instantiate self param and unify with receiver
        let self_param = substitute_type_vars(&imt.params[0], &instantiation);
        ctx.unify(&receiver_ty, &self_param)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("self in method '{}'", method),
                expected: ctx.resolve(&self_param).to_string(),
                actual: ctx.resolve(&receiver_ty).to_string(),
                detail: e.to_string(),
            })?;

        // Type check arguments
        let mut typed_args = Vec::new();
        for (arg, param) in args.iter().zip(method_params.iter()) {
            let instantiated = substitute_type_vars(param, &instantiation);
            let typed_arg =
                check_expr_with_expected(arg, Some(&instantiated), current_module, env, ctx)?;
            let arg_ty = typed_arg.ty();

            ctx.unify(&arg_ty, &instantiated)
                .map_err(|e| TypeError::TypeMismatchIn {
                    context: format!("argument in method '{}'", method),
                    expected: ctx.resolve(&instantiated).to_string(),
                    actual: ctx.resolve(&arg_ty).to_string(),
                    detail: e.to_string(),
                })?;

            typed_args.push(typed_arg);
        }

        let return_type = ctx.resolve(&substitute_type_vars(&imt.return_type, &instantiation));

        // Desugar to a regular Call with receiver prepended
        let mut all_args = vec![typed_receiver];
        all_args.extend(typed_args);

        return Ok(TypedExpr::Call {
            path: method_qpath,
            args: all_args,
            ty: return_type,
        });
    }

    Err(TypeError::UnboundMethod {
        method: method.to_string(),
        on_type: receiver_ty.to_string(),
    })
}

/// Find an impl method definition for a given receiver type and method name.
fn find_impl_method<'a>(
    receiver_ty: &Type,
    method: &str,
    definitions: &'a HashMap<QualifiedPath, Definition>,
) -> Option<(QualifiedPath, &'a ImplMethodType)> {
    // Get the module and type name from the receiver
    let (module, type_name) = match receiver_ty {
        Type::Struct { module, name, .. } | Type::Enum { module, name, .. } => {
            (module.clone(), name.as_str())
        }
        _ => {
            // For primitive types, look up their std impl methods
            if let Some((mod_name, prim_type_name)) = primitive_method_module(receiver_ty) {
                // Try root::<mod>::<Type>::<method> (inside std itself)
                let root_path = QualifiedPath::root()
                    .child(mod_name)
                    .child(prim_type_name)
                    .child(method);
                if let Some(Definition::ImplMethod(imt)) = definitions.get(&root_path) {
                    return Some((root_path, imt));
                }
                // Try std::<mod>::<Type>::<method> (when std is a dependency)
                let std_path = QualifiedPath::new(vec!["std".to_string()])
                    .child(mod_name)
                    .child(prim_type_name)
                    .child(method);
                if let Some(Definition::ImplMethod(imt)) = definitions.get(&std_path) {
                    return Some((std_path, imt));
                }
            }
            return None;
        }
    };

    // Build the qualified path directly: module::TypeName::method
    let type_qpath = module.child(type_name);
    let method_qpath = type_qpath.child(method);
    if let Some(Definition::ImplMethod(imt)) = definitions.get(&method_qpath) {
        return Some((method_qpath, imt));
    }

    None
}

/// Check a list literal expression
fn check_list_expr(
    elements: &[ListElement],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    if elements.is_empty() {
        let elem_ty = ctx.fresh_var();
        return Ok(TypedExpr::List {
            elements: vec![],
            ty: Type::List(Box::new(elem_ty)),
        });
    }

    let elem_ty = ctx.fresh_var();
    let list_ty = Type::List(Box::new(elem_ty.clone()));
    let mut typed_elements = Vec::with_capacity(elements.len());

    for element in elements {
        match element {
            ListElement::Item(expr) => {
                let typed = check_expr(expr, current_module, env, ctx)?;
                ctx.unify(&typed.ty(), &elem_ty)
                    .map_err(|e| TypeError::TypeMismatch {
                        expected: "matching list element types".to_string(),
                        actual: e.to_string(),
                    })?;
                typed_elements.push(TypedListElement::Item(typed));
            }
            ListElement::Spread(expr) => {
                let typed = check_expr(expr, current_module, env, ctx)?;
                ctx.unify(&typed.ty(), &list_ty)
                    .map_err(|e| TypeError::TypeMismatch {
                        expected: "List".to_string(),
                        actual: e.to_string(),
                    })?;
                typed_elements.push(TypedListElement::Spread(typed));
            }
        }
    }

    Ok(TypedExpr::List {
        elements: typed_elements,
        ty: Type::List(Box::new(ctx.resolve(&elem_ty))),
    })
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
    expected_type: Option<&Type>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let mut typed_params = Vec::new();
    let mut param_types = Vec::new();
    let mut lambda_env = env.clone();

    for param in params {
        // Check pattern is irrefutable
        check_irrefutable(&param.pattern).map_err(|msg| TypeError::RefutablePattern {
            context: "lambda parameter".to_string(),
            detail: msg,
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

    // Pre-unify unannotated parameter types with expected function type
    // to enable bidirectional type inference for lambda arguments
    if let Some(expected) = expected_type {
        let resolved_expected = ctx.resolve(expected);
        if let Type::Function {
            params: expected_params,
            ..
        } = &resolved_expected
        {
            for (param_ty, expected_param_ty) in param_types.iter().zip(expected_params.iter()) {
                if matches!(ctx.resolve(param_ty), Type::Var(_)) {
                    let _ = ctx.unify(param_ty, expected_param_ty);
                }
            }
        }
    }

    // Check the body in the extended environment
    let typed_body = check_expr(body, current_module, &lambda_env, ctx)?;
    let body_ty = typed_body.ty();

    // If return type is annotated, unify with body type
    let resolved_return = if let Some(annotation) = return_type {
        let declared_return =
            resolve_type_annotation(annotation, &HashMap::new(), current_module, env)?;
        ctx.unify(&body_ty, &declared_return)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: "lambda body".to_string(),
                expected: ctx.resolve(&declared_return).to_string(),
                actual: ctx.resolve(&body_ty).to_string(),
                detail: e.to_string(),
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
        } if struct_type.kind == StructTypeKind::Tuple => Err(TypeError::KindMisuse {
            kind: "tuple struct".to_string(),
            name: qualified_path.to_string(),
            problem: "must be constructed with () syntax, not {}".to_string(),
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
                    return Err(TypeError::InvalidAttribute {
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
            EnumVariantType::Unit => Err(TypeError::VariantMismatch {
                variant: qualified_path.to_string(),
                problem: "is a unit variant, doesn't take fields".to_string(),
            }),
            EnumVariantType::Tuple(_) => Err(TypeError::VariantMismatch {
                variant: qualified_path.to_string(),
                problem: "is a tuple variant, use ( ) syntax".to_string(),
            }),
        },
        ResolvedPath::Local { name, .. } => Err(TypeError::KindMisuse {
            kind: "variable".to_string(),
            name: name.to_string(),
            problem: "is not a struct".to_string(),
        }),
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => Err(TypeError::KindMisuse {
            kind: def.kind_name().to_string(),
            name: qualified_path.to_string(),
            problem: "cannot be constructed with struct syntax".to_string(),
        }),
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
            return Err(TypeError::TypeArgCount {
                kind: "struct".to_string(),
                name: name.to_string(),
                expected: struct_type.type_params.len(),
                actual: type_args.len(),
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
                return Err(TypeError::MissingField {
                    field: expected.to_string(),
                    context: format!("struct {}", name),
                });
            }
        }
    }

    // Check for extra/unknown fields (always check, even with spread)
    for provided in &provided_field_names {
        if !expected_field_names.contains(provided) {
            return Err(TypeError::UnknownField {
                field: provided.to_string(),
                context: format!("struct {}", name),
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
            .ok_or_else(|| TypeError::UnknownField {
                field: field_name.clone(),
                context: format!("struct {}", name),
            })?;

        // Substitute type variables to get the expected type for this instantiation
        let expected_type = substitute_type_vars(field_type, &instantiation);

        // Type-check the field expression
        let typed_expr = check_expr(field_expr, current_module, env, ctx)?;
        let actual_type = typed_expr.ty();

        // Unify with expected type
        ctx.unify(&actual_type, &expected_type)
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("field '{}' in struct {}", field_name, name),
                expected: ctx.resolve(&expected_type).to_string(),
                actual: ctx.resolve(&actual_type).to_string(),
                detail: e.to_string(),
            })?;

        typed_fields.push((field_name.clone(), typed_expr));
    }

    // Type-check spread expression if present
    let typed_spread = if let Some(spread_expr) = spread {
        let typed_spread_expr = check_expr(spread_expr, current_module, env, ctx)?;
        let spread_type = typed_spread_expr.ty();

        // Build the expected struct type for unification
        let expected_struct_type = Type::Struct {
            module: struct_type.module.clone(),
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
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!("spread expression in struct {}", name),
                expected: ctx.resolve(&expected_struct_type).to_string(),
                actual: ctx.resolve(&spread_type).to_string(),
                detail: e.to_string(),
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
            module: struct_type.module.clone(),
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
            return Err(TypeError::VariantMismatch {
                variant: qualified_variant_path.to_string(),
                problem: "is a unit variant, doesn't take fields".to_string(),
            });
        }
        EnumVariantType::Tuple(_) => {
            return Err(TypeError::VariantMismatch {
                variant: qualified_variant_path.to_string(),
                problem: "is a tuple variant, use ( ) syntax".to_string(),
            });
        }
    };

    // Handle explicit type arguments (turbofish) or create fresh type variables
    let instantiation: HashMap<TypeVarId, Type> = if let Some(type_args) = explicit_type_args {
        if type_args.len() != enum_type.type_params.len() {
            return Err(TypeError::TypeArgCount {
                kind: "enum".to_string(),
                name: enum_path.to_string(),
                expected: enum_type.type_params.len(),
                actual: type_args.len(),
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
            return Err(TypeError::MissingField {
                field: expected.to_string(),
                context: format!("enum variant {}", qualified_variant_path),
            });
        }
    }

    for provided in &provided_names {
        if !expected_names.contains(provided) {
            return Err(TypeError::UnknownField {
                field: provided.to_string(),
                context: format!("enum variant {}", qualified_variant_path),
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
            .map_err(|e| TypeError::TypeMismatchIn {
                context: format!(
                    "field '{}' in enum variant {}",
                    field_name, qualified_variant_path
                ),
                expected: ctx.resolve(&expected_type).to_string(),
                actual: ctx.resolve(&actual_type).to_string(),
                detail: e.to_string(),
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
            module: enum_type.module.clone(),
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
            module,
            name,
            fields: struct_fields,
            type_args,
        } => {
            // If fields are empty (recursive type stub), look up real fields from definitions
            let def_lookup = zoya_ir::DefinitionLookup::from_definitions(&env.definitions);
            let actual_fields =
                def_lookup.resolve_struct_fields(module, name, struct_fields, type_args);

            let (_, field_type) =
                actual_fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .ok_or_else(|| TypeError::UnknownField {
                        field: field.to_string(),
                        context: format!("struct {}", name),
                    })?;

            Ok(TypedExpr::FieldAccess {
                expr: Box::new(typed_expr),
                field: field.to_string(),
                ty: field_type.clone(),
            })
        }
        _ => Err(TypeError::UnknownField {
            field: field.to_string(),
            context: format!("non-struct type {}", expr_ty),
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
                return Err(TypeError::InvalidIndex {
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
            module,
            name,
            fields: struct_fields,
            type_args,
        } => {
            let def_lookup = zoya_ir::DefinitionLookup::from_definitions(&env.definitions);
            let actual_fields =
                def_lookup.resolve_struct_fields(module, name, struct_fields, type_args);

            let field_name = format!("${}", index);
            let (_, field_type) = actual_fields
                .iter()
                .find(|(n, _)| n == &field_name)
                .ok_or_else(|| TypeError::InvalidIndex {
                    message: format!("cannot use tuple index {} on struct {}", index, name),
                })?;

            Ok(TypedExpr::TupleIndex {
                expr: Box::new(typed_expr),
                index: index_usize,
                ty: field_type.clone(),
            })
        }
        _ => Err(TypeError::InvalidIndex {
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
            return Err(TypeError::InvalidIndex {
                message: format!("cannot index into non-list type {}", expr_ty),
            });
        }
    };

    // Type-check the index and verify it's Int
    let typed_index = check_expr(index, current_module, env, ctx)?;
    let index_ty = ctx.resolve(&typed_index.ty());
    ctx.unify(&index_ty, &Type::Int)
        .map_err(|_| TypeError::InvalidIndex {
            message: format!("list index must be Int, got {}", index_ty),
        })?;

    // Return type is Option<T>
    // Look up the Option definition to get its module path
    // Try both root:: (when type-checking std itself) and std:: (for user code)
    let find_option_module = |prefix: &str| -> Option<QualifiedPath> {
        let qpath = QualifiedPath::new(vec![prefix.into(), "option".into(), "Option".into()]);
        env.definitions
            .get(&qpath)
            .or_else(|| {
                env.reexports
                    .get(&qpath)
                    .and_then(|real| env.definitions.get(real))
            })
            .and_then(|def| match def {
                Definition::Enum(e) => Some(e.module.clone()),
                _ => None,
            })
    };
    let option_module = find_option_module("root")
        .or_else(|| find_option_module("std"))
        .unwrap_or_else(|| QualifiedPath::new(vec!["std".into(), "option".into()]));
    let option_ty = Type::Enum {
        module: option_module,
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

/// Check an expression with an optional expected type hint.
/// For lambda expressions, passes the expected type to enable bidirectional inference.
fn check_expr_with_expected(
    expr: &Expr,
    expected_type: Option<&Type>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    match (expr, expected_type) {
        (
            Expr::Lambda {
                params,
                return_type,
                body,
            },
            Some(expected),
        ) => check_lambda(
            params,
            return_type,
            body,
            Some(expected),
            current_module,
            env,
            ctx,
        ),
        _ => check_expr(expr, current_module, env, ctx),
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
        Expr::InterpolatedString(parts) => {
            check_interpolated_string(parts, current_module, env, ctx)
        }
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
        } => check_lambda(params, return_type, body, None, current_module, env, ctx),
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

fn check_interpolated_string(
    parts: &[StringPart],
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedExpr, TypeError> {
    let mut typed_parts = Vec::new();
    for part in parts {
        match part {
            StringPart::Literal(s) => {
                typed_parts.push(TypedStringPart::Literal(s.clone()));
            }
            StringPart::Expr(expr) => {
                let typed_expr = check_expr(expr, current_module, env, ctx)?;
                let ty = ctx.resolve(&typed_expr.ty());
                match &ty {
                    Type::String | Type::Int | Type::Float | Type::BigInt => {}
                    Type::Var(_) => {
                        return Err(TypeError::InvalidInterpolation {
                            message: "cannot interpolate expression of unknown type".to_string(),
                        });
                    }
                    _ => {
                        return Err(TypeError::InvalidInterpolation {
                            message: format!(
                                "cannot interpolate expression of type {ty}; only String, Int, Float, and BigInt can be interpolated"
                            ),
                        });
                    }
                }
                typed_parts.push(TypedStringPart::Expr(Box::new(typed_expr)));
            }
        }
    }
    Ok(TypedExpr::InterpolatedString(typed_parts))
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
        module: enum_type.module.clone(),
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
        module: struct_type.module.clone(),
        name: struct_type.name.clone(),
        type_args,
        fields: resolved_fields,
    };
    (instantiation, ty)
}

/// Check if an expression is a syntactic value (safe to generalize under value restriction)
fn is_syntactic_value(expr: &Expr) -> bool {
    match expr {
        Expr::Lambda { .. } => true,
        Expr::Int(_) | Expr::BigInt(_) | Expr::Float(_) | Expr::Bool(_) | Expr::String(_) => true,
        Expr::InterpolatedString(_) => false,
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
        Definition::ImplMethod(m) => Definition::ImplMethod(ImplMethodType {
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
                                    return Err(TypeError::UnboundPath {
                                        path: format!(
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
                                    return Err(TypeError::UnboundPath {
                                        path: format!(
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
    let def = env
        .definitions
        .get(qualified)
        .ok_or_else(|| TypeError::UnboundImport {
            name: qualified.to_string(),
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
        return Err(TypeError::PrivateReExport {
            name: qualified.to_string(),
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

/// Compute the first safe TypeVarId counter value to avoid collisions with dep definitions.
fn max_type_var_id_in_deps(deps: &[&CheckedPackage]) -> usize {
    let mut max_id: usize = 0;
    for dep in deps {
        for def in dep.definitions.values() {
            let ids: &[&[TypeVarId]] = match def {
                Definition::Function(f) => &[&f.type_var_ids],
                Definition::Struct(s) => &[&s.type_var_ids],
                Definition::Enum(e) => &[&e.type_var_ids],
                Definition::EnumVariant(e, _) => &[&e.type_var_ids],
                Definition::TypeAlias(t) => &[&t.type_var_ids],
                Definition::ImplMethod(m) => &[&m.impl_type_var_ids, &m.type_var_ids],
                Definition::Module(_) => &[],
            };
            for group in ids {
                for id in *group {
                    if id.0 >= max_id {
                        max_id = id.0 + 1;
                    }
                }
            }
        }
    }
    max_id
}

/// Check an entire module tree, returning a checked module tree.
///
/// This performs multi-module type checking:
/// 1. Register all declarations from all modules
/// 2. Type-check all function bodies with module context for path resolution
pub fn check(pkg: &Package, deps: &[&CheckedPackage]) -> Result<CheckedPackage, TypeError> {
    let mut env = TypeEnv::default();

    // Compute the starting TypeVarId counter to avoid collisions with dep definitions.
    // Dependencies contain TypeVarIds in their type definitions (e.g., Option<T> has T = TypeVarId(N)).
    // If our fresh variables start from 0, they can collide with these dep TypeVarIds during
    // instantiation and unification, causing spurious type errors.
    let start = max_type_var_id_in_deps(deps);
    let mut ctx = UnifyCtx::with_start(start);

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

    // Phase 0.7: Validate #[test], #[job], and HTTP method attribute usage
    // These are only valid on function definitions
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            for item in &module.items {
                let (attrs, kind) = match item {
                    Item::Function(_) => continue,
                    Item::Struct(s) => (&s.attributes, "struct"),
                    Item::Enum(e) => (&e.attributes, "enum"),
                    Item::TypeAlias(t) => (&t.attributes, "type alias"),
                    Item::Use(u) => (&u.attributes, "use"),
                    Item::Impl(i) => (&i.attributes, "impl"),
                    Item::ModDecl(_) => unreachable!("mod decls are removed by the loader"),
                };
                for attr_name in &["test", "job", "get", "post", "put", "patch", "delete"] {
                    if attrs.iter().any(|a| a.name == *attr_name) {
                        return Err(TypeError::InvalidAttribute {
                            message: format!(
                                "#[{}] is only valid on functions, not on {} definitions",
                                attr_name, kind
                            ),
                        });
                    }
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

    // Early import pass (pre-types): resolve internal imports that target
    // type names registered in Pass 1.  This ensures that cross-module type
    // references (e.g., `use super::json::JSON`) are available when enum
    // variants and struct fields are resolved in Pass 2.
    // Imports that fail (e.g., targeting not-yet-registered items) are
    // silently skipped — they will be retried below.
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            for item in &module.items {
                if let Item::Use(u) = item
                    && u.visibility == zoya_ast::Visibility::Private
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

    // Pass 2: Resolve struct fields, enum variants, type aliases
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            resolve_type_definitions(&module.items, path, &mut env, &mut ctx)?;
        }
    }

    // Early import pass (post-types): resolve internal imports that target
    // fully resolved types.  This runs before function signatures are
    // registered (Pass 3) so that `use super::result::Result` etc. are
    // available in type annotations.  Each use is resolved individually;
    // imports that fail (e.g., targeting not-yet-registered functions) are
    // silently skipped — they will be resolved in the full import pass
    // after Pass 3.
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

    // Pass 3b: Register impl method signatures
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            register_impl_methods(&module.items, path, &mut env, &mut ctx, &pkg.name)?;
        }
    }

    // Pass 3c: Synthesize Job enum and enqueue function for packages with #[job] functions
    // Each entry: (variant_name, qualified_path_string, param_types)
    let mut job_variants: Vec<(String, String, Vec<Type>)> = Vec::new();
    for path in &module_paths {
        if let Some(module) = pkg.modules.get(path) {
            for item in &module.items {
                if let Item::Function(func) = item
                    && func.attributes.iter().any(|a| a.name == "job")
                {
                    let func_path = path.child(&func.name);
                    if let Some(Definition::Function(ft)) = env.definitions.get(&func_path) {
                        // Build variant name: join path segments after "root" with "_",
                        // then PascalCase the result.
                        // E.g. root::tasks::send_email → "tasks_send_email" → "TasksSendEmail"
                        let segments = func_path.segments();
                        let name_segments: Vec<&str> =
                            segments.iter().skip(1).map(|s| s.as_str()).collect();
                        let variant_name = to_pascal_case(&name_segments.join("_"));
                        job_variants.push((
                            variant_name,
                            func_path.to_string(),
                            ft.params.clone(),
                        ));
                    }
                }
            }
        }
    }

    // Check for duplicate variant names (e.g. root::jobs::foo vs root::jobs_foo both → JobsFoo)
    {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for (variant_name, func_path, _) in &job_variants {
            if let Some(existing_path) = seen.get(variant_name.as_str()) {
                return Err(TypeError::InvalidAttribute {
                    message: format!(
                        "job variant name collision: '{}' and '{}' both produce Job::{}",
                        existing_path, func_path, variant_name
                    ),
                });
            }
            seen.insert(variant_name, func_path);
        }
    }

    // Store synthesized job type for later use (synthetic TypedFunction)
    let synthesized_job_type: Option<Type> = if !job_variants.is_empty() {
        let job_enum_path = QualifiedPath::root().child("Job");
        let variants: Vec<(String, EnumVariantType)> = job_variants
            .iter()
            .map(|(name, _, params)| {
                if params.is_empty() {
                    (name.clone(), EnumVariantType::Unit)
                } else {
                    (name.clone(), EnumVariantType::Tuple(params.clone()))
                }
            })
            .collect();

        let job_enum_type = EnumType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            name: "Job".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            variants: variants.clone(),
        };

        // Register the enum and its variants
        env.register(
            job_enum_path.clone(),
            Definition::Enum(job_enum_type.clone()),
        );
        for (variant_name, variant_type) in &job_enum_type.variants {
            env.register(
                job_enum_path.child(variant_name),
                Definition::EnumVariant(job_enum_type.clone(), variant_type.clone()),
            );
        }

        // Build the Job type for the enqueue param
        let job_type = Type::Enum {
            module: QualifiedPath::root(),
            name: "Job".to_string(),
            type_args: vec![],
            variants,
        };

        // Register enqueue function
        let enqueue_path = QualifiedPath::root().child("enqueue");
        env.register(
            enqueue_path.clone(),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![job_type.clone()],
                return_type: Type::Tuple(vec![]),
            }),
        );

        // Inject Job, its variants, and enqueue into every module's imports
        for path in &module_paths {
            let module_imports = env.imports.entry(path.clone()).or_default();
            module_imports
                .entry("Job".to_string())
                .or_insert_with(|| job_enum_path.clone());
            for (variant_name, _) in &job_enum_type.variants {
                module_imports
                    .entry(variant_name.clone())
                    .or_insert_with(|| job_enum_path.child(variant_name));
            }
            module_imports
                .entry("enqueue".to_string())
                .or_insert_with(|| enqueue_path.clone());
        }

        Some(job_type)
    } else {
        None
    };

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
            for (func_path, func) in functions {
                checked_items.insert(func_path, func);
            }
        }
    }

    // Add synthetic enqueue builtin function if Job enum was synthesized
    if let Some(job_type) = synthesized_job_type {
        let enqueue_path = QualifiedPath::root().child("enqueue");
        let unit_type = Type::Tuple(vec![]);
        checked_items.insert(
            enqueue_path,
            TypedFunction {
                name: "enqueue".to_string(),
                kind: FunctionKind::Builtin,
                params: vec![(
                    TypedPattern::Var {
                        name: "job".to_string(),
                        ty: job_type.clone(),
                    },
                    job_type,
                )],
                return_type: unit_type.clone(),
                body: TypedExpr::Tuple {
                    elements: vec![],
                    ty: unit_type,
                },
            },
        );
    }

    let external_definitions = env
        .definitions
        .iter()
        .filter(|(path, def)| {
            is_externally_visible(path, def, &env.definitions)
                || checked_items.get(path).is_some_and(|f| {
                    matches!(
                        f.kind,
                        FunctionKind::Test | FunctionKind::Job | FunctionKind::Http(_, _)
                    )
                })
        })
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
        items: checked_items,
        definitions: external_definitions,
        reexports: external_reexports,
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
    // Register type aliases first so they are available in struct fields
    // and enum variants within the same module.
    for item in items {
        if let Item::TypeAlias(def) = item {
            let qualified_path = current_module.child(&def.name);
            let alias_type = type_alias_from_def(def, current_module, env, ctx)?;
            env.register(qualified_path, Definition::TypeAlias(alias_type));
        }
    }

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

/// Pass 3b: Register impl method signatures.
/// For each impl block, resolve the target type, then register each method
/// as a Definition::ImplMethod under the type's qualified path.
fn register_impl_methods(
    items: &[Item],
    current_module: &QualifiedPath,
    env: &mut TypeEnv,
    ctx: &mut UnifyCtx,
    package_name: &str,
) -> Result<(), TypeError> {
    for item in items {
        let Item::Impl(impl_block) = item else {
            continue;
        };

        // Resolve the target type to find its qualified path
        let (type_qpath, _type_def) =
            resolve_impl_target(&impl_block.target_type, current_module, env, package_name)?;

        // For primitive type impls (in std), register a Module definition at the type path
        // so is_externally_visible can traverse the ancestor chain
        if !env.definitions.contains_key(&type_qpath) {
            env.register(
                type_qpath.clone(),
                Definition::Module(ModuleType {
                    visibility: Visibility::Public,
                    module: current_module.clone(),
                    name: type_qpath.last().to_string(),
                }),
            );
        }

        // Create fresh type variables for impl type params
        let mut impl_type_param_map = HashMap::new();
        let mut impl_type_var_ids = Vec::new();
        for name in &impl_block.type_params {
            if !is_pascal_case(name) {
                return Err(TypeError::NamingConvention {
                    kind: "type parameter".to_string(),
                    name: name.clone(),
                    convention: "PascalCase".to_string(),
                    suggestion: to_pascal_case(name),
                });
            }
            let var = ctx.fresh_var();
            if let Type::Var(id) = var {
                impl_type_param_map.insert(name.clone(), id);
                impl_type_var_ids.push(id);
            }
        }

        // Build the Self type from the target type annotation
        let self_type = resolve_type_annotation_with_self(
            &impl_block.target_type,
            &impl_type_param_map,
            current_module,
            env,
            // Self is not yet available — but target type won't contain Self
            &Type::Tuple(vec![]),
        )?;

        let target_type_name = type_qpath.last().to_string();

        for method in &impl_block.methods {
            // Validate method name
            if !is_snake_case(&method.name) {
                return Err(TypeError::NamingConvention {
                    kind: "method name".to_string(),
                    name: method.name.clone(),
                    convention: "snake_case".to_string(),
                    suggestion: to_snake_case(&method.name),
                });
            }

            let method_qpath = type_qpath.child(&method.name);

            // Check for duplicate
            if env.definitions.contains_key(&method_qpath) {
                return Err(TypeError::DuplicateDefinition {
                    name: method.name.clone(),
                    on_type: target_type_name.clone(),
                });
            }

            // Create fresh type vars for method's own type params
            let mut method_type_param_map = impl_type_param_map.clone();
            let mut method_type_var_ids = Vec::new();
            for name in &method.type_params {
                if !is_pascal_case(name) {
                    return Err(TypeError::NamingConvention {
                        kind: "type parameter".to_string(),
                        name: name.clone(),
                        convention: "PascalCase".to_string(),
                        suggestion: to_pascal_case(name),
                    });
                }
                let var = ctx.fresh_var();
                if let Type::Var(id) = var {
                    method_type_param_map.insert(name.clone(), id);
                    method_type_var_ids.push(id);
                }
            }

            // Build parameter types
            let mut param_types = Vec::new();

            // If has_self, first param is the Self type
            if method.has_self {
                param_types.push(self_type.clone());
            }

            // Resolve explicit params with Self available
            for param in &method.params {
                let ty = resolve_type_annotation_with_self(
                    &param.typ,
                    &method_type_param_map,
                    current_module,
                    env,
                    &self_type,
                )?;
                param_types.push(ty);
            }

            // Resolve return type with Self available
            let return_type = if let Some(ref annotation) = method.return_type {
                resolve_type_annotation_with_self(
                    annotation,
                    &method_type_param_map,
                    current_module,
                    env,
                    &self_type,
                )?
            } else {
                ctx.fresh_var()
            };

            env.register(
                method_qpath,
                Definition::ImplMethod(ImplMethodType {
                    visibility: method.visibility,
                    module: current_module.clone(),
                    target_type_name: target_type_name.clone(),
                    impl_type_params: impl_block.type_params.clone(),
                    impl_type_var_ids: impl_type_var_ids.clone(),
                    has_self: method.has_self,
                    type_params: method.type_params.clone(),
                    type_var_ids: method_type_var_ids,
                    params: param_types,
                    return_type,
                }),
            );
        }
    }
    Ok(())
}

/// Resolve an impl block's target type to a (QualifiedPath, Definition) pair.
/// The target must be a struct or enum defined in the current package.
fn resolve_impl_target(
    target_type: &TypeAnnotation,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    package_name: &str,
) -> Result<(QualifiedPath, Definition), TypeError> {
    // Extract the path from the type annotation
    let path = match target_type {
        TypeAnnotation::Named(path) => path,
        TypeAnnotation::Parameterized(path, _) => path,
        _ => {
            return Err(TypeError::InvalidImpl {
                message: "impl target must be a named type".to_string(),
            });
        }
    };

    let type_name = path
        .segments
        .last()
        .map(|s| s.as_str())
        .unwrap_or("<unknown>");

    // Reject primitive types — unless we're in the std package
    if path.segments.len() == 1
        && matches!(
            type_name,
            "Int" | "BigInt" | "Float" | "Bool" | "String" | "List" | "Set" | "Dict" | "Task"
        )
    {
        if package_name == "std" && primitive_module_for_name(type_name).is_some() {
            // In std, primitive impl blocks resolve to root::<mod>::<Type>
            let qpath = current_module.child(type_name);
            // Return a dummy definition — callers ignore it
            return Ok((
                qpath,
                Definition::Module(ModuleType {
                    visibility: Visibility::Public,
                    module: current_module.clone(),
                    name: type_name.to_string(),
                }),
            ));
        }
        return Err(TypeError::InvalidImpl {
            message: format!("cannot define impl for primitive type '{}'", type_name),
        });
    }

    // Resolve the path to find the definition
    let resolved = resolution::resolve_pattern_path(
        path,
        current_module,
        &env.imports,
        &env.definitions,
        &env.reexports,
    )?;

    match resolved {
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => {
            // Verify it's a struct or enum
            match &def {
                Definition::Struct(_) | Definition::Enum(_) => {}
                _ => {
                    return Err(TypeError::InvalidImpl {
                        message: format!(
                            "cannot define impl for {} '{}'",
                            def.kind_name(),
                            qualified_path
                        ),
                    });
                }
            }

            // Orphan rule: type must be in the current package (path starts with "root::")
            let segments = qualified_path.segments();
            if !segments.is_empty() && segments[0] != "root" {
                // Type is from a dependency package
                let dep_name = &segments[0];
                return Err(TypeError::InvalidImpl {
                    message: format!(
                        "cannot define impl for type '{}' from package '{}' (orphan rule)",
                        type_name, dep_name
                    ),
                });
            }

            // Suppress unused variable warning
            let _ = package_name;

            Ok((qualified_path, def.clone()))
        }
        ResolvedPath::Local { name, .. } => Err(TypeError::KindMisuse {
            kind: "variable".to_string(),
            name: name.to_string(),
            problem: "is not a type".to_string(),
        }),
    }
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
) -> Result<Vec<(QualifiedPath, TypedFunction)>, TypeError> {
    let mut checked_items = Vec::new();

    for item in items {
        match item {
            Item::Function(func) => {
                // Clear substitutions before each function body check to prevent
                // type variable bindings from one function leaking into another.
                ctx.clear_substitutions();
                let typed = check_function_in_module(func, current_module, env, ctx, package_name)?;
                let func_path = current_module.child(&func.name);
                if let Some(Definition::Function(ft)) = env.definitions.get_mut(&func_path) {
                    ft.return_type = typed.return_type.clone();
                }
                checked_items.push((func_path, typed));
            }
            Item::Impl(impl_block) => {
                let (type_qpath, _) = resolve_impl_target(
                    &impl_block.target_type,
                    current_module,
                    env,
                    package_name,
                )?;

                for method in &impl_block.methods {
                    // Clear substitutions before each method body check
                    ctx.clear_substitutions();
                    let method_qpath = type_qpath.child(&method.name);
                    let typed = check_impl_method_body(
                        method,
                        impl_block,
                        &type_qpath,
                        current_module,
                        env,
                        ctx,
                        package_name,
                    )?;
                    // Update the definition's return type
                    if let Some(Definition::ImplMethod(imt)) =
                        env.definitions.get_mut(&method_qpath)
                    {
                        imt.return_type = typed.return_type.clone();
                    }
                    checked_items.push((method_qpath, typed));
                }
            }
            _ => {}
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
