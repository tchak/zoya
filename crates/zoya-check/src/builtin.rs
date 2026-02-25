use zoya_ir::Type;

/// Check if a type is numeric (for ordering comparisons)
pub fn is_numeric_type(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::BigInt | Type::Float)
}

/// For a primitive type, return the (module_name, type_name) pair used to
/// locate its impl methods in the std package.
pub fn primitive_method_module(ty: &Type) -> Option<(&'static str, &'static str)> {
    match ty {
        Type::Int => Some(("int", "Int")),
        Type::BigInt => Some(("bigint", "BigInt")),
        Type::Float => Some(("float", "Float")),
        Type::String => Some(("string", "String")),
        Type::List(_) => Some(("list", "List")),
        Type::Set(_) => Some(("set", "Set")),
        Type::Dict(_, _) => Some(("dict", "Dict")),
        Type::Task(_) => Some(("task", "Task")),
        Type::Bytes => Some(("bytes", "Bytes")),
        _ => None,
    }
}

/// Map a primitive type name to the std module that contains its impl block.
pub fn primitive_module_for_name(name: &str) -> Option<&'static str> {
    match name {
        "Int" => Some("int"),
        "BigInt" => Some("bigint"),
        "Float" => Some("float"),
        "String" => Some("string"),
        "List" => Some("list"),
        "Set" => Some("set"),
        "Dict" => Some("dict"),
        "Task" => Some("task"),
        "Bytes" => Some("bytes"),
        _ => None,
    }
}
