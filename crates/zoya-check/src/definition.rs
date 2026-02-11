use std::collections::HashMap;

use zoya_ast::{EnumDef, FunctionDef, StructDef, StructKind, TypeAliasDef};
use zoya_ir::{
    EnumType, EnumVariantType, FunctionType, StructType, StructTypeKind, Type, TypeAliasType,
    TypeError,
};
use zoya_package::QualifiedPath;

use crate::check::TypeEnv;
use crate::type_resolver::resolve_type_annotation;
use crate::unify::UnifyCtx;
use zoya_naming::{is_pascal_case, to_pascal_case};

/// Extract function type from a function definition (for adding to env).
/// Uses a separate UnifyCtx to create fresh type variables for the signature.
pub fn function_type_from_def(
    func: &FunctionDef,
    current_module: &QualifiedPath,
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
        let ty = resolve_type_annotation(&param.typ, &type_param_map, current_module, env)?;
        param_types.push(ty);
    }

    let return_type = if let Some(ref annotation) = func.return_type {
        resolve_type_annotation(annotation, &type_param_map, current_module, env)?
    } else {
        // Create a fresh type variable for inferred return type
        ctx.fresh_var()
    };

    Ok(FunctionType {
        visibility: func.visibility,
        module: current_module.clone(),
        type_params: func.type_params.clone(),
        type_var_ids,
        params: param_types,
        return_type,
    })
}

/// Extract struct type from a struct definition (for adding to env).
pub fn struct_type_from_def(
    def: &StructDef,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<StructType, TypeError> {
    // Check struct name is PascalCase
    if !is_pascal_case(&def.name) {
        return Err(TypeError {
            message: format!(
                "struct name '{}' should be PascalCase (e.g., '{}')",
                def.name,
                to_pascal_case(&def.name)
            ),
        });
    }

    // Unit structs cannot have type parameters
    if matches!(def.kind, StructKind::Unit) && !def.type_params.is_empty() {
        return Err(TypeError {
            message: format!("unit struct '{}' cannot have type parameters", def.name),
        });
    }

    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    let mut type_var_ids = Vec::new();
    for name in &def.type_params {
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
            type_var_ids.push(id);
        }
    }

    // Resolve fields based on struct kind
    let (kind, fields) = match &def.kind {
        StructKind::Unit => (StructTypeKind::Unit, vec![]),
        StructKind::Tuple(types) => {
            let mut fields = Vec::new();
            for (i, typ) in types.iter().enumerate() {
                let ty = resolve_type_annotation(typ, &type_param_map, current_module, env)?;
                fields.push((format!("${}", i), ty));
            }
            (StructTypeKind::Tuple, fields)
        }
        StructKind::Named(field_defs) => {
            let mut fields = Vec::new();
            for field in field_defs {
                let ty = resolve_type_annotation(&field.typ, &type_param_map, current_module, env)?;
                fields.push((field.name.clone(), ty));
            }
            (StructTypeKind::Named, fields)
        }
    };

    Ok(StructType {
        visibility: def.visibility,
        module: current_module.clone(),
        name: def.name.clone(),
        type_params: def.type_params.clone(),
        type_var_ids,
        kind,
        fields,
    })
}

/// Extract enum type from an enum definition (for adding to env).
pub fn enum_type_from_def(
    def: &EnumDef,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<EnumType, TypeError> {
    // Check enum name is PascalCase
    if !is_pascal_case(&def.name) {
        return Err(TypeError {
            message: format!(
                "enum name '{}' should be PascalCase (e.g., '{}')",
                def.name,
                to_pascal_case(&def.name)
            ),
        });
    }

    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    let mut type_var_ids = Vec::new();
    for name in &def.type_params {
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
            type_var_ids.push(id);
        }
    }

    // Resolve variant types
    let mut variants = Vec::new();
    for variant in &def.variants {
        // Check variant name is PascalCase
        if !is_pascal_case(&variant.name) {
            return Err(TypeError {
                message: format!(
                    "enum variant '{}' should be PascalCase (e.g., '{}')",
                    variant.name,
                    to_pascal_case(&variant.name)
                ),
            });
        }
        let variant_type = match &variant.kind {
            zoya_ast::EnumVariantKind::Unit => EnumVariantType::Unit,
            zoya_ast::EnumVariantKind::Tuple(types) => {
                let resolved_types = types
                    .iter()
                    .map(|t| resolve_type_annotation(t, &type_param_map, current_module, env))
                    .collect::<Result<Vec<_>, _>>()?;
                EnumVariantType::Tuple(resolved_types)
            }
            zoya_ast::EnumVariantKind::Struct(fields) => {
                let resolved_fields = fields
                    .iter()
                    .map(|f| {
                        let ty =
                            resolve_type_annotation(&f.typ, &type_param_map, current_module, env)?;
                        Ok((f.name.clone(), ty))
                    })
                    .collect::<Result<Vec<_>, TypeError>>()?;
                EnumVariantType::Struct(resolved_fields)
            }
        };
        variants.push((variant.name.clone(), variant_type));
    }

    Ok(EnumType {
        visibility: def.visibility,
        module: current_module.clone(),
        name: def.name.clone(),
        type_params: def.type_params.clone(),
        type_var_ids,
        variants,
    })
}

/// Extract type alias from a type alias definition (for adding to env).
pub fn type_alias_from_def(
    def: &TypeAliasDef,
    current_module: &QualifiedPath,
    env: &TypeEnv,
    ctx: &mut UnifyCtx,
) -> Result<TypeAliasType, TypeError> {
    // Check type alias name is PascalCase
    if !is_pascal_case(&def.name) {
        return Err(TypeError {
            message: format!(
                "type alias name '{}' should be PascalCase (e.g., '{}')",
                def.name,
                to_pascal_case(&def.name)
            ),
        });
    }

    // Create fresh type variables for type parameters
    let mut type_param_map = HashMap::new();
    let mut type_var_ids = Vec::new();
    for name in &def.type_params {
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
            type_var_ids.push(id);
        }
    }

    // Resolve the underlying type
    let typ = resolve_type_annotation(&def.typ, &type_param_map, current_module, env)?;

    Ok(TypeAliasType {
        visibility: def.visibility,
        module: current_module.clone(),
        name: def.name.clone(),
        type_params: def.type_params.clone(),
        type_var_ids,
        typ,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::{
        EnumVariant, EnumVariantKind, Path, StructFieldDef, StructKind, TypeAnnotation, Visibility,
    };

    fn root() -> QualifiedPath {
        QualifiedPath::root()
    }

    // ========================================================================
    // struct_type_from_def tests
    // ========================================================================

    #[test]
    fn test_struct_valid_name() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Point".to_string(),
            type_params: vec![],
            kind: StructKind::Named(vec![StructFieldDef {
                name: "x".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }]),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "Point");
    }

    #[test]
    fn test_struct_invalid_name_lowercase() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "point".to_string(),
            type_params: vec![],
            kind: StructKind::Unit,
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("should be PascalCase"));
        assert!(err.message.contains("Point"));
    }

    #[test]
    fn test_struct_invalid_name_snake_case() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "my_point".to_string(),
            type_params: vec![],
            kind: StructKind::Unit,
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("should be PascalCase"));
        assert!(err.message.contains("MyPoint"));
    }

    #[test]
    fn test_struct_invalid_type_param_lowercase() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Container".to_string(),
            type_params: vec!["t".to_string()],
            kind: StructKind::Named(vec![StructFieldDef {
                name: "value".to_string(),
                typ: TypeAnnotation::Named(Path::simple("t".to_string())),
            }]),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("type parameter"));
        assert!(err.message.contains("should be PascalCase"));
    }

    #[test]
    fn test_struct_invalid_type_param_snake_case() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Container".to_string(),
            type_params: vec!["my_type".to_string()],
            kind: StructKind::Named(vec![StructFieldDef {
                name: "value".to_string(),
                typ: TypeAnnotation::Named(Path::simple("my_type".to_string())),
            }]),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("type parameter 'my_type'"));
        assert!(err.message.contains("MyType"));
    }

    #[test]
    fn test_struct_with_valid_type_params() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Container".to_string(),
            type_params: vec!["T".to_string(), "U".to_string()],
            kind: StructKind::Named(vec![StructFieldDef {
                name: "value".to_string(),
                typ: TypeAnnotation::Named(Path::simple("T".to_string())),
            }]),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let st = result.unwrap();
        assert_eq!(st.type_params.len(), 2);
        assert_eq!(st.type_var_ids.len(), 2);
    }

    #[test]
    fn test_struct_unit_generic_rejected() {
        let def = StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Phantom".to_string(),
            type_params: vec!["T".to_string()],
            kind: StructKind::Unit,
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = struct_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("unit struct 'Phantom' cannot have type parameters")
        );
    }

    // ========================================================================
    // enum_type_from_def tests
    // ========================================================================

    #[test]
    fn test_enum_valid_name() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Option".to_string(),
            type_params: vec!["T".to_string()],
            variants: vec![
                EnumVariant {
                    name: "None".to_string(),
                    kind: EnumVariantKind::Unit,
                },
                EnumVariant {
                    name: "Some".to_string(),
                    kind: EnumVariantKind::Tuple(vec![TypeAnnotation::Named(Path::simple(
                        "T".to_string(),
                    ))]),
                },
            ],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "Option");
    }

    #[test]
    fn test_enum_invalid_name_lowercase() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "option".to_string(),
            type_params: vec![],
            variants: vec![],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("enum name 'option'"));
        assert!(err.message.contains("should be PascalCase"));
    }

    #[test]
    fn test_enum_invalid_name_snake_case() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "my_enum".to_string(),
            type_params: vec![],
            variants: vec![],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("MyEnum"));
    }

    #[test]
    fn test_enum_invalid_type_param() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Result".to_string(),
            type_params: vec!["ok_type".to_string()],
            variants: vec![],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("type parameter 'ok_type'"));
    }

    #[test]
    fn test_enum_invalid_variant_name_lowercase() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Status".to_string(),
            type_params: vec![],
            variants: vec![EnumVariant {
                name: "ok".to_string(),
                kind: EnumVariantKind::Unit,
            }],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("enum variant 'ok'"));
        assert!(err.message.contains("should be PascalCase"));
    }

    #[test]
    fn test_enum_invalid_variant_name_snake_case() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Status".to_string(),
            type_params: vec![],
            variants: vec![EnumVariant {
                name: "not_found".to_string(),
                kind: EnumVariantKind::Unit,
            }],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("NotFound"));
    }

    #[test]
    fn test_enum_unit_variant() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Status".to_string(),
            type_params: vec![],
            variants: vec![
                EnumVariant {
                    name: "Ok".to_string(),
                    kind: EnumVariantKind::Unit,
                },
                EnumVariant {
                    name: "Error".to_string(),
                    kind: EnumVariantKind::Unit,
                },
            ],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let et = result.unwrap();
        assert_eq!(et.variants.len(), 2);
        assert!(matches!(et.variants[0].1, EnumVariantType::Unit));
        assert!(matches!(et.variants[1].1, EnumVariantType::Unit));
    }

    #[test]
    fn test_enum_tuple_variant() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Message".to_string(),
            type_params: vec![],
            variants: vec![
                EnumVariant {
                    name: "Text".to_string(),
                    kind: EnumVariantKind::Tuple(vec![TypeAnnotation::Named(Path::simple(
                        "String".to_string(),
                    ))]),
                },
                EnumVariant {
                    name: "Pair".to_string(),
                    kind: EnumVariantKind::Tuple(vec![
                        TypeAnnotation::Named(Path::simple("Int".to_string())),
                        TypeAnnotation::Named(Path::simple("Int".to_string())),
                    ]),
                },
            ],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let et = result.unwrap();
        assert!(matches!(&et.variants[0].1, EnumVariantType::Tuple(v) if v.len() == 1));
        assert!(matches!(&et.variants[1].1, EnumVariantType::Tuple(v) if v.len() == 2));
    }

    #[test]
    fn test_enum_struct_variant() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Shape".to_string(),
            type_params: vec![],
            variants: vec![
                EnumVariant {
                    name: "Circle".to_string(),
                    kind: EnumVariantKind::Struct(vec![StructFieldDef {
                        name: "radius".to_string(),
                        typ: TypeAnnotation::Named(Path::simple("Float".to_string())),
                    }]),
                },
                EnumVariant {
                    name: "Rectangle".to_string(),
                    kind: EnumVariantKind::Struct(vec![
                        StructFieldDef {
                            name: "width".to_string(),
                            typ: TypeAnnotation::Named(Path::simple("Float".to_string())),
                        },
                        StructFieldDef {
                            name: "height".to_string(),
                            typ: TypeAnnotation::Named(Path::simple("Float".to_string())),
                        },
                    ]),
                },
            ],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let et = result.unwrap();
        assert!(matches!(&et.variants[0].1, EnumVariantType::Struct(f) if f.len() == 1));
        assert!(matches!(&et.variants[1].1, EnumVariantType::Struct(f) if f.len() == 2));
    }

    #[test]
    fn test_enum_tuple_variant_with_unknown_type() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Container".to_string(),
            type_params: vec![],
            variants: vec![EnumVariant {
                name: "Value".to_string(),
                kind: EnumVariantKind::Tuple(vec![TypeAnnotation::Named(Path::simple(
                    "UnknownType".to_string(),
                ))]),
            }],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }

    #[test]
    fn test_enum_struct_variant_with_unknown_type() {
        let def = EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Container".to_string(),
            type_params: vec![],
            variants: vec![EnumVariant {
                name: "Data".to_string(),
                kind: EnumVariantKind::Struct(vec![StructFieldDef {
                    name: "field".to_string(),
                    typ: TypeAnnotation::Named(Path::simple("UnknownType".to_string())),
                }]),
            }],
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = enum_type_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }

    // ========================================================================
    // type_alias_from_def tests
    // ========================================================================

    #[test]
    fn test_type_alias_valid_name() {
        let def = TypeAliasDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "IntList".to_string(),
            type_params: vec![],
            typ: TypeAnnotation::Parameterized(
                Path::simple("List".to_string()),
                vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
            ),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = type_alias_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "IntList");
    }

    #[test]
    fn test_type_alias_invalid_name_lowercase() {
        let def = TypeAliasDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "intList".to_string(),
            type_params: vec![],
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = type_alias_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("type alias name 'intList'"));
        assert!(err.message.contains("should be PascalCase"));
    }

    #[test]
    fn test_type_alias_invalid_name_snake_case() {
        let def = TypeAliasDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "int_list".to_string(),
            type_params: vec![],
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = type_alias_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("IntList"));
    }

    #[test]
    fn test_type_alias_invalid_type_param() {
        let def = TypeAliasDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Container".to_string(),
            type_params: vec!["elem_type".to_string()],
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = type_alias_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("type parameter 'elem_type'"));
    }

    #[test]
    fn test_type_alias_with_valid_type_params() {
        let def = TypeAliasDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Pair".to_string(),
            type_params: vec!["A".to_string(), "B".to_string()],
            typ: TypeAnnotation::Tuple(vec![
                TypeAnnotation::Named(Path::simple("A".to_string())),
                TypeAnnotation::Named(Path::simple("B".to_string())),
            ]),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = type_alias_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let ta = result.unwrap();
        assert_eq!(ta.type_params.len(), 2);
        assert_eq!(ta.type_var_ids.len(), 2);
    }

    #[test]
    fn test_type_alias_unknown_underlying_type() {
        let def = TypeAliasDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "MyAlias".to_string(),
            type_params: vec![],
            typ: TypeAnnotation::Named(Path::simple("NonExistentType".to_string())),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = type_alias_from_def(&def, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }

    // ========================================================================
    // function_type_from_def tests
    // ========================================================================

    #[test]
    fn test_function_type_simple() {
        let func = FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "add".to_string(),
            type_params: vec![],
            params: vec![
                zoya_ast::Param {
                    pattern: zoya_ast::Pattern::Path(Path::simple("x".to_string())),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                },
                zoya_ast::Param {
                    pattern: zoya_ast::Pattern::Path(Path::simple("y".to_string())),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                },
            ],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: zoya_ast::Expr::Int(0),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = function_type_from_def(&func, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let ft = result.unwrap();
        assert_eq!(ft.visibility, Visibility::Public);
        assert_eq!(ft.params.len(), 2);
        assert_eq!(ft.params[0], Type::Int);
        assert_eq!(ft.params[1], Type::Int);
        assert_eq!(ft.return_type, Type::Int);
    }

    #[test]
    fn test_function_type_generic() {
        let func = FunctionDef {
            attributes: vec![],
            visibility: Visibility::Private,
            name: "identity".to_string(),
            type_params: vec!["T".to_string()],
            params: vec![zoya_ast::Param {
                pattern: zoya_ast::Pattern::Path(Path::simple("x".to_string())),
                typ: TypeAnnotation::Named(Path::simple("T".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("T".to_string()))),
            body: zoya_ast::Expr::Int(0),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = function_type_from_def(&func, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let ft = result.unwrap();
        assert_eq!(ft.visibility, Visibility::Private);
        assert_eq!(ft.type_params.len(), 1);
        assert_eq!(ft.type_var_ids.len(), 1);
        // Param and return type should both reference the same type variable
        assert!(matches!(ft.params[0], Type::Var(_)));
        assert!(matches!(ft.return_type, Type::Var(_)));
    }

    #[test]
    fn test_function_type_no_return_annotation() {
        let func = FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "foo".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: zoya_ast::Expr::Int(0),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = function_type_from_def(&func, &root(), &env, &mut ctx);
        assert!(result.is_ok());
        let ft = result.unwrap();
        // Should have a fresh type variable for return type
        assert!(matches!(ft.return_type, Type::Var(_)));
    }

    #[test]
    fn test_function_type_unknown_param_type() {
        let func = FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "foo".to_string(),
            type_params: vec![],
            params: vec![zoya_ast::Param {
                pattern: zoya_ast::Pattern::Path(Path::simple("x".to_string())),
                typ: TypeAnnotation::Named(Path::simple("UnknownType".to_string())),
            }],
            return_type: None,
            body: zoya_ast::Expr::Int(0),
        };
        let env = TypeEnv::default();
        let mut ctx = UnifyCtx::new();
        let result = function_type_from_def(&func, &root(), &env, &mut ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown identifier"));
    }
}
