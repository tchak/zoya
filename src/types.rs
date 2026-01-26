use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int32,
    Int64,
    Float,
    Bool,
    Var(String), // Type variable for generics (e.g., T, U)
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int32 => write!(f, "Int32"),
            Type::Int64 => write!(f, "Int64"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::Var(name) => write!(f, "{}", name),
        }
    }
}

/// Function type signature
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionType {
    pub type_params: Vec<String>,
    pub params: Vec<Type>,
    pub return_type: Type,
}

impl FunctionType {
    /// Instantiate a generic function with concrete types
    pub fn instantiate(&self, substitutions: &HashMap<String, Type>) -> FunctionType {
        FunctionType {
            type_params: vec![], // Instantiated function has no type params
            params: self
                .params
                .iter()
                .map(|t| substitute(t, substitutions))
                .collect(),
            return_type: substitute(&self.return_type, substitutions),
        }
    }
}

/// Apply substitutions to a type
fn substitute(ty: &Type, substitutions: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Var(name) => substitutions.get(name).cloned().unwrap_or_else(|| ty.clone()),
        _ => ty.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub message: String,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
