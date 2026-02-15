use std::collections::{HashMap, HashSet};

use zoya_ast::TypeAnnotation;
use zoya_ir::{Definition, Type, TypeError, TypeVarId};
use zoya_package::QualifiedPath;

use crate::check::TypeEnv;
use crate::resolution::ResolvedPath;
use crate::unify::{substitute_type_vars, substitute_variant_type_vars};

/// Resolve a type annotation to a concrete Type.
/// `type_param_map` maps source-level type parameter names (like "T") to TypeVarIds.
/// `current_module` is the module context for name resolution.
/// `env` provides access to struct definitions for struct type resolution.
pub fn resolve_type_annotation(
    annotation: &TypeAnnotation,
    type_param_map: &HashMap<String, TypeVarId>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
) -> Result<Type, TypeError> {
    resolve_type_annotation_inner(
        annotation,
        type_param_map,
        current_module,
        env,
        None,
        &mut HashSet::new(),
    )
}

/// Resolve a type annotation to a concrete Type, with an optional `Self` type.
/// Inside impl blocks, `Self` resolves to the target type.
pub fn resolve_type_annotation_with_self(
    annotation: &TypeAnnotation,
    type_param_map: &HashMap<String, TypeVarId>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    self_type: &Type,
) -> Result<Type, TypeError> {
    resolve_type_annotation_inner(
        annotation,
        type_param_map,
        current_module,
        env,
        Some(self_type),
        &mut HashSet::new(),
    )
}

fn resolve_type_annotation_inner(
    annotation: &TypeAnnotation,
    type_param_map: &HashMap<String, TypeVarId>,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    self_type: Option<&Type>,
    expanding_aliases: &mut HashSet<QualifiedPath>,
) -> Result<Type, TypeError> {
    match annotation {
        TypeAnnotation::Named(path) => {
            // Check for built-in types first (only for simple paths)
            if let Some(name) = path.as_simple() {
                if name == "Self" {
                    return match self_type {
                        Some(ty) => Ok(ty.clone()),
                        None => Err(TypeError {
                            message: "`Self` can only be used inside an impl block".to_string(),
                        }),
                    };
                } else if name == "Int" {
                    return Ok(Type::Int);
                } else if name == "BigInt" {
                    return Ok(Type::BigInt);
                } else if name == "Float" {
                    return Ok(Type::Float);
                } else if name == "Bool" {
                    return Ok(Type::Bool);
                } else if name == "String" {
                    return Ok(Type::String);
                } else if let Some(&id) = type_param_map.get(name) {
                    return Ok(Type::Var(id));
                }
            }

            // Use resolve_pattern_path (type annotations never reference locals)
            let resolved = crate::resolution::resolve_pattern_path(
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
                    let name = path
                        .segments
                        .last()
                        .map(|s| s.as_str())
                        .unwrap_or("<unknown>");
                    match def {
                        Definition::Struct(struct_def) => {
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
                                module: struct_def.module.clone(),
                                name: name.to_string(),
                                type_args: vec![],
                                fields: struct_def.fields.clone(),
                            })
                        }
                        Definition::Enum(enum_def) => {
                            // Non-generic enum reference
                            if !enum_def.type_params.is_empty() {
                                return Err(TypeError {
                                    message: format!(
                                        "enum {} requires {} type argument(s)",
                                        name,
                                        enum_def.type_params.len()
                                    ),
                                });
                            }
                            // Non-generic enum: use variants as-is
                            Ok(Type::Enum {
                                module: enum_def.module.clone(),
                                name: name.to_string(),
                                type_args: vec![],
                                variants: enum_def.variants.clone(),
                            })
                        }
                        Definition::TypeAlias(alias_def) => {
                            // Non-generic type alias reference
                            if !alias_def.type_params.is_empty() {
                                return Err(TypeError {
                                    message: format!(
                                        "type alias {} requires {} type argument(s)",
                                        name,
                                        alias_def.type_params.len()
                                    ),
                                });
                            }
                            // Cycle detection
                            if !expanding_aliases.insert(qualified_path.clone()) {
                                return Err(TypeError {
                                    message: format!(
                                        "circular type alias detected: {}",
                                        qualified_path
                                    ),
                                });
                            }
                            let result = Ok(alias_def.typ.clone());
                            expanding_aliases.remove(&qualified_path);
                            result
                        }
                        Definition::Function(_)
                        | Definition::EnumVariant(..)
                        | Definition::Module(..)
                        | Definition::ImplMethod(_) => Err(TypeError {
                            message: format!(
                                "{} '{}' is not a type",
                                def.kind_name(),
                                qualified_path
                            ),
                        }),
                    }
                }
                ResolvedPath::Local { name, .. } => Err(TypeError {
                    message: format!("variable '{}' is not a type", name),
                }),
            }
        }
        TypeAnnotation::Parameterized(path, params) => {
            // Check for built-in generic types first (only for simple paths)
            if let Some(name) = path.as_simple()
                && name == "List"
            {
                if params.len() != 1 {
                    return Err(TypeError {
                        message: "List requires exactly one type parameter".to_string(),
                    });
                }
                let elem_type = resolve_type_annotation_inner(
                    &params[0],
                    type_param_map,
                    current_module,
                    env,
                    self_type,
                    expanding_aliases,
                )?;
                return Ok(Type::List(Box::new(elem_type)));
            }

            if let Some(name) = path.as_simple()
                && name == "Set"
            {
                if params.len() != 1 {
                    return Err(TypeError {
                        message: "Set requires exactly one type parameter".to_string(),
                    });
                }
                let elem_type = resolve_type_annotation_inner(
                    &params[0],
                    type_param_map,
                    current_module,
                    env,
                    self_type,
                    expanding_aliases,
                )?;
                return Ok(Type::Set(Box::new(elem_type)));
            }

            if let Some(name) = path.as_simple()
                && name == "Dict"
            {
                if params.len() != 2 {
                    return Err(TypeError {
                        message: "Dict requires exactly two type parameters".to_string(),
                    });
                }
                let key_type = resolve_type_annotation_inner(
                    &params[0],
                    type_param_map,
                    current_module,
                    env,
                    self_type,
                    expanding_aliases,
                )?;
                let val_type = resolve_type_annotation_inner(
                    &params[1],
                    type_param_map,
                    current_module,
                    env,
                    self_type,
                    expanding_aliases,
                )?;
                return Ok(Type::Dict(Box::new(key_type), Box::new(val_type)));
            }

            // Use resolve_pattern_path (type annotations never reference locals)
            let resolved = crate::resolution::resolve_pattern_path(
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
                    let name = path
                        .segments
                        .last()
                        .map(|s| s.as_str())
                        .unwrap_or("<unknown>");
                    match def {
                        Definition::Struct(struct_def) => {
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
                                .map(|p| {
                                    resolve_type_annotation_inner(
                                        p,
                                        type_param_map,
                                        current_module,
                                        env,
                                        self_type,
                                        expanding_aliases,
                                    )
                                })
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
                                module: struct_def.module.clone(),
                                name: name.to_string(),
                                type_args,
                                fields,
                            })
                        }
                        Definition::Enum(enum_def) => {
                            // Generic enum reference
                            if params.len() != enum_def.type_params.len() {
                                return Err(TypeError {
                                    message: format!(
                                        "enum {} expects {} type argument(s), got {}",
                                        name,
                                        enum_def.type_params.len(),
                                        params.len()
                                    ),
                                });
                            }
                            let type_args = params
                                .iter()
                                .map(|p| {
                                    resolve_type_annotation_inner(
                                        p,
                                        type_param_map,
                                        current_module,
                                        env,
                                        self_type,
                                        expanding_aliases,
                                    )
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            // Substitute type args into variant types
                            let mut subst = HashMap::new();
                            for (id, arg) in enum_def.type_var_ids.iter().zip(type_args.iter()) {
                                subst.insert(*id, arg.clone());
                            }
                            let variants = enum_def
                                .variants
                                .iter()
                                .map(|(n, vt)| {
                                    (n.clone(), substitute_variant_type_vars(vt, &subst))
                                })
                                .collect();
                            Ok(Type::Enum {
                                module: enum_def.module.clone(),
                                name: name.to_string(),
                                type_args,
                                variants,
                            })
                        }
                        Definition::TypeAlias(alias_def) => {
                            // Generic type alias reference
                            if params.len() != alias_def.type_params.len() {
                                return Err(TypeError {
                                    message: format!(
                                        "type alias {} expects {} type argument(s), got {}",
                                        name,
                                        alias_def.type_params.len(),
                                        params.len()
                                    ),
                                });
                            }
                            // Cycle detection
                            if !expanding_aliases.insert(qualified_path.clone()) {
                                return Err(TypeError {
                                    message: format!(
                                        "circular type alias detected: {}",
                                        qualified_path
                                    ),
                                });
                            }
                            let type_args = params
                                .iter()
                                .map(|p| {
                                    resolve_type_annotation_inner(
                                        p,
                                        type_param_map,
                                        current_module,
                                        env,
                                        self_type,
                                        expanding_aliases,
                                    )
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            // Substitute type args into the underlying type
                            let mut subst = HashMap::new();
                            for (id, arg) in alias_def.type_var_ids.iter().zip(type_args.iter()) {
                                subst.insert(*id, arg.clone());
                            }
                            expanding_aliases.remove(&qualified_path);
                            Ok(substitute_type_vars(&alias_def.typ, &subst))
                        }
                        Definition::Function(_)
                        | Definition::EnumVariant(..)
                        | Definition::Module(..)
                        | Definition::ImplMethod(_) => Err(TypeError {
                            message: format!(
                                "{} '{}' is not a type",
                                def.kind_name(),
                                qualified_path
                            ),
                        }),
                    }
                }
                ResolvedPath::Local { name, .. } => Err(TypeError {
                    message: format!("variable '{}' is not a type", name),
                }),
            }
        }
        TypeAnnotation::Tuple(params) => {
            let mut types = Vec::new();
            for param in params {
                types.push(resolve_type_annotation_inner(
                    param,
                    type_param_map,
                    current_module,
                    env,
                    self_type,
                    expanding_aliases,
                )?);
            }
            Ok(Type::Tuple(types))
        }
        TypeAnnotation::Function(params, ret) => {
            let mut param_types = Vec::new();
            for param in params {
                param_types.push(resolve_type_annotation_inner(
                    param,
                    type_param_map,
                    current_module,
                    env,
                    self_type,
                    expanding_aliases,
                )?);
            }
            let ret_type = resolve_type_annotation_inner(
                ret,
                type_param_map,
                current_module,
                env,
                self_type,
                expanding_aliases,
            )?;
            Ok(Type::Function {
                params: param_types,
                ret: Box::new(ret_type),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::{Path, PathPrefix, TypeAnnotation, Visibility};
    use zoya_ir::{
        Definition, EnumType, EnumVariantType, QualifiedPath, StructType, StructTypeKind,
        TypeAliasType,
    };

    fn empty_env() -> TypeEnv {
        TypeEnv::default()
    }

    fn empty_map() -> HashMap<String, TypeVarId> {
        HashMap::new()
    }

    fn root() -> QualifiedPath {
        QualifiedPath::root()
    }

    fn qpath(path: &str) -> QualifiedPath {
        QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
    }

    // ========================================================================
    // Basic type resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_int() {
        let annotation = TypeAnnotation::Named(Path::simple("Int".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Int);
    }

    #[test]
    fn test_resolve_bigint() {
        let annotation = TypeAnnotation::Named(Path::simple("BigInt".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::BigInt);
    }

    #[test]
    fn test_resolve_float() {
        let annotation = TypeAnnotation::Named(Path::simple("Float".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Float);
    }

    #[test]
    fn test_resolve_bool() {
        let annotation = TypeAnnotation::Named(Path::simple("Bool".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Bool);
    }

    #[test]
    fn test_resolve_string() {
        let annotation = TypeAnnotation::Named(Path::simple("String".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::String);
    }

    // ========================================================================
    // Unknown type tests
    // ========================================================================

    #[test]
    fn test_resolve_unknown_type() {
        let annotation = TypeAnnotation::Named(Path::simple("UnknownType".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
        assert!(err.message.contains("UnknownType"));
    }

    #[test]
    fn test_resolve_unknown_parameterized_type() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("UnknownGeneric".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }

    // ========================================================================
    // Qualified path error tests
    // ========================================================================

    #[test]
    fn test_resolve_qualified_type_path_error() {
        // Qualified paths are now supported, but unknown paths return "unknown path" error
        let annotation = TypeAnnotation::Named(Path {
            prefix: PathPrefix::None,
            segments: vec!["Module".to_string(), "Type".to_string()],
            type_args: None,
        });
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown path"));
    }

    #[test]
    fn test_resolve_qualified_parameterized_type_path_error() {
        // Qualified paths are now supported, but unknown paths return "unknown path" error
        let annotation = TypeAnnotation::Parameterized(
            Path {
                prefix: PathPrefix::None,
                segments: vec!["Module".to_string(), "Container".to_string()],
                type_args: None,
            },
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown path"));
    }

    // ========================================================================
    // Type parameter resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_type_param() {
        let mut type_param_map = HashMap::new();
        type_param_map.insert("T".to_string(), TypeVarId(42));

        let annotation = TypeAnnotation::Named(Path::simple("T".to_string()));
        let result = resolve_type_annotation(&annotation, &type_param_map, &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Var(TypeVarId(42)));
    }

    // ========================================================================
    // List type tests
    // ========================================================================

    #[test]
    fn test_resolve_list_int() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("List".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::List(Box::new(Type::Int)));
    }

    #[test]
    fn test_resolve_list_wrong_param_count_zero() {
        let annotation = TypeAnnotation::Parameterized(Path::simple("List".to_string()), vec![]);
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("List requires exactly one type parameter")
        );
    }

    #[test]
    fn test_resolve_list_wrong_param_count_two() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("List".to_string()),
            vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
            ],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("List requires exactly one type parameter")
        );
    }

    // ========================================================================
    // Dict type tests
    // ========================================================================

    #[test]
    fn test_resolve_dict() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Dict".to_string()),
            vec![
                TypeAnnotation::Named(Path::simple("String".to_string())),
                TypeAnnotation::Named(Path::simple("Int".to_string())),
            ],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Type::Dict(Box::new(Type::String), Box::new(Type::Int))
        );
    }

    #[test]
    fn test_resolve_dict_wrong_param_count_one() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Dict".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("Dict requires exactly two type parameters")
        );
    }

    #[test]
    fn test_resolve_dict_wrong_param_count_zero() {
        let annotation = TypeAnnotation::Parameterized(Path::simple("Dict".to_string()), vec![]);
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("Dict requires exactly two type parameters")
        );
    }

    #[test]
    fn test_resolve_dict_wrong_param_count_three() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Dict".to_string()),
            vec![
                TypeAnnotation::Named(Path::simple("String".to_string())),
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("Bool".to_string())),
            ],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("Dict requires exactly two type parameters")
        );
    }

    #[test]
    fn test_resolve_nested_list() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("List".to_string()),
            vec![TypeAnnotation::Parameterized(
                Path::simple("List".to_string()),
                vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
            )],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Type::List(Box::new(Type::List(Box::new(Type::Int))))
        );
    }

    // ========================================================================
    // Struct type resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_non_generic_struct() {
        let mut env = empty_env();
        env.register(
            qpath("root::Point"),
            Definition::Struct(StructType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                kind: StructTypeKind::Named,
                fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
            }),
        );

        let annotation = TypeAnnotation::Named(Path::simple("Point".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Struct {
                name,
                type_args,
                fields,
                ..
            } => {
                assert_eq!(name, "Point");
                assert!(type_args.is_empty());
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("Expected struct type"),
        }
    }

    #[test]
    fn test_resolve_struct_requires_type_args() {
        let mut env = empty_env();
        env.register(
            qpath("root::Container"),
            Definition::Struct(StructType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "Container".to_string(),
                type_params: vec!["T".to_string()],
                type_var_ids: vec![TypeVarId(1)],
                kind: StructTypeKind::Named,
                fields: vec![("value".to_string(), Type::Var(TypeVarId(1)))],
            }),
        );

        let annotation = TypeAnnotation::Named(Path::simple("Container".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("struct Container requires 1 type argument")
        );
    }

    #[test]
    fn test_resolve_struct_wrong_type_arg_count() {
        let mut env = empty_env();
        env.register(
            qpath("root::Pair"),
            Definition::Struct(StructType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "Pair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                type_var_ids: vec![TypeVarId(1), TypeVarId(2)],
                kind: StructTypeKind::Named,
                fields: vec![
                    ("first".to_string(), Type::Var(TypeVarId(1))),
                    ("second".to_string(), Type::Var(TypeVarId(2))),
                ],
            }),
        );

        // Too few type args
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Pair".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("struct Pair expects 2 type argument(s), got 1")
        );
    }

    #[test]
    fn test_resolve_generic_struct_with_type_args() {
        let mut env = empty_env();
        env.register(
            qpath("root::Container"),
            Definition::Struct(StructType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "Container".to_string(),
                type_params: vec!["T".to_string()],
                type_var_ids: vec![TypeVarId(1)],
                kind: StructTypeKind::Named,
                fields: vec![("value".to_string(), Type::Var(TypeVarId(1)))],
            }),
        );

        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Container".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Struct {
                name,
                type_args,
                fields,
                ..
            } => {
                assert_eq!(name, "Container");
                assert_eq!(type_args, vec![Type::Int]);
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].1, Type::Int); // Field type is substituted
            }
            _ => panic!("Expected struct type"),
        }
    }

    // ========================================================================
    // Enum type resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_non_generic_enum() {
        let mut env = empty_env();
        env.register(
            qpath("root::Status"),
            Definition::Enum(EnumType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "Status".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                variants: vec![
                    ("Ok".to_string(), EnumVariantType::Unit),
                    ("Error".to_string(), EnumVariantType::Unit),
                ],
            }),
        );

        let annotation = TypeAnnotation::Named(Path::simple("Status".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Enum {
                name,
                type_args,
                variants,
                ..
            } => {
                assert_eq!(name, "Status");
                assert!(type_args.is_empty());
                assert_eq!(variants.len(), 2);
            }
            _ => panic!("Expected enum type"),
        }
    }

    #[test]
    fn test_resolve_enum_requires_type_args() {
        let mut env = empty_env();
        env.register(
            qpath("root::Option"),
            Definition::Enum(EnumType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
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
            }),
        );

        let annotation = TypeAnnotation::Named(Path::simple("Option".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("enum Option requires 1 type argument"));
    }

    #[test]
    fn test_resolve_enum_wrong_type_arg_count() {
        let mut env = empty_env();
        env.register(
            qpath("root::Result"),
            Definition::Enum(EnumType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "Result".to_string(),
                type_params: vec!["T".to_string(), "E".to_string()],
                type_var_ids: vec![TypeVarId(1), TypeVarId(2)],
                variants: vec![
                    (
                        "Ok".to_string(),
                        EnumVariantType::Tuple(vec![Type::Var(TypeVarId(1))]),
                    ),
                    (
                        "Err".to_string(),
                        EnumVariantType::Tuple(vec![Type::Var(TypeVarId(2))]),
                    ),
                ],
            }),
        );

        // Too many type args
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Result".to_string()),
            vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
                TypeAnnotation::Named(Path::simple("Bool".to_string())),
            ],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("enum Result expects 2 type argument(s), got 3")
        );
    }

    #[test]
    fn test_resolve_generic_enum_with_type_args() {
        let mut env = empty_env();
        env.register(
            qpath("root::Option"),
            Definition::Enum(EnumType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
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
            }),
        );

        let annotation = TypeAnnotation::Parameterized(
            Path::simple("Option".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Enum {
                name,
                type_args,
                variants,
                ..
            } => {
                assert_eq!(name, "Option");
                assert_eq!(type_args, vec![Type::Int]);
                // Check Some variant has substituted type
                assert!(matches!(&variants[1].1, EnumVariantType::Tuple(v) if v[0] == Type::Int));
            }
            _ => panic!("Expected enum type"),
        }
    }

    // ========================================================================
    // Type alias resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_non_generic_type_alias() {
        let mut env = empty_env();
        env.register(
            qpath("root::IntList"),
            Definition::TypeAlias(TypeAliasType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "IntList".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                typ: Type::List(Box::new(Type::Int)),
            }),
        );

        let annotation = TypeAnnotation::Named(Path::simple("IntList".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::List(Box::new(Type::Int)));
    }

    #[test]
    fn test_resolve_type_alias_requires_type_args() {
        let mut env = empty_env();
        env.register(
            qpath("root::MyList"),
            Definition::TypeAlias(TypeAliasType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "MyList".to_string(),
                type_params: vec!["T".to_string()],
                type_var_ids: vec![TypeVarId(1)],
                typ: Type::List(Box::new(Type::Var(TypeVarId(1)))),
            }),
        );

        let annotation = TypeAnnotation::Named(Path::simple("MyList".to_string()));
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("type alias MyList requires 1 type argument")
        );
    }

    #[test]
    fn test_resolve_type_alias_wrong_type_arg_count() {
        let mut env = empty_env();
        env.register(
            qpath("root::MyPair"),
            Definition::TypeAlias(TypeAliasType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "MyPair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                type_var_ids: vec![TypeVarId(1), TypeVarId(2)],
                typ: Type::Tuple(vec![Type::Var(TypeVarId(1)), Type::Var(TypeVarId(2))]),
            }),
        );

        let annotation = TypeAnnotation::Parameterized(
            Path::simple("MyPair".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("type alias MyPair expects 2 type argument(s), got 1")
        );
    }

    #[test]
    fn test_resolve_generic_type_alias_with_type_args() {
        let mut env = empty_env();
        env.register(
            qpath("root::MyList"),
            Definition::TypeAlias(TypeAliasType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                name: "MyList".to_string(),
                type_params: vec!["T".to_string()],
                type_var_ids: vec![TypeVarId(1)],
                typ: Type::List(Box::new(Type::Var(TypeVarId(1)))),
            }),
        );

        let annotation = TypeAnnotation::Parameterized(
            Path::simple("MyList".to_string()),
            vec![TypeAnnotation::Named(Path::simple("String".to_string()))],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &env);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::List(Box::new(Type::String)));
    }

    // ========================================================================
    // Tuple type resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_empty_tuple() {
        let annotation = TypeAnnotation::Tuple(vec![]);
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Type::Tuple(vec![]));
    }

    #[test]
    fn test_resolve_tuple_types() {
        let annotation = TypeAnnotation::Tuple(vec![
            TypeAnnotation::Named(Path::simple("Int".to_string())),
            TypeAnnotation::Named(Path::simple("String".to_string())),
            TypeAnnotation::Named(Path::simple("Bool".to_string())),
        ]);
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Type::Tuple(vec![Type::Int, Type::String, Type::Bool])
        );
    }

    #[test]
    fn test_resolve_nested_tuple() {
        let annotation = TypeAnnotation::Tuple(vec![
            TypeAnnotation::Named(Path::simple("Int".to_string())),
            TypeAnnotation::Tuple(vec![
                TypeAnnotation::Named(Path::simple("String".to_string())),
                TypeAnnotation::Named(Path::simple("Bool".to_string())),
            ]),
        ]);
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Type::Tuple(vec![Type::Int, Type::Tuple(vec![Type::String, Type::Bool])])
        );
    }

    // ========================================================================
    // Function type resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_function_type_no_params() {
        let annotation = TypeAnnotation::Function(
            vec![],
            Box::new(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Function { params, ret } => {
                assert!(params.is_empty());
                assert_eq!(*ret, Type::Int);
            }
            _ => panic!("Expected function type"),
        }
    }

    #[test]
    fn test_resolve_function_type_with_params() {
        let annotation = TypeAnnotation::Function(
            vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
            ],
            Box::new(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Function { params, ret } => {
                assert_eq!(params, vec![Type::Int, Type::String]);
                assert_eq!(*ret, Type::Bool);
            }
            _ => panic!("Expected function type"),
        }
    }

    #[test]
    fn test_resolve_nested_function_type() {
        // (Int -> Bool) -> String
        let annotation = TypeAnnotation::Function(
            vec![TypeAnnotation::Function(
                vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
                Box::new(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
            )],
            Box::new(TypeAnnotation::Named(Path::simple("String".to_string()))),
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Function { params, ret } => {
                assert_eq!(params.len(), 1);
                match &params[0] {
                    Type::Function {
                        params: inner_params,
                        ret: inner_ret,
                    } => {
                        assert_eq!(inner_params, &vec![Type::Int]);
                        assert_eq!(**inner_ret, Type::Bool);
                    }
                    _ => panic!("Expected inner function type"),
                }
                assert_eq!(*ret, Type::String);
            }
            _ => panic!("Expected function type"),
        }
    }

    #[test]
    fn test_resolve_function_type_with_type_params() {
        let mut type_param_map = HashMap::new();
        type_param_map.insert("T".to_string(), TypeVarId(99));

        let annotation = TypeAnnotation::Function(
            vec![TypeAnnotation::Named(Path::simple("T".to_string()))],
            Box::new(TypeAnnotation::Named(Path::simple("T".to_string()))),
        );
        let result = resolve_type_annotation(&annotation, &type_param_map, &root(), &empty_env());
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Function { params, ret } => {
                assert_eq!(params, vec![Type::Var(TypeVarId(99))]);
                assert_eq!(*ret, Type::Var(TypeVarId(99)));
            }
            _ => panic!("Expected function type"),
        }
    }

    // ========================================================================
    // Complex/nested type resolution tests
    // ========================================================================

    #[test]
    fn test_resolve_list_of_tuples() {
        let annotation = TypeAnnotation::Parameterized(
            Path::simple("List".to_string()),
            vec![TypeAnnotation::Tuple(vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
            ])],
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Type::List(Box::new(Type::Tuple(vec![Type::Int, Type::String])))
        );
    }

    #[test]
    fn test_resolve_function_returning_list() {
        let annotation = TypeAnnotation::Function(
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
            Box::new(TypeAnnotation::Parameterized(
                Path::simple("List".to_string()),
                vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
            )),
        );
        let result = resolve_type_annotation(&annotation, &empty_map(), &root(), &empty_env());
        assert!(result.is_ok());
        match result.unwrap() {
            Type::Function { params, ret } => {
                assert_eq!(params, vec![Type::Int]);
                assert_eq!(*ret, Type::List(Box::new(Type::Int)));
            }
            _ => panic!("Expected function type"),
        }
    }
}
