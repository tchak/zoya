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
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// Named struct type with instantiated type parameters
    Struct {
        name: String,
        type_args: Vec<Type>,
        /// Field names and their instantiated types
        fields: Vec<(String, Type)>,
    },
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
            Type::Function { params, ret } => {
                if params.is_empty() {
                    write!(f, "() -> {}", ret)
                } else if params.len() == 1 {
                    write!(f, "{} -> {}", params[0], ret)
                } else {
                    let param_strs: Vec<String> =
                        params.iter().map(|p| p.to_string()).collect();
                    write!(f, "({}) -> {}", param_strs.join(", "), ret)
                }
            }
            Type::Struct { name, type_args, .. } => {
                if type_args.is_empty() {
                    write!(f, "{}", name)
                } else {
                    let args: Vec<String> = type_args.iter().map(|t| t.to_string()).collect();
                    write!(f, "{}<{}>", name, args.join(", "))
                }
            }
        }
    }
}

/// Function type signature (for named functions)
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

/// Struct type definition (stored in type environment)
#[derive(Debug, Clone, PartialEq)]
pub struct StructType {
    /// Struct name
    pub name: String,
    /// Source-level type parameter names (e.g., ["T", "U"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// Fields: name and type (types may contain Var(id) for generics)
    pub fields: Vec<(String, Type)>,
}

/// Type scheme for polymorphic values (let polymorphism)
/// Represents: forall a1..an. T
#[derive(Debug, Clone, PartialEq)]
pub struct TypeScheme {
    /// Quantified type variables (can be instantiated differently at each use)
    pub quantified: Vec<TypeVarId>,
    /// The underlying type
    pub ty: Type,
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
