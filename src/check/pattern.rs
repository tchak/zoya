use std::collections::{HashMap, HashSet};

use crate::ast::{
    EnumPattern, EnumPatternFields, LetBinding, ListPattern, MatchArm, Pattern,
    StructPattern, TuplePattern,
};
use crate::ir::{QualifiedPath, TypedLetBinding, TypedMatchArm, TypedPattern};
use crate::types::{EnumVariantType, Type, TypeError, TypeScheme, TypeVarId};
use crate::unify::UnifyCtx;

use super::naming::{is_snake_case, to_snake_case};
use super::type_resolver::resolve_type_annotation;
use super::{check_with_env, substitute_type_vars, substitute_variant_type_vars, TypeEnv};

/// Check a list of patterns against a single element type (for list patterns)
pub fn check_patterns_against_elem(
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
pub fn check_patterns_against_types(
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
pub fn check_pattern(
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

                ListPattern::Prefix {
                    patterns,
                    rest_binding,
                } => {
                    let (typed_patterns, mut bindings) =
                        check_patterns_against_elem(patterns, &resolved_elem, env, ctx)?;

                    // Handle rest binding: rest @ .. binds to List<T>
                    if let Some(name) = rest_binding {
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
                        bindings.insert(name.clone(), rest_ty);
                    }

                    Ok((
                        TypedPattern::ListPrefix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding.clone(),
                            min_len: patterns.len(),
                        },
                        bindings,
                    ))
                }

                ListPattern::Suffix {
                    patterns,
                    rest_binding,
                } => {
                    let (typed_patterns, mut bindings) =
                        check_patterns_against_elem(patterns, &resolved_elem, env, ctx)?;

                    // Handle rest binding
                    if let Some(name) = rest_binding {
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
                        bindings.insert(name.clone(), rest_ty);
                    }

                    Ok((
                        TypedPattern::ListSuffix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding.clone(),
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
                    let (prefix_typed, mut bindings) =
                        check_patterns_against_elem(prefix, &resolved_elem, env, ctx)?;
                    let (suffix_typed, suffix_bindings) =
                        check_patterns_against_elem(suffix, &resolved_elem, env, ctx)?;
                    bindings.extend(suffix_bindings);

                    // Handle rest binding
                    if let Some(name) = rest_binding {
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
                        bindings.insert(name.clone(), rest_ty);
                    }

                    Ok((
                        TypedPattern::ListPrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            rest_binding: rest_binding.clone(),
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
                            ctx.unify(scrutinee_ty, &Type::Tuple(vec![]))
                                .map_err(|e| TypeError {
                                    message: format!(
                                        "tuple pattern cannot match type {}: {}",
                                        ctx.resolve(scrutinee_ty),
                                        e.message
                                    ),
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

                    let (typed_patterns, mut bindings) =
                        check_patterns_against_types(patterns, &tuple_types, env, ctx)?;

                    // Handle rest binding: rest @ .. binds to tuple of remaining elements
                    if let Some(name) = rest_binding {
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
                        bindings.insert(name.clone(), rest_ty);
                    }

                    Ok((
                        TypedPattern::TuplePrefix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding.clone(),
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
                    let (typed_patterns, mut bindings) =
                        check_patterns_against_types(patterns, &tuple_types[start_idx..], env, ctx)?;

                    // Handle rest binding: rest @ .. binds to tuple of leading elements
                    if let Some(name) = rest_binding {
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
                        bindings.insert(name.clone(), rest_ty);
                    }

                    Ok((
                        TypedPattern::TupleSuffix {
                            patterns: typed_patterns,
                            rest_binding: rest_binding.clone(),
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
                    let (prefix_typed, mut bindings) =
                        check_patterns_against_types(prefix, &tuple_types, env, ctx)?;

                    // Suffix patterns match from the end
                    let suffix_start = tuple_types.len() - suffix.len();
                    let (suffix_typed, suffix_bindings) =
                        check_patterns_against_types(suffix, &tuple_types[suffix_start..], env, ctx)?;
                    bindings.extend(suffix_bindings);

                    // Handle rest binding: rest @ .. binds to tuple of middle elements
                    if let Some(name) = rest_binding {
                        if !is_snake_case(name) {
                            return Err(TypeError {
                                message: format!(
                                    "variable '{}' should be snake_case (e.g., '{}')",
                                    name,
                                    to_snake_case(name)
                                ),
                            });
                        }
                        let rest_types: Vec<Type> = tuple_types[prefix.len()..suffix_start].to_vec();
                        let rest_ty = Type::Tuple(rest_types);
                        bindings.insert(name.clone(), rest_ty);
                    }

                    Ok((
                        TypedPattern::TuplePrefixSuffix {
                            prefix: prefix_typed,
                            suffix: suffix_typed,
                            rest_binding: rest_binding.clone(),
                            total_len: tuple_types.len(),
                        },
                        bindings,
                    ))
                }
            }
        }

        Pattern::Struct(struct_pattern) => {
            // Get struct path and fields based on pattern variant
            let (path, field_patterns, is_partial) = match struct_pattern {
                StructPattern::Exact { path, fields } => (path, fields, false),
                StructPattern::Partial { path, fields } => (path, fields, true),
            };

            // Extract struct name from path (must be single segment for now)
            let struct_name = path.as_simple().ok_or_else(|| TypeError {
                message: format!("struct patterns don't support qualified paths yet: {}", path),
            })?;

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
                    path: QualifiedPath::simple(struct_name.to_string()),
                    fields: typed_fields,
                }
            } else {
                TypedPattern::StructExact {
                    path: QualifiedPath::simple(struct_name.to_string()),
                    fields: typed_fields,
                }
            };

            Ok((typed_pattern, all_bindings))
        }

        Pattern::Enum(enum_pattern) => {
            check_enum_pattern(enum_pattern, scrutinee_ty, env, ctx)
        }

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
            let (typed_pattern, mut bindings) = check_pattern(pattern, scrutinee_ty, env, ctx)?;

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

/// Check an enum pattern
fn check_enum_pattern(
    enum_pattern: &EnumPattern,
    scrutinee_ty: &Type,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    let EnumPattern { path, fields } = enum_pattern;

    // Extract enum_name and variant_name from path
    let (enum_name, variant_name) = match path.segments.as_slice() {
        [e, v] => (e.as_str(), v.as_str()),
        _ => {
            return Err(TypeError {
                message: format!("invalid enum pattern path: {}", path),
            });
        }
    };

    // Look up the enum definition
    let enum_type = env.enums.get(enum_name).ok_or_else(|| TypeError {
        message: format!("unknown enum in pattern: {}", enum_name),
    })?;

    // Find the variant
    let variant_type = enum_type
        .variants
        .iter()
        .find(|(name, _)| name == variant_name)
        .map(|(_, vt)| vt)
        .ok_or_else(|| TypeError {
            message: format!("enum {} has no variant {}", enum_name, variant_name),
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

    // Check the pattern fields based on variant kind
    match (fields, variant_type) {
        (EnumPatternFields::Unit, EnumVariantType::Unit) => Ok((
            TypedPattern::EnumUnit {
                path: QualifiedPath::new(vec![
                    enum_name.to_string(),
                    variant_name.to_string(),
                ]),
            },
            HashMap::new(),
        )),

        (EnumPatternFields::Tuple(tuple_pattern), EnumVariantType::Tuple(expected_types)) => {
            let resolved_types: Vec<Type> = expected_types
                .iter()
                .map(|t| ctx.resolve(&substitute_type_vars(t, &instantiation)))
                .collect();

            check_enum_tuple_pattern(
                enum_name,
                variant_name,
                tuple_pattern,
                &resolved_types,
                env,
                ctx,
            )
        }

        (
            EnumPatternFields::Struct { fields, is_partial },
            EnumVariantType::Struct(expected_fields),
        ) => {
            let resolved_fields: Vec<(String, Type)> = expected_fields
                .iter()
                .map(|(n, t)| (n.clone(), ctx.resolve(&substitute_type_vars(t, &instantiation))))
                .collect();

            check_enum_struct_pattern(
                enum_name,
                variant_name,
                fields,
                *is_partial,
                &resolved_fields,
                env,
                ctx,
            )
        }

        (EnumPatternFields::Unit, _) => Err(TypeError {
            message: format!(
                "enum variant {}::{} is not a unit variant",
                enum_name, variant_name
            ),
        }),

        (EnumPatternFields::Tuple(_), _) => Err(TypeError {
            message: format!(
                "enum variant {}::{} is not a tuple variant",
                enum_name, variant_name
            ),
        }),

        (EnumPatternFields::Struct { .. }, _) => Err(TypeError {
            message: format!(
                "enum variant {}::{} is not a struct variant",
                enum_name, variant_name
            ),
        }),
    }
}

/// Check an enum tuple variant pattern
fn check_enum_tuple_pattern(
    enum_name: &str,
    variant_name: &str,
    tuple_pattern: &TuplePattern,
    expected_types: &[Type],
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
                    path: QualifiedPath::new(vec![
                        enum_name.to_string(),
                        variant_name.to_string(),
                    ]),
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
                check_patterns_against_types(patterns, expected_types, env, ctx)?;
            Ok((
                TypedPattern::EnumTupleExact {
                    path: QualifiedPath::new(vec![
                        enum_name.to_string(),
                        variant_name.to_string(),
                    ]),
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
                check_patterns_against_types(patterns, expected_types, env, ctx)?;

            // Handle rest binding: rest @ .. binds to tuple of remaining elements
            if let Some(name) = rest_binding {
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
                bindings.insert(name.clone(), rest_ty);
            }

            Ok((
                TypedPattern::EnumTuplePrefix {
                    path: QualifiedPath::new(vec![
                        enum_name.to_string(),
                        variant_name.to_string(),
                    ]),
                    patterns: typed_patterns,
                    rest_binding: rest_binding.clone(),
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
            let (typed_patterns, mut bindings) =
                check_patterns_against_types(patterns, &expected_types[start_idx..], env, ctx)?;

            // Handle rest binding: rest @ .. binds to tuple of leading elements
            if let Some(name) = rest_binding {
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
                bindings.insert(name.clone(), rest_ty);
            }

            Ok((
                TypedPattern::EnumTupleSuffix {
                    path: QualifiedPath::new(vec![
                        enum_name.to_string(),
                        variant_name.to_string(),
                    ]),
                    patterns: typed_patterns,
                    rest_binding: rest_binding.clone(),
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
                check_patterns_against_types(prefix, expected_types, env, ctx)?;
            let suffix_start = total_fields - suffix.len();
            let (suffix_typed, suffix_bindings) =
                check_patterns_against_types(suffix, &expected_types[suffix_start..], env, ctx)?;
            bindings.extend(suffix_bindings);

            // Handle rest binding: rest @ .. binds to tuple of middle elements
            if let Some(name) = rest_binding {
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
                bindings.insert(name.clone(), rest_ty);
            }

            Ok((
                TypedPattern::EnumTuplePrefixSuffix {
                    path: QualifiedPath::new(vec![
                        enum_name.to_string(),
                        variant_name.to_string(),
                    ]),
                    prefix: prefix_typed,
                    suffix: suffix_typed,
                    rest_binding: rest_binding.clone(),
                    total_fields,
                },
                bindings,
            ))
        }
    }
}

/// Check an enum struct variant pattern
fn check_enum_struct_pattern(
    enum_name: &str,
    variant_name: &str,
    field_patterns: &[crate::ast::StructFieldPattern],
    is_partial: bool,
    expected_fields: &[(String, Type)],
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedPattern, HashMap<String, Type>), TypeError> {
    // For exact patterns, verify all fields are covered
    if !is_partial {
        let expected_field_names: HashSet<&str> =
            expected_fields.iter().map(|(n, _)| n.as_str()).collect();
        let provided_field_names: HashSet<&str> =
            field_patterns.iter().map(|f| f.field_name.as_str()).collect();

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
            check_pattern(&field_pattern.pattern, field_type, env, ctx)?;
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
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypedMatchArm, TypeError> {
    let (typed_pattern, bindings) = check_pattern(&arm.pattern, scrutinee_ty, env, ctx)?;

    // Create arm environment with pattern bindings
    let mut arm_env = env.clone();
    arm_env
        .locals
        .extend(bindings.into_iter().map(|(n, ty)| (n, TypeScheme::mono(ty))));

    let typed_result = check_with_env(&arm.result, &arm_env, ctx)?;

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

        Pattern::Struct(struct_pattern) => {
            let fields = match struct_pattern {
                StructPattern::Exact { fields, .. } => fields,
                StructPattern::Partial { fields, .. } => fields,
            };
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
        Pattern::Enum(_) => Err("enum patterns may not match all variants".to_string()),
    }
}

/// Check a let binding and return a typed let binding plus the bindings it introduces.
pub fn check_let_binding(
    binding: &LetBinding,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<(TypedLetBinding, HashMap<String, Type>), TypeError> {
    // Check pattern is irrefutable
    check_irrefutable(&binding.pattern).map_err(|msg| TypeError {
        message: format!("refutable pattern in let binding: {}", msg),
    })?;

    // Type check the value
    let typed_value = check_with_env(&binding.value, env, ctx)?;
    let inferred_type = typed_value.ty();

    // If type annotation exists (only allowed on simple Var patterns), unify with inferred type
    let binding_type = if let Some(ref annotation) = binding.type_annotation {
        let declared_type = resolve_type_annotation(annotation, &HashMap::new(), env)?;
        ctx.unify(&inferred_type, &declared_type).map_err(|e| TypeError {
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
    let (typed_pattern, bindings) = check_pattern(&binding.pattern, &binding_type, env, ctx)?;

    Ok((
        TypedLetBinding {
            pattern: typed_pattern,
            value: typed_value,
            ty: binding_type,
        },
        bindings,
    ))
}
