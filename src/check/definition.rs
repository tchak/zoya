use std::collections::HashMap;

use crate::ast::{EnumDef, FunctionDef, StructDef, TypeAliasDef};
use crate::types::{EnumType, EnumVariantType, FunctionType, StructType, Type, TypeAliasType, TypeError};
use crate::unify::UnifyCtx;

use super::naming::{is_pascal_case, to_pascal_case};
use super::type_resolver::resolve_type_annotation;
use super::TypeEnv;

/// Extract function type from a function definition (for adding to env).
/// Uses a separate UnifyCtx to create fresh type variables for the signature.
pub fn function_type_from_def(
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

/// Extract struct type from a struct definition (for adding to env).
pub fn struct_type_from_def(
    def: &StructDef,
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

/// Extract enum type from an enum definition (for adding to env).
pub fn enum_type_from_def(
    def: &EnumDef,
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
            crate::ast::EnumVariantKind::Unit => EnumVariantType::Unit,
            crate::ast::EnumVariantKind::Tuple(types) => {
                let resolved_types = types
                    .iter()
                    .map(|t| resolve_type_annotation(t, &type_param_map, env))
                    .collect::<Result<Vec<_>, _>>()?;
                EnumVariantType::Tuple(resolved_types)
            }
            crate::ast::EnumVariantKind::Struct(fields) => {
                let resolved_fields = fields
                    .iter()
                    .map(|f| {
                        let ty = resolve_type_annotation(&f.typ, &type_param_map, env)?;
                        Ok((f.name.clone(), ty))
                    })
                    .collect::<Result<Vec<_>, TypeError>>()?;
                EnumVariantType::Struct(resolved_fields)
            }
        };
        variants.push((variant.name.clone(), variant_type));
    }

    Ok(EnumType {
        name: def.name.clone(),
        type_params: def.type_params.clone(),
        type_var_ids,
        variants,
    })
}

/// Extract type alias from a type alias definition (for adding to env).
pub fn type_alias_from_def(
    def: &TypeAliasDef,
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
    let typ = resolve_type_annotation(&def.typ, &type_param_map, env)?;

    Ok(TypeAliasType {
        name: def.name.clone(),
        type_params: def.type_params.clone(),
        type_var_ids,
        typ,
    })
}
