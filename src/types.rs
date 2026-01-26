use std::fmt;

/// Unique identifier for a type variable
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub usize);

impl fmt::Display for TypeVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "?{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int32,
    Int64,
    Float,
    Bool,
    String,
    List(Box<Type>),  // List with element type
    Tuple(Vec<Type>), // Tuple with element types (heterogeneous, fixed size)
    Var(TypeVarId),   // Unification type variable
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int32 => write!(f, "Int32"),
            Type::Int64 => write!(f, "Int64"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::String => write!(f, "String"),
            Type::List(elem) => write!(f, "List<{}>", elem),
            Type::Tuple(elems) => {
                let elem_strs: Vec<String> = elems.iter().map(|e| e.to_string()).collect();
                write!(f, "({})", elem_strs.join(", "))
            }
            Type::Var(id) => write!(f, "{}", id),
        }
    }
}

/// Function type signature
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionType {
    /// Source-level type parameter names (e.g., ["T", "U"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// Parameter types
    pub params: Vec<Type>,
    /// Return type
    pub return_type: Type,
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
