use std::collections::HashMap;

use zoya_ir::{EnumType, EnumVariantType, Type, TypeVarId};

/// Check if a type is numeric (for ordering comparisons)
pub fn is_numeric_type(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::BigInt | Type::Float)
}

/// Get the type signature of a built-in method on a type.
/// Returns (parameter_types, return_type) if the method exists.
pub fn builtin_method(receiver_ty: &Type, method: &str) -> Option<(Vec<Type>, Type)> {
    match (receiver_ty, method) {
        // String methods
        (Type::String, "len") => Some((vec![], Type::Int)),
        (Type::String, "is_empty") => Some((vec![], Type::Bool)),
        (Type::String, "contains") => Some((vec![Type::String], Type::Bool)),
        (Type::String, "starts_with") => Some((vec![Type::String], Type::Bool)),
        (Type::String, "ends_with") => Some((vec![Type::String], Type::Bool)),
        (Type::String, "to_uppercase") => Some((vec![], Type::String)),
        (Type::String, "to_lowercase") => Some((vec![], Type::String)),
        (Type::String, "trim") => Some((vec![], Type::String)),

        // Int methods
        (Type::Int, "abs") => Some((vec![], Type::Int)),
        (Type::Int, "to_string") => Some((vec![], Type::String)),
        (Type::Int, "to_float") => Some((vec![], Type::Float)),
        (Type::Int, "min") => Some((vec![Type::Int], Type::Int)),
        (Type::Int, "max") => Some((vec![Type::Int], Type::Int)),

        // BigInt methods
        (Type::BigInt, "abs") => Some((vec![], Type::BigInt)),
        (Type::BigInt, "to_string") => Some((vec![], Type::String)),
        (Type::BigInt, "min") => Some((vec![Type::BigInt], Type::BigInt)),
        (Type::BigInt, "max") => Some((vec![Type::BigInt], Type::BigInt)),

        // Float methods
        (Type::Float, "abs") => Some((vec![], Type::Float)),
        (Type::Float, "to_string") => Some((vec![], Type::String)),
        (Type::Float, "to_int") => Some((vec![], Type::Int)),
        (Type::Float, "floor") => Some((vec![], Type::Float)),
        (Type::Float, "ceil") => Some((vec![], Type::Float)),
        (Type::Float, "round") => Some((vec![], Type::Float)),
        (Type::Float, "sqrt") => Some((vec![], Type::Float)),
        (Type::Float, "min") => Some((vec![Type::Float], Type::Float)),
        (Type::Float, "max") => Some((vec![Type::Float], Type::Float)),

        // List methods
        (Type::List(_), "len") => Some((vec![], Type::Int)),
        (Type::List(_), "is_empty") => Some((vec![], Type::Bool)),
        (Type::List(elem_ty), "reverse") => Some((vec![], Type::List(elem_ty.clone()))),
        (Type::List(elem_ty), "push") => {
            Some((vec![*elem_ty.clone()], Type::List(elem_ty.clone())))
        }
        (Type::List(elem_ty), "concat") => Some((
            vec![Type::List(elem_ty.clone())],
            Type::List(elem_ty.clone()),
        )),

        _ => None,
    }
}

/// Create built-in enum definitions (Option, Result).
/// These use high type var IDs (starting at 1,000,000) to avoid collision with
/// normal type checking which starts from 0.
pub fn builtin_enums() -> HashMap<String, EnumType> {
    let mut enums = HashMap::new();

    // Option<T> { None, Some(T) }
    // Type var ID 1_000_000 for T
    let option_t_id = TypeVarId(1_000_000);
    enums.insert(
        "Option".to_string(),
        EnumType {
            name: "Option".to_string(),
            type_params: vec!["T".to_string()],
            type_var_ids: vec![option_t_id],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                (
                    "Some".to_string(),
                    EnumVariantType::Tuple(vec![Type::Var(option_t_id)]),
                ),
            ],
        },
    );

    // Result<T, E> { Ok(T), Err(E) }
    // Type var ID 1_000_001 for T, 1_000_002 for E
    let result_t_id = TypeVarId(1_000_001);
    let result_e_id = TypeVarId(1_000_002);
    enums.insert(
        "Result".to_string(),
        EnumType {
            name: "Result".to_string(),
            type_params: vec!["T".to_string(), "E".to_string()],
            type_var_ids: vec![result_t_id, result_e_id],
            variants: vec![
                (
                    "Ok".to_string(),
                    EnumVariantType::Tuple(vec![Type::Var(result_t_id)]),
                ),
                (
                    "Err".to_string(),
                    EnumVariantType::Tuple(vec![Type::Var(result_e_id)]),
                ),
            ],
        },
    );

    enums
}
