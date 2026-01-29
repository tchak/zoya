use std::collections::HashMap;

use crate::ast::TypeAnnotation;
use crate::types::{Type, TypeError, TypeVarId};

use super::{substitute_type_vars, substitute_variant_type_vars, TypeEnv};

/// Resolve a type annotation to a concrete Type.
/// `type_param_map` maps source-level type parameter names (like "T") to TypeVarIds.
/// `env` provides access to struct definitions for struct type resolution.
pub fn resolve_type_annotation(
    annotation: &TypeAnnotation,
    type_param_map: &HashMap<String, TypeVarId>,
    env: &TypeEnv,
) -> Result<Type, TypeError> {
    match annotation {
        TypeAnnotation::Named(path) => {
            // For now, only support simple (single-segment) type names
            let name = path.as_simple().ok_or_else(|| TypeError {
                message: format!("qualified type paths not yet supported: {}", path),
            })?;
            if name == "Int" {
                Ok(Type::Int)
            } else if name == "BigInt" {
                Ok(Type::BigInt)
            } else if name == "Float" {
                Ok(Type::Float)
            } else if name == "Bool" {
                Ok(Type::Bool)
            } else if name == "String" {
                Ok(Type::String)
            } else if let Some(&id) = type_param_map.get(name) {
                Ok(Type::Var(id))
            } else if let Some(struct_def) = env.structs.get(name) {
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
                    name: name.to_string(),
                    type_args: vec![],
                    fields: struct_def.fields.clone(),
                })
            } else if let Some(enum_def) = env.enums.get(name) {
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
                    name: name.to_string(),
                    type_args: vec![],
                    variants: enum_def.variants.clone(),
                })
            } else if let Some(alias_def) = env.type_aliases.get(name) {
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
                // Non-generic alias: return the underlying type as-is
                Ok(alias_def.typ.clone())
            } else {
                Err(TypeError {
                    message: format!("unknown type: {}", name),
                })
            }
        }
        TypeAnnotation::Parameterized(path, params) => {
            // For now, only support simple (single-segment) type names
            let name = path.as_simple().ok_or_else(|| TypeError {
                message: format!("qualified type paths not yet supported: {}", path),
            })?;
            if name == "List" {
                if params.len() != 1 {
                    return Err(TypeError {
                        message: "List requires exactly one type parameter".to_string(),
                    });
                }
                let elem_type = resolve_type_annotation(&params[0], type_param_map, env)?;
                Ok(Type::List(Box::new(elem_type)))
            } else if let Some(struct_def) = env.structs.get(name) {
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
                    .map(|p| resolve_type_annotation(p, type_param_map, env))
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
                    name: name.to_string(),
                    type_args,
                    fields,
                })
            } else if let Some(enum_def) = env.enums.get(name) {
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
                    .map(|p| resolve_type_annotation(p, type_param_map, env))
                    .collect::<Result<Vec<_>, _>>()?;
                // Substitute type args into variant types
                let mut subst = HashMap::new();
                for (id, arg) in enum_def.type_var_ids.iter().zip(type_args.iter()) {
                    subst.insert(*id, arg.clone());
                }
                let variants = enum_def
                    .variants
                    .iter()
                    .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, &subst)))
                    .collect();
                Ok(Type::Enum {
                    name: name.to_string(),
                    type_args,
                    variants,
                })
            } else if let Some(alias_def) = env.type_aliases.get(name) {
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
                let type_args = params
                    .iter()
                    .map(|p| resolve_type_annotation(p, type_param_map, env))
                    .collect::<Result<Vec<_>, _>>()?;
                // Substitute type args into the underlying type
                let mut subst = HashMap::new();
                for (id, arg) in alias_def.type_var_ids.iter().zip(type_args.iter()) {
                    subst.insert(*id, arg.clone());
                }
                Ok(substitute_type_vars(&alias_def.typ, &subst))
            } else {
                Err(TypeError {
                    message: format!("unknown parameterized type: {}", name),
                })
            }
        }
        TypeAnnotation::Tuple(params) => {
            let mut types = Vec::new();
            for param in params {
                types.push(resolve_type_annotation(param, type_param_map, env)?);
            }
            Ok(Type::Tuple(types))
        }
        TypeAnnotation::Function(params, ret) => {
            let mut param_types = Vec::new();
            for param in params {
                param_types.push(resolve_type_annotation(param, type_param_map, env)?);
            }
            let ret_type = resolve_type_annotation(ret, type_param_map, env)?;
            Ok(Type::Function {
                params: param_types,
                ret: Box::new(ret_type),
            })
        }
    }
}
