use std::collections::{HashMap, HashSet};

use zoya_ast::{LetBinding, ListPattern, MatchArm, Path, Pattern, TuplePattern};
use zoya_ir::{
    Definition, EnumVariantType, QualifiedPath, Type, TypeError, TypeScheme, TypeVarId,
    TypedLetBinding, TypedMatchArm, TypedPattern,
};
use zoya_module::ModulePath;

use crate::check::{TypeEnv, check_expr, substitute_type_vars, substitute_variant_type_vars};
use crate::naming::{is_snake_case, to_snake_case};
use crate::resolution::{self, ResolvedPath};
use crate::type_resolver::resolve_type_annotation;
use crate::unify::UnifyCtx;

/// Check a list of patterns against a single element type (for list patterns)
pub fn check_patterns_against_elem(
    patterns: &[Pattern],
    elem_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(Vec<TypedPattern>, HashMap<String, Type>), TypeError> {
    let mut typed_patterns = Vec::new();
    let mut all_bindings = HashMap::new();
    for pat in patterns {
        let (typed_pat, bindings) = check_pattern(pat, elem_ty, current_module, env, ctx)?;
        typed_patterns.push(typed_pat);
        all_bindings.extend(bindings);
    }
    Ok((typed_patterns, all_bindings))
}

/// Check a list of patterns against corresponding types (for tuple patterns)
pub fn check_patterns_against_types(
    patterns: &[Pattern],
    types: &[Type],
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(Vec<TypedPattern>, HashMap<String, Type>), TypeError> {
    let mut typed_patterns = Vec::new();
    let mut all_bindings = HashMap::new();
    for (pat, ty) in patterns.iter().zip(types.iter()) {
        let (typed_pat, bindings) = check_pattern(pat, ty, current_module, env, ctx)?;
        typed_patterns.push(typed_pat);
        all_bindings.extend(bindings);
    }
    Ok((typed_patterns, all_bindings))
}

/// Check a pattern and return typed pattern with any bindings it introduces
pub fn check_pattern(
    pattern: &Pattern,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    match pattern {
        Pattern::Literal(expr) => {
            let typed = check_expr(expr, current_module, env, ctx)?;
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
            // Check variable name is snake_case
            if !is_snake_case(name) {
                return Err(TypeError {
                    message: format!(
                        "variable '{}' should be snake_case (e.g., '{}')",
                        name,
                        to_snake_case(name)
                    ),
                });
            }
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
                    let (typed_patterns, bindings) = check_patterns_against_elem(
                        patterns,
                        &resolved_elem,
                        current_module,
                        env,
                        ctx,
                    )?;
                    Ok((
                        TypedPattern::ListExact {
                            patterns: typed_patterns,
                            len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::Prefix {
                    patterns,
                    rest_binding,
                } => {
                    let (typed_patterns, mut bindings) = check_patterns_against_elem(
                        patterns,
                        &resolved_elem,
                        current_module,
                        env,
                        ctx,
                    )?;

                    // Handle rest binding: rest @ .. binds to List<T>
                    let rest_binding_with_type = if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_ty = Type::List(Box::new(resolved_elem.clone()));
                        bindings.insert(name.clone(), rest_ty.clone());
                        Some((name.clone(), rest_ty))
                    } else {
                        None
                    };

                    Ok((
                        TypedPattern::ListPrefix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding_with_type,
                            min_len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::Suffix {
                    patterns,
                    rest_binding,
                } => {
                    let (typed_patterns, mut bindings) = check_patterns_against_elem(
                        patterns,
                        &resolved_elem,
                        current_module,
                        env,
                        ctx,
                    )?;

                    // Handle rest binding
                    let rest_binding_with_type = if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_ty = Type::List(Box::new(resolved_elem.clone()));
                        bindings.insert(name.clone(), rest_ty.clone());
                        Some((name.clone(), rest_ty))
                    } else {
                        None
                    };

                    Ok((
                        TypedPattern::ListSuffix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding_with_type,
                            min_len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::PrefixSuffix {
                    prefix,
                    suffix,
                    rest_binding,
                } => {
                    let (prefix_typed, mut bindings) = check_patterns_against_elem(
                        prefix,
                        &resolved_elem,
                        current_module,
                        env,
                        ctx,
                    )?;
                    let (suffix_typed, suffix_bindings) = check_patterns_against_elem(
                        suffix,
                        &resolved_elem,
                        current_module,
                        env,
                        ctx,
                    )?;
                    bindings.extend(suffix_bindings);

                    // Handle rest binding
                    let rest_binding_with_type = if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_ty = Type::List(Box::new(resolved_elem.clone()));
                        bindings.insert(name.clone(), rest_ty.clone());
                        Some((name.clone(), rest_ty))
                    } else {
                        None
                    };

                    Ok((
                        TypedPattern::ListPrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            rest_binding: rest_binding_with_type,
                            min_len: prefix.len() + suffix.len(),
                        },
                        bindings,
                    ))
                }
            }
        }

        Pattern::Tuple(tuple_pattern) => {
            // Get the tuple element types from scrutinee, or infer from pattern
            let resolved = ctx.resolve(scrutinee_ty);
            let tuple_types = match &resolved {
                Type::Tuple(types) => types.clone(),
                Type::Var(_) => {
                    // Type inference: infer tuple type from pattern
                    match tuple_pattern {
                        TuplePattern::Empty => {
                            // Unify with empty tuple
                            ctx.unify(scrutinee_ty, &Type::Tuple(vec![])).map_err(|e| {
                                TypeError {
                                    message: format!(
                                        "tuple pattern cannot match type {}: {}",
                                        ctx.resolve(scrutinee_ty),
                                        e.message
                                    ),
                                }
                            })?;
                            vec![]
                        }
                        TuplePattern::Exact(patterns) => {
                            // Create fresh type vars for each element and unify
                            let elem_types: Vec<Type> =
                                (0..patterns.len()).map(|_| ctx.fresh_var()).collect();
                            ctx.unify(scrutinee_ty, &Type::Tuple(elem_types.clone()))
                                .map_err(|e| TypeError {
                                    message: format!(
                                        "tuple pattern cannot match type {}: {}",
                                        ctx.resolve(scrutinee_ty),
                                        e.message
                                    ),
                                })?;
                            elem_types
                        }
                        TuplePattern::Prefix { .. }
                        | TuplePattern::Suffix { .. }
                        | TuplePattern::PrefixSuffix { .. } => {
                            // Can't infer tuple size from rest patterns
                            return Err(TypeError {
                                message:
                                    "cannot infer tuple type for pattern with '..' - add a type annotation".to_string(),
                            });
                        }
                    }
                }
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

                    let (typed_patterns, bindings) = check_patterns_against_types(
                        patterns,
                        &tuple_types,
                        current_module,
                        env,
                        ctx,
                    )?;
                    Ok((
                        TypedPattern::TupleExact {
                            patterns: typed_patterns,
                            len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                TuplePattern::Prefix {
                    patterns,
                    rest_binding,
                } => {
                    if patterns.len() > tuple_types.len() {
                        return Err(TypeError {
                            message: format!(
                                "tuple pattern has {} prefix elements but tuple has only {} elements",
                                patterns.len(),
                                tuple_types.len()
                            ),
                        });
                    }

                    let (typed_patterns, mut bindings) = check_patterns_against_types(
                        patterns,
                        &tuple_types,
                        current_module,
                        env,
                        ctx,
                    )?;

                    // Handle rest binding: rest @ .. binds to tuple of remaining elements
                    let rest_binding_with_type = if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_types: Vec<Type> = tuple_types[patterns.len()..].to_vec();
                        let rest_ty = Type::Tuple(rest_types);
                        bindings.insert(name.clone(), rest_ty.clone());
                        Some((name.clone(), rest_ty))
                    } else {
                        None
                    };

                    Ok((
                        TypedPattern::TuplePrefix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding_with_type,
                            total_len: tuple_types.len(),
                        },
                        bindings,
                    ))
                }

                TuplePattern::Suffix {
                    patterns,
                    rest_binding,
                } => {
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
                    let (typed_patterns, mut bindings) = check_patterns_against_types(
                        patterns,
                        &tuple_types[start_idx..],
                        current_module,
                        env,
                        ctx,
                    )?;

                    // Handle rest binding: rest @ .. binds to tuple of leading elements
                    let rest_binding_with_type = if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_types: Vec<Type> = tuple_types[..start_idx].to_vec();
                        let rest_ty = Type::Tuple(rest_types);
                        bindings.insert(name.clone(), rest_ty.clone());
                        Some((name.clone(), rest_ty))
                    } else {
                        None
                    };

                    Ok((
                        TypedPattern::TupleSuffix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding_with_type,
                            total_len: tuple_types.len(),
                        },
                        bindings,
                    ))
                }

                TuplePattern::PrefixSuffix {
                    prefix,
                    suffix,
                    rest_binding,
                } => {
                    let total_patterns = prefix.len() + suffix.len();
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
                    let (prefix_typed, mut bindings) = check_patterns_against_types(
                        prefix,
                        &tuple_types,
                        current_module,
                        env,
                        ctx,
                    )?;

                    // Suffix patterns match from the end
                    let suffix_start = tuple_types.len() - suffix.len();
                    let (suffix_typed, suffix_bindings) = check_patterns_against_types(
                        suffix,
                        &tuple_types[suffix_start..],
                        current_module,
                        env,
                        ctx,
                    )?;
                    bindings.extend(suffix_bindings);

                    // Handle rest binding: rest @ .. binds to tuple of middle elements
                    let rest_binding_with_type = if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_types: Vec<Type> =
                            tuple_types[prefix.len()..suffix_start].to_vec();
                        let rest_ty = Type::Tuple(rest_types);
                        bindings.insert(name.clone(), rest_ty.clone());
                        Some((name.clone(), rest_ty))
                    } else {
                        None
                    };

                    Ok((
                        TypedPattern::TuplePrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            rest_binding: rest_binding_with_type,
                            total_len: tuple_types.len(),
                        },
                        bindings,
                    ))
                }
            }
        }

        // Path pattern: Option::None, root::Color::Red (unit enum variants)
        Pattern::Path(path) => check_path_pattern(path, scrutinee_ty, current_module, env, ctx),

        // Call pattern: Option::Some(x), root::Result::Ok(v) (tuple enum variants)
        Pattern::Call { path, args } => {
            check_call_pattern(path, args, scrutinee_ty, current_module, env, ctx)
        }

        // Struct pattern: Point { x }, Message::Move { x, .. }
        // Works for both struct types and enum struct variants
        Pattern::Struct {
            path,
            fields,
            is_partial,
        } => check_struct_pattern(
            path,
            fields,
            *is_partial,
            scrutinee_ty,
            current_module,
            env,
            ctx,
        ),

        Pattern::As { name, pattern } => {
            // Check variable name is snake_case
            if !is_snake_case(name) {
                return Err(TypeError {
                    message: format!(
                        "variable '{}' should be snake_case (e.g., '{}')",
                        name,
                        to_snake_case(name)
                    ),
                });
            }

            // Recursively check the inner pattern
            let (typed_pattern, mut bindings) =
                check_pattern(pattern, scrutinee_ty, current_module, env, ctx)?;

            // Add binding for the entire matched value
            let resolved_ty = ctx.resolve(scrutinee_ty);
            bindings.insert(name.clone(), resolved_ty.clone());

            Ok((
                TypedPattern::As {
                    name: name.clone(),
                    ty: resolved_ty,
                    pattern: Box::new(typed_pattern),
                },
                bindings,
            ))
        }
    }
}

/// Check a path pattern (unit enum variant): Option::None, root::Color::Red
fn check_path_pattern(
    path: &Path,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    let resolved = resolution::resolve_pattern_path(path, current_module, &env.definitions)?;

    match resolved {
        ResolvedPath::Definition {
            def: Definition::EnumVariant(enum_type, variant_type),
            qualified_path,
        } => {
            // Must be a unit variant when used as a bare path pattern
            if !matches!(variant_type, EnumVariantType::Unit) {
                return Err(TypeError {
                    message: format!("enum variant '{}' is not a unit variant", qualified_path),
                });
            }

            // Find variant name from enum_type.variants
            let variant_name = enum_type
                .variants
                .iter()
                .find(|(_, vt)| vt == variant_type)
                .map(|(name, _)| name)
                .ok_or_else(|| TypeError {
                    message: format!("enum variant not found in {:?}", variant_type),
                })?;

            // Create fresh type variables for generic parameters
            let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
            for &old_id in &enum_type.type_var_ids {
                instantiation.insert(old_id, ctx.fresh_var());
            }

            // Build the expected enum type and unify with scrutinee
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
            let expected_enum_ty = Type::Enum {
                name: enum_type.name.clone(),
                type_args,
                variants: resolved_variants,
            };

            ctx.unify(scrutinee_ty, &expected_enum_ty)
                .map_err(|e| TypeError {
                    message: format!(
                        "enum pattern {} cannot match type {}: {}",
                        qualified_path,
                        ctx.resolve(scrutinee_ty),
                        e.message
                    ),
                })?;

            Ok((
                TypedPattern::EnumUnit {
                    path: QualifiedPath::new(vec![
                        enum_type.name.clone(),
                        variant_name.clone(),
                    ]),
                },
                HashMap::new(),
            ))
        }
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => {
            let kind = def.kind_name();
            Err(TypeError {
                message: format!("{} '{}' cannot be used as a pattern", kind, qualified_path),
            })
        }
        ResolvedPath::Local { name, .. } => Err(TypeError {
            message: format!(
                "variable '{}' cannot be used as a pattern (did you mean to use a wildcard '_'?)",
                name
            ),
        }),
    }
}

/// Check a call pattern (tuple enum variant): Option::Some(x), root::Result::Ok(v)
fn check_call_pattern(
    path: &Path,
    args: &TuplePattern,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    let resolved = resolution::resolve_pattern_path(path, current_module, &env.definitions)?;

    match resolved {
        ResolvedPath::Definition {
            def: Definition::EnumVariant(enum_type, variant_type),
            qualified_path,
        } => {
            // Must be a tuple variant
            let expected_types = match variant_type {
                EnumVariantType::Tuple(types) => types,
                EnumVariantType::Unit => {
                    return Err(TypeError {
                        message: format!(
                            "enum variant '{}' is a unit variant, doesn't take arguments",
                            qualified_path
                        ),
                    });
                }
                EnumVariantType::Struct(_) => {
                    return Err(TypeError {
                        message: format!(
                            "enum variant '{}' is a struct variant, use {{ }} syntax",
                            qualified_path
                        ),
                    });
                }
            };

            // Find variant name from enum_type.variants
            let variant_name = enum_type
                .variants
                .iter()
                .find(|(_, vt)| vt == variant_type)
                .map(|(name, _)| name)
                .ok_or_else(|| TypeError {
                    message: format!("enum variant not found in {:?}", variant_type),
                })?;

            // Create fresh type variables for generic parameters
            let mut instantiation: HashMap<TypeVarId, Type> = HashMap::new();
            for &old_id in &enum_type.type_var_ids {
                instantiation.insert(old_id, ctx.fresh_var());
            }

            // Build the expected enum type and unify with scrutinee
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
            let expected_enum_ty = Type::Enum {
                name: enum_type.name.clone(),
                type_args,
                variants: resolved_variants,
            };

            ctx.unify(scrutinee_ty, &expected_enum_ty)
                .map_err(|e| TypeError {
                    message: format!(
                        "enum pattern '{}' cannot match type {}: {}",
                        qualified_path,
                        ctx.resolve(scrutinee_ty),
                        e.message
                    ),
                })?;

            // Resolve expected types with type variable substitution
            let resolved_types: Vec<Type> = expected_types
                .iter()
                .map(|t| ctx.resolve(&substitute_type_vars(t, &instantiation)))
                .collect();

            check_enum_tuple_pattern(
                &enum_type.name,
                variant_name,
                args,
                &resolved_types,
                current_module,
                env,
                ctx,
            )
        }
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => {
            let kind = def.kind_name();
            Err(TypeError {
                message: format!(
                    "{} '{}' cannot be used as a call pattern",
                    kind, qualified_path
                ),
            })
        }
        ResolvedPath::Local { name, .. } => Err(TypeError {
            message: format!("variable '{}' cannot be used as a call pattern", name),
        }),
    }
}

/// Check a struct pattern: Point { x }, Message::Move { x, .. }
/// This handles both struct types and enum struct variants
fn check_struct_pattern(
    path: &Path,
    field_patterns: &[zoya_ast::StructFieldPattern],
    is_partial: bool,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    let resolved = resolution::resolve_pattern_path(path, current_module, &env.definitions)?;

    match resolved {
        ResolvedPath::Definition {
            def: Definition::Struct(struct_type),
            qualified_path,
        } => check_struct_type_pattern(
            &qualified_path,
            struct_type,
            field_patterns,
            is_partial,
            scrutinee_ty,
            current_module,
            env,
            ctx,
        ),
        ResolvedPath::Definition {
            def: Definition::EnumVariant(enum_type, variant_type),
            qualified_path,
        } => {
            // Find variant name from enum_type.variants
            let variant_name = enum_type
                .variants
                .iter()
                .find(|(_, vt)| vt == variant_type)
                .map(|(name, _)| name)
                .ok_or_else(|| TypeError {
                    message: format!("enum variant not found in {:?}", variant_type),
                })?;

            match variant_type {
                EnumVariantType::Struct(_) => check_enum_struct_variant_pattern(
                    &enum_type.name,
                    variant_name,
                    enum_type,
                    field_patterns,
                    is_partial,
                    scrutinee_ty,
                    current_module,
                    env,
                    ctx,
                ),
                EnumVariantType::Unit => Err(TypeError {
                    message: format!(
                        "enum variant '{}' is a unit variant, doesn't take fields",
                        qualified_path
                    ),
                }),
                EnumVariantType::Tuple(_) => Err(TypeError {
                    message: format!(
                        "enum variant '{}' is a tuple variant, use ( ) syntax",
                        qualified_path
                    ),
                }),
            }
        }
        ResolvedPath::Definition {
            qualified_path,
            def,
        } => Err(TypeError {
            message: format!(
                "{} '{}' cannot be used as a struct pattern",
                def.kind_name(),
                qualified_path
            ),
        }),
        ResolvedPath::Local { name, .. } => Err(TypeError {
            message: format!("variable '{}' cannot be used as a struct pattern", name),
        }),
    }
}

/// Check a struct type pattern: Point { x, y }
#[allow(clippy::too_many_arguments)]
fn check_struct_type_pattern(
    qualified_path: &QualifiedPath,
    struct_type: &zoya_ir::StructType,
    field_patterns: &[zoya_ast::StructFieldPattern],
    is_partial: bool,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    let struct_name = &struct_type.name;
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
        name: struct_name.to_string(),
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
        let expected_field_names: HashSet<&str> =
            struct_type.fields.iter().map(|(n, _)| n.as_str()).collect();
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
        let (typed_sub_pattern, sub_bindings) = check_pattern(
            &field_pattern.pattern,
            &resolved_field_type,
            current_module,
            env,
            ctx,
        )?;
        all_bindings.extend(sub_bindings);
        typed_fields.push((field_pattern.field_name.clone(), typed_sub_pattern));
    }

    let typed_pattern = if is_partial {
        TypedPattern::StructPartial {
            path: qualified_path.clone(),
            fields: typed_fields,
        }
    } else {
        TypedPattern::StructExact {
            path: qualified_path.clone(),
            fields: typed_fields,
        }
    };

    Ok((typed_pattern, all_bindings))
}

/// Check an enum struct variant pattern: Message::Move { x, y }
#[allow(clippy::too_many_arguments)]
fn check_enum_struct_variant_pattern(
    enum_name: &str,
    variant_name: &str,
    enum_type: &zoya_ir::EnumType,
    field_patterns: &[zoya_ast::StructFieldPattern],
    is_partial: bool,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    // Find the variant and verify it's a struct variant
    let variant_type = enum_type
        .variants
        .iter()
        .find(|(name, _)| name == variant_name)
        .map(|(_, vt)| vt)
        .ok_or_else(|| TypeError {
            message: format!("enum {} has no variant {}", enum_name, variant_name),
        })?;

    let expected_fields = match variant_type {
        EnumVariantType::Struct(fields) => fields,
        _ => {
            return Err(TypeError {
                message: format!(
                    "enum variant {}::{} is not a struct variant",
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

    // Build the expected enum type and unify with scrutinee
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
    let expected_enum_ty = Type::Enum {
        name: enum_name.to_string(),
        type_args,
        variants: resolved_variants,
    };

    ctx.unify(scrutinee_ty, &expected_enum_ty)
        .map_err(|e| TypeError {
            message: format!(
                "enum pattern {}::{} cannot match type {}: {}",
                enum_name,
                variant_name,
                ctx.resolve(scrutinee_ty),
                e.message
            ),
        })?;

    // Resolve expected fields with type variable substitution
    let resolved_fields: Vec<(String, Type)> = expected_fields
        .iter()
        .map(|(n, t)| {
            (
                n.clone(),
                ctx.resolve(&substitute_type_vars(t, &instantiation)),
            )
        })
        .collect();

    check_enum_struct_pattern(
        enum_name,
        variant_name,
        field_patterns,
        is_partial,
        &resolved_fields,
        current_module,
        env,
        ctx,
    )
}

/// Check an enum tuple variant pattern
fn check_enum_tuple_pattern(
    enum_name: &str,
    variant_name: &str,
    tuple_pattern: &TuplePattern,
    expected_types: &[Type],
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    let total_fields = expected_types.len();

    match tuple_pattern {
        TuplePattern::Empty => {
            if total_fields != 0 {
                return Err(TypeError {
                    message: format!(
                        "enum variant {}::{} has {} field(s), empty pattern not allowed",
                        enum_name, variant_name, total_fields
                    ),
                });
            }
            Ok((
                TypedPattern::EnumTupleExact {
                    path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
                    patterns: vec![],
                    total_fields: 0,
                },
                HashMap::new(),
            ))
        }

        TuplePattern::Exact(patterns) => {
            if patterns.len() != total_fields {
                return Err(TypeError {
                    message: format!(
                        "enum variant {}::{} has {} field(s) but pattern has {}",
                        enum_name,
                        variant_name,
                        total_fields,
                        patterns.len()
                    ),
                });
            }
            let (typed_patterns, bindings) =
                check_patterns_against_types(patterns, expected_types, current_module, env, ctx)?;
            Ok((
                TypedPattern::EnumTupleExact {
                    path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
                    patterns: typed_patterns,
                    total_fields,
                },
                bindings,
            ))
        }

        TuplePattern::Prefix {
            patterns,
            rest_binding,
        } => {
            if patterns.len() > total_fields {
                return Err(TypeError {
                    message: format!(
                        "enum variant {}::{} has {} field(s) but prefix pattern has {}",
                        enum_name,
                        variant_name,
                        total_fields,
                        patterns.len()
                    ),
                });
            }
            let (typed_patterns, mut bindings) =
                check_patterns_against_types(patterns, expected_types, current_module, env, ctx)?;

            // Handle rest binding: rest @ .. binds to tuple of remaining elements
            let rest_binding_with_type = if let Some(name) = rest_binding {
                if !is_snake_case(name) {
                    return Err(TypeError {
                        message: format!(
                            "variable '{}' should be snake_case (e.g., '{}')",
                            name,
                            to_snake_case(name)
                        ),
                    });
                }
                let rest_types: Vec<Type> = expected_types[patterns.len()..].to_vec();
                let rest_ty = Type::Tuple(rest_types);
                bindings.insert(name.clone(), rest_ty.clone());
                Some((name.clone(), rest_ty))
            } else {
                None
            };

            Ok((
                TypedPattern::EnumTuplePrefix {
                    path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
                    patterns: typed_patterns,
                    rest_binding: rest_binding_with_type,
                    total_fields,
                },
                bindings,
            ))
        }

        TuplePattern::Suffix {
            patterns,
            rest_binding,
        } => {
            if patterns.len() > total_fields {
                return Err(TypeError {
                    message: format!(
                        "enum variant {}::{} has {} field(s) but suffix pattern has {}",
                        enum_name,
                        variant_name,
                        total_fields,
                        patterns.len()
                    ),
                });
            }
            let start_idx = total_fields - patterns.len();
            let (typed_patterns, mut bindings) = check_patterns_against_types(
                patterns,
                &expected_types[start_idx..],
                current_module,
                env,
                ctx,
            )?;

            // Handle rest binding: rest @ .. binds to tuple of leading elements
            let rest_binding_with_type = if let Some(name) = rest_binding {
                if !is_snake_case(name) {
                    return Err(TypeError {
                        message: format!(
                            "variable '{}' should be snake_case (e.g., '{}')",
                            name,
                            to_snake_case(name)
                        ),
                    });
                }
                let rest_types: Vec<Type> = expected_types[..start_idx].to_vec();
                let rest_ty = Type::Tuple(rest_types);
                bindings.insert(name.clone(), rest_ty.clone());
                Some((name.clone(), rest_ty))
            } else {
                None
            };

            Ok((
                TypedPattern::EnumTupleSuffix {
                    path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
                    patterns: typed_patterns,
                    rest_binding: rest_binding_with_type,
                    total_fields,
                },
                bindings,
            ))
        }

        TuplePattern::PrefixSuffix {
            prefix,
            suffix,
            rest_binding,
        } => {
            let total_patterns = prefix.len() + suffix.len();
            if total_patterns > total_fields {
                return Err(TypeError {
                    message: format!(
                        "enum variant {}::{} has {} field(s) but pattern has {}",
                        enum_name, variant_name, total_fields, total_patterns
                    ),
                });
            }
            let (prefix_typed, mut bindings) =
                check_patterns_against_types(prefix, expected_types, current_module, env, ctx)?;
            let suffix_start = total_fields - suffix.len();
            let (suffix_typed, suffix_bindings) = check_patterns_against_types(
                suffix,
                &expected_types[suffix_start..],
                current_module,
                env,
                ctx,
            )?;
            bindings.extend(suffix_bindings);

            // Handle rest binding: rest @ .. binds to tuple of middle elements
            let rest_binding_with_type = if let Some(name) = rest_binding {
                if !is_snake_case(name) {
                    return Err(TypeError {
                        message: format!(
                            "variable '{}' should be snake_case (e.g., '{}')",
                            name,
                            to_snake_case(name)
                        ),
                    });
                }
                let rest_types: Vec<Type> = expected_types[prefix.len()..suffix_start].to_vec();
                let rest_ty = Type::Tuple(rest_types);
                bindings.insert(name.clone(), rest_ty.clone());
                Some((name.clone(), rest_ty))
            } else {
                None
            };

            Ok((
                TypedPattern::EnumTuplePrefixSuffix {
                    path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
                    prefix: prefix_typed,
                    suffix: suffix_typed,
                    rest_binding: rest_binding_with_type,
                    total_fields,
                },
                bindings,
            ))
        }
    }
}

/// Check an enum struct variant pattern
#[allow(clippy::too_many_arguments)]
fn check_enum_struct_pattern(
    enum_name: &str,
    variant_name: &str,
    field_patterns: &[zoya_ast::StructFieldPattern],
    is_partial: bool,
    expected_fields: &[(String, Type)],
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    // For exact patterns, verify all fields are covered
    if !is_partial {
        let expected_field_names: HashSet<&str> =
            expected_fields.iter().map(|(n, _)| n.as_str()).collect();
        let provided_field_names: HashSet<&str> = field_patterns
            .iter()
            .map(|f| f.field_name.as_str())
            .collect();

        for expected in &expected_field_names {
            if !provided_field_names.contains(expected) {
                return Err(TypeError {
                    message: format!(
                        "missing field '{}' in enum variant pattern {}::{} (use '..' for partial match)",
                        expected, enum_name, variant_name
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
        let (_, field_type) = expected_fields
            .iter()
            .find(|(n, _)| n == &field_pattern.field_name)
            .ok_or_else(|| TypeError {
                message: format!(
                    "enum variant {}::{} has no field '{}'",
                    enum_name, variant_name, field_pattern.field_name
                ),
            })?;

        // Recursively check the field pattern
        let (typed_sub_pattern, sub_bindings) =
            check_pattern(&field_pattern.pattern, field_type, current_module, env, ctx)?;
        all_bindings.extend(sub_bindings);
        typed_fields.push((field_pattern.field_name.clone(), typed_sub_pattern));
    }

    let typed_pattern = if is_partial {
        TypedPattern::EnumStructPartial {
            path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
            fields: typed_fields,
        }
    } else {
        TypedPattern::EnumStructExact {
            path: QualifiedPath::new(vec![enum_name.to_string(), variant_name.to_string()]),
            fields: typed_fields,
        }
    };

    Ok((typed_pattern, all_bindings))
}

/// Check a match arm
pub fn check_match_arm(
    arm: &MatchArm,
    scrutinee_ty: &Type,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedMatchArm, TypeError> {
    let (typed_pattern, bindings) =
        check_pattern(&arm.pattern, scrutinee_ty, current_module, env, ctx)?;

    // Create arm environment with pattern bindings
    let mut arm_env = env.clone();
    arm_env.locals.extend(
        bindings
            .into_iter()
            .map(|(n, ty)| (n, TypeScheme::mono(ty))),
    );

    let typed_result = check_expr(&arm.result, current_module, &arm_env, ctx)?;

    Ok(TypedMatchArm {
        pattern: typed_pattern,
        result: typed_result,
    })
}

/// Check if a pattern is irrefutable (always matches).
/// Returns Ok(()) if irrefutable, Err with message if refutable.
pub fn check_irrefutable(pattern: &Pattern) -> Result<(), String> {
    match pattern {
        Pattern::Wildcard => Ok(()),
        Pattern::Var(_) => Ok(()),

        Pattern::Tuple(tuple_pattern) => {
            let patterns = match tuple_pattern {
                TuplePattern::Empty => return Ok(()),
                TuplePattern::Exact(patterns) => patterns.iter().collect::<Vec<_>>(),
                TuplePattern::Prefix { patterns, .. } => patterns.iter().collect(),
                TuplePattern::Suffix { patterns, .. } => patterns.iter().collect(),
                TuplePattern::PrefixSuffix { prefix, suffix, .. } => {
                    prefix.iter().chain(suffix.iter()).collect()
                }
            };
            for p in patterns {
                check_irrefutable(p)?;
            }
            Ok(())
        }

        Pattern::Struct { fields, .. } => {
            for field in fields {
                check_irrefutable(&field.pattern)?;
            }
            Ok(())
        }

        Pattern::As { pattern, .. } => check_irrefutable(pattern),

        // Refutable patterns
        Pattern::Literal(_) => Err("literal patterns may not match".to_string()),
        Pattern::List(_) => {
            Err("list patterns may not match (lists have dynamic length)".to_string())
        }
        Pattern::Path(_) => Err("enum patterns may not match all variants".to_string()),
        Pattern::Call { .. } => Err("enum patterns may not match all variants".to_string()),
    }
}

/// Check a let binding and return a typed let binding plus the bindings it introduces.
pub fn check_let_binding(
    binding: &LetBinding,
    current_module: &ModulePath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedLetBinding, HashMap<String, Type>), TypeError> {
    // Check pattern is irrefutable
    check_irrefutable(&binding.pattern).map_err(|msg| TypeError {
        message: format!("refutable pattern in let binding: {}", msg),
    })?;

    // Type check the value
    let typed_value = check_expr(&binding.value, current_module, env, ctx)?;
    let inferred_type = typed_value.ty();

    // If type annotation exists (only allowed on simple Var patterns), unify with inferred type
    let binding_type = if let Some(ref annotation) = binding.type_annotation {
        let declared_type =
            resolve_type_annotation(annotation, &HashMap::new(), current_module, env)?;
        ctx.unify(&inferred_type, &declared_type)
            .map_err(|e| TypeError {
                message: format!(
                    "let binding declares type {} but value has type {}: {}",
                    declared_type,
                    ctx.resolve(&inferred_type),
                    e.message
                ),
            })?;
        declared_type
    } else {
        ctx.resolve(&inferred_type)
    };

    // Type check the pattern against the value type
    let (typed_pattern, bindings) =
        check_pattern(&binding.pattern, &binding_type, current_module, env, ctx)?;

    Ok((
        TypedLetBinding {
            pattern: typed_pattern,
            value: typed_value,
            ty: binding_type,
        },
        bindings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::{Expr, Path, PathPrefix, StructFieldPattern};
    use zoya_ir::{Definition, EnumType, QualifiedPath, StructType};

    fn qpath(path: &str) -> QualifiedPath {
        QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
    }

    fn default_env() -> TypeEnv {
        TypeEnv::default()
    }

    // ========================================================================
    // Variable pattern tests
    // ========================================================================

    #[test]
    fn test_pattern_var_snake_case() {
        let pattern = Pattern::Var("my_var".to_string());
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::Var { .. }));
        assert_eq!(bindings.get("my_var"), Some(&Type::Int));
    }

    #[test]
    fn test_pattern_var_invalid_pascal_case() {
        let pattern = Pattern::Var("MyVar".to_string());
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("should be snake_case"));
        assert!(err.message.contains("my_var"));
    }

    #[test]
    fn test_pattern_var_underscore_prefix() {
        let pattern = Pattern::Var("_unused".to_string());
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::String,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
    }

    // ========================================================================
    // Wildcard pattern tests
    // ========================================================================

    #[test]
    fn test_pattern_wildcard() {
        let pattern = Pattern::Wildcard;
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::Wildcard));
        assert!(bindings.is_empty());
    }

    // ========================================================================
    // Literal pattern tests
    // ========================================================================

    #[test]
    fn test_pattern_literal_int() {
        let pattern = Pattern::Literal(Box::new(Expr::Int(42)));
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_pattern_literal_type_mismatch() {
        let pattern = Pattern::Literal(Box::new(Expr::Int(42)));
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::String,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("pattern type"));
        assert!(err.message.contains("does not match scrutinee type"));
    }

    // ========================================================================
    // List pattern tests - Empty
    // ========================================================================

    #[test]
    fn test_list_pattern_empty() {
        let pattern = Pattern::List(ListPattern::Empty);
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::ListEmpty));
        assert!(bindings.is_empty());
    }

    // ========================================================================
    // List pattern tests - Exact
    // ========================================================================

    #[test]
    fn test_list_pattern_exact() {
        let pattern = Pattern::List(ListPattern::Exact(vec![
            Pattern::Var("a".to_string()),
            Pattern::Var("b".to_string()),
        ]));
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::ListExact { len: 2, .. }));
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings.get("a"), Some(&Type::Int));
        assert_eq!(bindings.get("b"), Some(&Type::Int));
    }

    // ========================================================================
    // List pattern tests - Prefix
    // ========================================================================

    #[test]
    fn test_list_pattern_prefix() {
        let pattern = Pattern::List(ListPattern::Prefix {
            patterns: vec![Pattern::Var("head".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::ListPrefix { min_len: 1, .. }));
        assert_eq!(bindings.get("head"), Some(&Type::Int));
    }

    #[test]
    fn test_list_pattern_prefix_with_rest_binding() {
        let pattern = Pattern::List(ListPattern::Prefix {
            patterns: vec![Pattern::Var("head".to_string())],
            rest_binding: Some("tail".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("head"), Some(&Type::Int));
        assert_eq!(bindings.get("tail"), Some(&Type::List(Box::new(Type::Int))));
    }

    #[test]
    fn test_list_pattern_prefix_rest_binding_invalid_name() {
        let pattern = Pattern::List(ListPattern::Prefix {
            patterns: vec![Pattern::Var("head".to_string())],
            rest_binding: Some("InvalidName".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("should be snake_case"));
    }

    // ========================================================================
    // List pattern tests - Suffix
    // ========================================================================

    #[test]
    fn test_list_pattern_suffix() {
        let pattern = Pattern::List(ListPattern::Suffix {
            patterns: vec![Pattern::Var("last".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::String)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::ListSuffix { min_len: 1, .. }));
        assert_eq!(bindings.get("last"), Some(&Type::String));
    }

    #[test]
    fn test_list_pattern_suffix_with_rest_binding() {
        let pattern = Pattern::List(ListPattern::Suffix {
            patterns: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("init".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("last"), Some(&Type::Int));
        assert_eq!(bindings.get("init"), Some(&Type::List(Box::new(Type::Int))));
    }

    #[test]
    fn test_list_pattern_suffix_rest_binding_invalid_name() {
        let pattern = Pattern::List(ListPattern::Suffix {
            patterns: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("BadName".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
    }

    // ========================================================================
    // List pattern tests - PrefixSuffix
    // ========================================================================

    #[test]
    fn test_list_pattern_prefix_suffix() {
        let pattern = Pattern::List(ListPattern::PrefixSuffix {
            prefix: vec![Pattern::Var("first".to_string())],
            suffix: vec![Pattern::Var("last".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(
            typed,
            TypedPattern::ListPrefixSuffix { min_len: 2, .. }
        ));
        assert_eq!(bindings.get("first"), Some(&Type::Int));
        assert_eq!(bindings.get("last"), Some(&Type::Int));
    }

    #[test]
    fn test_list_pattern_prefix_suffix_with_rest_binding() {
        let pattern = Pattern::List(ListPattern::PrefixSuffix {
            prefix: vec![Pattern::Var("first".to_string())],
            suffix: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("middle".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(
            bindings.get("middle"),
            Some(&Type::List(Box::new(Type::Int)))
        );
    }

    #[test]
    fn test_list_pattern_prefix_suffix_rest_binding_invalid_name() {
        let pattern = Pattern::List(ListPattern::PrefixSuffix {
            prefix: vec![Pattern::Var("first".to_string())],
            suffix: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("BadMiddle".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::List(Box::new(Type::Int)),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
    }

    // ========================================================================
    // List pattern type mismatch tests
    // ========================================================================

    #[test]
    fn test_list_pattern_non_list_scrutinee() {
        let pattern = Pattern::List(ListPattern::Empty);
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("list pattern cannot match type"));
    }

    // ========================================================================
    // Tuple pattern tests - Empty
    // ========================================================================

    #[test]
    fn test_tuple_pattern_empty() {
        let pattern = Pattern::Tuple(TuplePattern::Empty);
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().0, TypedPattern::TupleEmpty));
    }

    #[test]
    fn test_tuple_pattern_empty_mismatch() {
        let pattern = Pattern::Tuple(TuplePattern::Empty);
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("empty tuple pattern cannot match"));
    }

    // ========================================================================
    // Tuple pattern tests - Exact
    // ========================================================================

    #[test]
    fn test_tuple_pattern_exact() {
        let pattern = Pattern::Tuple(TuplePattern::Exact(vec![
            Pattern::Var("x".to_string()),
            Pattern::Var("y".to_string()),
        ]));
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::TupleExact { len: 2, .. }));
        assert_eq!(bindings.get("x"), Some(&Type::Int));
        assert_eq!(bindings.get("y"), Some(&Type::String));
    }

    #[test]
    fn test_tuple_pattern_exact_length_mismatch() {
        let pattern = Pattern::Tuple(TuplePattern::Exact(vec![
            Pattern::Var("x".to_string()),
            Pattern::Var("y".to_string()),
            Pattern::Var("z".to_string()),
        ]));
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::Int]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("tuple pattern has 3 elements but tuple has 2")
        );
    }

    // ========================================================================
    // Tuple pattern tests - Prefix
    // ========================================================================

    #[test]
    fn test_tuple_pattern_prefix() {
        let pattern = Pattern::Tuple(TuplePattern::Prefix {
            patterns: vec![Pattern::Var("first".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(
            typed,
            TypedPattern::TuplePrefix { total_len: 3, .. }
        ));
        assert_eq!(bindings.get("first"), Some(&Type::Int));
    }

    #[test]
    fn test_tuple_pattern_prefix_with_rest_binding() {
        let pattern = Pattern::Tuple(TuplePattern::Prefix {
            patterns: vec![Pattern::Var("first".to_string())],
            rest_binding: Some("rest".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("first"), Some(&Type::Int));
        assert_eq!(
            bindings.get("rest"),
            Some(&Type::Tuple(vec![Type::String, Type::Bool]))
        );
    }

    #[test]
    fn test_tuple_pattern_prefix_too_long() {
        let pattern = Pattern::Tuple(TuplePattern::Prefix {
            patterns: vec![
                Pattern::Var("a".to_string()),
                Pattern::Var("b".to_string()),
                Pattern::Var("c".to_string()),
            ],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::Int]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("prefix elements"));
    }

    #[test]
    fn test_tuple_pattern_prefix_rest_binding_invalid_name() {
        let pattern = Pattern::Tuple(TuplePattern::Prefix {
            patterns: vec![Pattern::Var("first".to_string())],
            rest_binding: Some("BadName".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
    }

    // ========================================================================
    // Tuple pattern tests - Suffix
    // ========================================================================

    #[test]
    fn test_tuple_pattern_suffix() {
        let pattern = Pattern::Tuple(TuplePattern::Suffix {
            patterns: vec![Pattern::Var("last".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(
            typed,
            TypedPattern::TupleSuffix { total_len: 3, .. }
        ));
        assert_eq!(bindings.get("last"), Some(&Type::Bool));
    }

    #[test]
    fn test_tuple_pattern_suffix_with_rest_binding() {
        let pattern = Pattern::Tuple(TuplePattern::Suffix {
            patterns: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("init".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("last"), Some(&Type::Bool));
        assert_eq!(
            bindings.get("init"),
            Some(&Type::Tuple(vec![Type::Int, Type::String]))
        );
    }

    #[test]
    fn test_tuple_pattern_suffix_too_long() {
        let pattern = Pattern::Tuple(TuplePattern::Suffix {
            patterns: vec![
                Pattern::Var("a".to_string()),
                Pattern::Var("b".to_string()),
                Pattern::Var("c".to_string()),
            ],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::Int]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("suffix elements"));
    }

    #[test]
    fn test_tuple_pattern_suffix_rest_binding_invalid_name() {
        let pattern = Pattern::Tuple(TuplePattern::Suffix {
            patterns: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("BadInit".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
    }

    // ========================================================================
    // Tuple pattern tests - PrefixSuffix
    // ========================================================================

    #[test]
    fn test_tuple_pattern_prefix_suffix() {
        let pattern = Pattern::Tuple(TuplePattern::PrefixSuffix {
            prefix: vec![Pattern::Var("first".to_string())],
            suffix: vec![Pattern::Var("last".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(
            typed,
            TypedPattern::TuplePrefixSuffix { total_len: 3, .. }
        ));
        assert_eq!(bindings.get("first"), Some(&Type::Int));
        assert_eq!(bindings.get("last"), Some(&Type::Bool));
    }

    #[test]
    fn test_tuple_pattern_prefix_suffix_with_rest_binding() {
        let pattern = Pattern::Tuple(TuplePattern::PrefixSuffix {
            prefix: vec![Pattern::Var("first".to_string())],
            suffix: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("middle".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Float, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("first"), Some(&Type::Int));
        assert_eq!(bindings.get("last"), Some(&Type::Bool));
        assert_eq!(
            bindings.get("middle"),
            Some(&Type::Tuple(vec![Type::String, Type::Float]))
        );
    }

    #[test]
    fn test_tuple_pattern_prefix_suffix_too_long() {
        let pattern = Pattern::Tuple(TuplePattern::PrefixSuffix {
            prefix: vec![Pattern::Var("a".to_string()), Pattern::Var("b".to_string())],
            suffix: vec![Pattern::Var("c".to_string()), Pattern::Var("d".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::Int, Type::Int]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("4 elements but tuple has only 3"));
    }

    #[test]
    fn test_tuple_pattern_prefix_suffix_rest_binding_invalid_name() {
        let pattern = Pattern::Tuple(TuplePattern::PrefixSuffix {
            prefix: vec![Pattern::Var("first".to_string())],
            suffix: vec![Pattern::Var("last".to_string())],
            rest_binding: Some("BadMiddle".to_string()),
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
    }

    // ========================================================================
    // Tuple pattern tests - Type inference for Type::Var
    // ========================================================================

    #[test]
    fn test_tuple_pattern_infer_type() {
        let pattern = Pattern::Tuple(TuplePattern::Exact(vec![
            Pattern::Var("x".to_string()),
            Pattern::Var("y".to_string()),
        ]));
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = ctx.fresh_var();
        let result = check_pattern(
            &pattern,
            &scrutinee_ty,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        // The scrutinee should be unified to a tuple type
        let resolved = ctx.resolve(&scrutinee_ty);
        assert!(matches!(resolved, Type::Tuple(elems) if elems.len() == 2));
    }

    #[test]
    fn test_tuple_pattern_cannot_infer_with_rest() {
        let pattern = Pattern::Tuple(TuplePattern::Prefix {
            patterns: vec![Pattern::Var("x".to_string())],
            rest_binding: None,
        });
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = ctx.fresh_var();
        let result = check_pattern(
            &pattern,
            &scrutinee_ty,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("cannot infer tuple type"));
    }

    // ========================================================================
    // Tuple pattern tests - Non-tuple scrutinee
    // ========================================================================

    #[test]
    fn test_tuple_pattern_non_tuple_scrutinee() {
        let pattern = Pattern::Tuple(TuplePattern::Exact(vec![Pattern::Var("x".to_string())]));
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("tuple pattern cannot match type"));
    }

    // ========================================================================
    // Struct pattern tests
    // ========================================================================

    fn env_with_point() -> TypeEnv {
        let mut env = TypeEnv::default();
        env.register(
            qpath("root::Point"),
            Definition::Struct(StructType {
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
            }),
        );
        env
    }

    #[test]
    fn test_struct_pattern_exact() {
        let pattern = Pattern::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![
                StructFieldPattern {
                    field_name: "x".to_string(),
                    pattern: Box::new(Pattern::Var("px".to_string())),
                },
                StructFieldPattern {
                    field_name: "y".to_string(),
                    pattern: Box::new(Pattern::Var("py".to_string())),
                },
            ],
            is_partial: false,
        };
        let env = env_with_point();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Struct {
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::StructExact { .. }));
        assert_eq!(bindings.get("px"), Some(&Type::Int));
        assert_eq!(bindings.get("py"), Some(&Type::Int));
    }

    #[test]
    fn test_struct_pattern_exact_missing_field() {
        let pattern = Pattern::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![
                StructFieldPattern {
                    field_name: "x".to_string(),
                    pattern: Box::new(Pattern::Var("px".to_string())),
                },
                // Missing "y" field
            ],
            is_partial: false,
        };
        let env = env_with_point();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Struct {
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("missing field 'y'"));
        assert!(err.message.contains("use '..' for partial match"));
    }

    #[test]
    fn test_struct_pattern_partial() {
        let pattern = Pattern::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![StructFieldPattern {
                field_name: "x".to_string(),
                pattern: Box::new(Pattern::Var("px".to_string())),
            }],
            is_partial: true,
        };
        let env = env_with_point();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Struct {
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::StructPartial { .. }));
        assert_eq!(bindings.get("px"), Some(&Type::Int));
        assert!(bindings.get("py").is_none()); // y not bound
    }

    #[test]
    fn test_struct_pattern_unknown_field() {
        let pattern = Pattern::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![StructFieldPattern {
                field_name: "z".to_string(), // Point has no 'z' field
                pattern: Box::new(Pattern::Var("pz".to_string())),
            }],
            is_partial: true,
        };
        let env = env_with_point();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Struct {
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("struct Point has no field 'z'"));
    }

    #[test]
    fn test_struct_pattern_unknown_struct() {
        let pattern = Pattern::Struct {
            path: Path::simple("UnknownStruct".to_string()),
            fields: vec![],
            is_partial: true,
        };
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }

    // ========================================================================
    // Enum pattern tests
    // ========================================================================

    fn env_with_option() -> TypeEnv {
        let mut env = TypeEnv::default();
        let enum_type = EnumType {
            name: "Option".to_string(),
            type_params: vec!["T".to_string()],
            type_var_ids: vec![TypeVarId(1)],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                (
                    "Some".to_string(),
                    EnumVariantType::Tuple(vec![Type::Var(TypeVarId(1))]),
                ),
            ],
        };
        env.register(qpath("root::Option"), Definition::Enum(enum_type.clone()));
        // Register each variant separately
        for (variant_name, variant_type) in &enum_type.variants {
            env.register(
                qpath(&format!("root::Option::{}", variant_name)),
                Definition::EnumVariant(enum_type.clone(), variant_type.clone()),
            );
        }
        env
    }

    fn env_with_message() -> TypeEnv {
        let mut env = TypeEnv::default();
        let enum_type = EnumType {
            name: "Message".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            variants: vec![
                ("Quit".to_string(), EnumVariantType::Unit),
                (
                    "Move".to_string(),
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
                (
                    "Write".to_string(),
                    EnumVariantType::Tuple(vec![Type::String]),
                ),
            ],
        };
        env.register(qpath("root::Message"), Definition::Enum(enum_type.clone()));
        // Register each variant separately
        for (variant_name, variant_type) in &enum_type.variants {
            env.register(
                qpath(&format!("root::Message::{}", variant_name)),
                Definition::EnumVariant(enum_type.clone(), variant_type.clone()),
            );
        }
        env
    }

    #[test]
    fn test_enum_pattern_unit_variant() {
        // Pattern::Path is used for unit enum variants
        let pattern = Pattern::Path(Path {
            prefix: PathPrefix::None,
            segments: vec!["Option".to_string(), "None".to_string()],
            type_args: None,
        });
        let env = env_with_option();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (typed, bindings) = result.unwrap();
        assert!(matches!(typed, TypedPattern::EnumUnit { .. }));
        assert!(bindings.is_empty());
    }

    #[test]
    fn test_enum_pattern_tuple_variant() {
        // Pattern::Call is used for tuple enum variants
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Option".to_string(), "Some".to_string()],
                type_args: None,
            },
            args: TuplePattern::Exact(vec![Pattern::Var("value".to_string())]),
        };
        let env = env_with_option();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("value"), Some(&Type::Int));
    }

    #[test]
    fn test_enum_pattern_struct_variant() {
        // Pattern::Struct with a qualified path is used for enum struct variants
        let pattern = Pattern::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![
                StructFieldPattern {
                    field_name: "x".to_string(),
                    pattern: Box::new(Pattern::Var("px".to_string())),
                },
                StructFieldPattern {
                    field_name: "y".to_string(),
                    pattern: Box::new(Pattern::Var("py".to_string())),
                },
            ],
            is_partial: false,
        };
        let env = env_with_message();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Message".to_string(),
            type_args: vec![],
            variants: vec![
                ("Quit".to_string(), EnumVariantType::Unit),
                (
                    "Move".to_string(),
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
                (
                    "Write".to_string(),
                    EnumVariantType::Tuple(vec![Type::String]),
                ),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("px"), Some(&Type::Int));
        assert_eq!(bindings.get("py"), Some(&Type::Int));
    }

    #[test]
    fn test_enum_pattern_struct_variant_partial() {
        let pattern = Pattern::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![StructFieldPattern {
                field_name: "x".to_string(),
                pattern: Box::new(Pattern::Var("px".to_string())),
            }],
            is_partial: true,
        };
        let env = env_with_message();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Message".to_string(),
            type_args: vec![],
            variants: vec![
                ("Quit".to_string(), EnumVariantType::Unit),
                (
                    "Move".to_string(),
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
                (
                    "Write".to_string(),
                    EnumVariantType::Tuple(vec![Type::String]),
                ),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_enum_pattern_struct_variant_missing_field() {
        let pattern = Pattern::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Move".to_string()],
                type_args: None,
            },
            fields: vec![
                StructFieldPattern {
                    field_name: "x".to_string(),
                    pattern: Box::new(Pattern::Var("px".to_string())),
                },
                // Missing "y" field
            ],
            is_partial: false,
        };
        let env = env_with_message();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Message".to_string(),
            type_args: vec![],
            variants: vec![
                ("Quit".to_string(), EnumVariantType::Unit),
                (
                    "Move".to_string(),
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
                (
                    "Write".to_string(),
                    EnumVariantType::Tuple(vec![Type::String]),
                ),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("missing field 'y'"));
    }

    #[test]
    fn test_enum_pattern_kind_mismatch_unit_vs_tuple() {
        // Try to match a tuple variant with a unit pattern (Pattern::Path)
        let pattern = Pattern::Path(Path {
            prefix: PathPrefix::None,
            segments: vec!["Option".to_string(), "Some".to_string()],
            type_args: None,
        });
        let env = env_with_option();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("is not a unit variant"));
    }

    #[test]
    fn test_enum_pattern_kind_mismatch_tuple_vs_unit() {
        // Try to match a unit variant with a tuple pattern (Pattern::Call)
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Option".to_string(), "None".to_string()],
                type_args: None,
            },
            args: TuplePattern::Exact(vec![Pattern::Var("x".to_string())]),
        };
        let env = env_with_option();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("is a unit variant"),
            "Expected 'is a unit variant' but got: {}",
            err.message
        );
    }

    #[test]
    fn test_enum_pattern_kind_mismatch_struct_vs_tuple() {
        // Try to match a tuple variant with a struct pattern (Pattern::Struct)
        let pattern = Pattern::Struct {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Message".to_string(), "Write".to_string()],
                type_args: None,
            },
            fields: vec![],
            is_partial: true,
        };
        let env = env_with_message();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Message".to_string(),
            type_args: vec![],
            variants: vec![
                ("Quit".to_string(), EnumVariantType::Unit),
                (
                    "Move".to_string(),
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
                (
                    "Write".to_string(),
                    EnumVariantType::Tuple(vec![Type::String]),
                ),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("is a tuple variant"),
            "Expected 'is a tuple variant' but got: {}",
            err.message
        );
    }

    #[test]
    fn test_enum_pattern_unknown_enum() {
        let pattern = Pattern::Path(Path {
            prefix: PathPrefix::None,
            segments: vec!["UnknownEnum".to_string(), "Variant".to_string()],
            type_args: None,
        });
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown path"));
    }

    #[test]
    fn test_enum_pattern_unknown_variant() {
        let pattern = Pattern::Path(Path {
            prefix: PathPrefix::None,
            segments: vec!["Option".to_string(), "Unknown".to_string()],
            type_args: None,
        });
        let env = env_with_option();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
            ],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // With the new scheme, unknown variants are reported as unknown paths
        assert!(
            err.message.contains("unknown path") && err.message.contains("Unknown"),
            "Expected 'unknown path' error with 'Unknown' but got: {}",
            err.message
        );
    }

    #[test]
    fn test_enum_pattern_invalid_path() {
        // Single-segment path is treated as struct, not enum
        let pattern = Pattern::Struct {
            path: Path::simple("JustOneName".to_string()),
            fields: vec![],
            is_partial: true,
        };
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }

    // ========================================================================
    // As pattern tests
    // ========================================================================

    #[test]
    fn test_as_pattern() {
        let pattern = Pattern::As {
            name: "whole".to_string(),
            pattern: Box::new(Pattern::Tuple(TuplePattern::Exact(vec![
                Pattern::Var("x".to_string()),
                Pattern::Var("y".to_string()),
            ]))),
        };
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Tuple(vec![Type::Int, Type::String]);
        let result = check_pattern(
            &pattern,
            &scrutinee_ty,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(
            bindings.get("whole"),
            Some(&Type::Tuple(vec![Type::Int, Type::String]))
        );
        assert_eq!(bindings.get("x"), Some(&Type::Int));
        assert_eq!(bindings.get("y"), Some(&Type::String));
    }

    #[test]
    fn test_as_pattern_invalid_name() {
        let pattern = Pattern::As {
            name: "BadName".to_string(),
            pattern: Box::new(Pattern::Var("x".to_string())),
        };
        let mut ctx = UnifyCtx::new();
        let result = check_pattern(
            &pattern,
            &Type::Int,
            &ModulePath::root(),
            &default_env(),
            &mut ctx,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("should be snake_case"));
    }

    // ========================================================================
    // check_irrefutable tests
    // ========================================================================

    #[test]
    fn test_irrefutable_wildcard() {
        assert!(check_irrefutable(&Pattern::Wildcard).is_ok());
    }

    #[test]
    fn test_irrefutable_var() {
        assert!(check_irrefutable(&Pattern::Var("x".to_string())).is_ok());
    }

    #[test]
    fn test_irrefutable_tuple_empty() {
        let pattern = Pattern::Tuple(TuplePattern::Empty);
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_tuple_exact_irrefutable() {
        let pattern = Pattern::Tuple(TuplePattern::Exact(vec![
            Pattern::Var("x".to_string()),
            Pattern::Wildcard,
        ]));
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_tuple_exact_refutable() {
        let pattern = Pattern::Tuple(TuplePattern::Exact(vec![
            Pattern::Var("x".to_string()),
            Pattern::Literal(Box::new(Expr::Int(42))), // Literal is refutable
        ]));
        assert!(check_irrefutable(&pattern).is_err());
    }

    #[test]
    fn test_irrefutable_tuple_prefix() {
        let pattern = Pattern::Tuple(TuplePattern::Prefix {
            patterns: vec![Pattern::Var("x".to_string())],
            rest_binding: None,
        });
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_tuple_suffix() {
        let pattern = Pattern::Tuple(TuplePattern::Suffix {
            patterns: vec![Pattern::Var("x".to_string())],
            rest_binding: None,
        });
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_tuple_prefix_suffix() {
        let pattern = Pattern::Tuple(TuplePattern::PrefixSuffix {
            prefix: vec![Pattern::Var("a".to_string())],
            suffix: vec![Pattern::Var("b".to_string())],
            rest_binding: None,
        });
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_struct() {
        let pattern = Pattern::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![StructFieldPattern {
                field_name: "x".to_string(),
                pattern: Box::new(Pattern::Var("px".to_string())),
            }],
            is_partial: false,
        };
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_struct_refutable_field() {
        let pattern = Pattern::Struct {
            path: Path::simple("Point".to_string()),
            fields: vec![StructFieldPattern {
                field_name: "x".to_string(),
                pattern: Box::new(Pattern::Literal(Box::new(Expr::Int(0)))),
            }],
            is_partial: false,
        };
        assert!(check_irrefutable(&pattern).is_err());
    }

    #[test]
    fn test_irrefutable_as_pattern() {
        let pattern = Pattern::As {
            name: "whole".to_string(),
            pattern: Box::new(Pattern::Var("x".to_string())),
        };
        assert!(check_irrefutable(&pattern).is_ok());
    }

    #[test]
    fn test_irrefutable_as_pattern_refutable_inner() {
        let pattern = Pattern::As {
            name: "whole".to_string(),
            pattern: Box::new(Pattern::Literal(Box::new(Expr::Int(42)))),
        };
        assert!(check_irrefutable(&pattern).is_err());
    }

    #[test]
    fn test_refutable_literal() {
        let pattern = Pattern::Literal(Box::new(Expr::Int(42)));
        let result = check_irrefutable(&pattern);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("literal patterns may not match")
        );
    }

    #[test]
    fn test_refutable_list() {
        let pattern = Pattern::List(ListPattern::Empty);
        let result = check_irrefutable(&pattern);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("list patterns may not match"));
    }

    #[test]
    fn test_refutable_path_pattern() {
        let pattern = Pattern::Path(Path {
            prefix: PathPrefix::None,
            segments: vec!["Option".to_string(), "None".to_string()],
            type_args: None,
        });
        let result = check_irrefutable(&pattern);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("enum patterns may not match"));
    }

    #[test]
    fn test_refutable_call_pattern() {
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Option".to_string(), "Some".to_string()],
                type_args: None,
            },
            args: TuplePattern::Exact(vec![Pattern::Var("x".to_string())]),
        };
        let result = check_irrefutable(&pattern);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("enum patterns may not match"));
    }

    // ========================================================================
    // Enum tuple pattern with prefix/suffix variants
    // ========================================================================

    fn env_with_multi_tuple_enum() -> TypeEnv {
        let mut env = TypeEnv::default();
        let enum_type = EnumType {
            name: "Data".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            variants: vec![(
                "Triple".to_string(),
                EnumVariantType::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            )],
        };
        env.register(qpath("root::Data"), Definition::Enum(enum_type.clone()));
        // Register each variant separately
        for (variant_name, variant_type) in &enum_type.variants {
            env.register(
                qpath(&format!("root::Data::{}", variant_name)),
                Definition::EnumVariant(enum_type.clone(), variant_type.clone()),
            );
        }
        env
    }

    #[test]
    fn test_enum_tuple_pattern_prefix() {
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Data".to_string(), "Triple".to_string()],
                type_args: None,
            },
            args: TuplePattern::Prefix {
                patterns: vec![Pattern::Var("first".to_string())],
                rest_binding: Some("rest".to_string()),
            },
        };
        let env = env_with_multi_tuple_enum();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Data".to_string(),
            type_args: vec![],
            variants: vec![(
                "Triple".to_string(),
                EnumVariantType::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            )],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("first"), Some(&Type::Int));
        assert_eq!(
            bindings.get("rest"),
            Some(&Type::Tuple(vec![Type::String, Type::Bool]))
        );
    }

    #[test]
    fn test_enum_tuple_pattern_suffix() {
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Data".to_string(), "Triple".to_string()],
                type_args: None,
            },
            args: TuplePattern::Suffix {
                patterns: vec![Pattern::Var("last".to_string())],
                rest_binding: Some("init".to_string()),
            },
        };
        let env = env_with_multi_tuple_enum();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Data".to_string(),
            type_args: vec![],
            variants: vec![(
                "Triple".to_string(),
                EnumVariantType::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            )],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("last"), Some(&Type::Bool));
        assert_eq!(
            bindings.get("init"),
            Some(&Type::Tuple(vec![Type::Int, Type::String]))
        );
    }

    #[test]
    fn test_enum_tuple_pattern_prefix_suffix() {
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Data".to_string(), "Triple".to_string()],
                type_args: None,
            },
            args: TuplePattern::PrefixSuffix {
                prefix: vec![Pattern::Var("first".to_string())],
                suffix: vec![Pattern::Var("last".to_string())],
                rest_binding: Some("middle".to_string()),
            },
        };
        let env = env_with_multi_tuple_enum();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Data".to_string(),
            type_args: vec![],
            variants: vec![(
                "Triple".to_string(),
                EnumVariantType::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            )],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_ok());
        let (_, bindings) = result.unwrap();
        assert_eq!(bindings.get("first"), Some(&Type::Int));
        assert_eq!(bindings.get("last"), Some(&Type::Bool));
        assert_eq!(
            bindings.get("middle"),
            Some(&Type::Tuple(vec![Type::String]))
        );
    }

    #[test]
    fn test_enum_tuple_pattern_too_many_elements() {
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Data".to_string(), "Triple".to_string()],
                type_args: None,
            },
            args: TuplePattern::Exact(vec![
                Pattern::Var("a".to_string()),
                Pattern::Var("b".to_string()),
                Pattern::Var("c".to_string()),
                Pattern::Var("d".to_string()), // Too many!
            ]),
        };
        let env = env_with_multi_tuple_enum();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Data".to_string(),
            type_args: vec![],
            variants: vec![(
                "Triple".to_string(),
                EnumVariantType::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            )],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("3 field(s) but pattern has 4"));
    }

    #[test]
    fn test_enum_tuple_pattern_empty_on_nonempty() {
        let pattern = Pattern::Call {
            path: Path {
                prefix: PathPrefix::None,
                segments: vec!["Data".to_string(), "Triple".to_string()],
                type_args: None,
            },
            args: TuplePattern::Empty,
        };
        let env = env_with_multi_tuple_enum();
        let mut ctx = UnifyCtx::new();
        let scrutinee_ty = Type::Enum {
            name: "Data".to_string(),
            type_args: vec![],
            variants: vec![(
                "Triple".to_string(),
                EnumVariantType::Tuple(vec![Type::Int, Type::String, Type::Bool]),
            )],
        };
        let result = check_pattern(&pattern, &scrutinee_ty, &ModulePath::root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("empty pattern not allowed"));
    }
}
