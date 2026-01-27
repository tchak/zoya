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
    Int,
    BigInt,
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
    /// Named enum type with instantiated type parameters
    Enum {
        name: String,
        type_args: Vec<Type>,
        /// Variant names and their types (for exhaustiveness checking)
        variants: Vec<(String, EnumVariantType)>,
    },
}

/// Type information for an enum variant
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnumVariantType {
    /// Unit variant: `None`
    Unit,
    /// Tuple variant: `Some(T)` - types are the fields in order
    Tuple(Vec<Type>),
    /// Struct variant: `Move { x: Int, y: Int }` - field names and types
    Struct(Vec<(String, Type)>),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::BigInt => write!(f, "BigInt"),
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
            Type::Enum { name, type_args, .. } => {
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

/// Enum type definition (stored in type environment)
#[derive(Debug, Clone, PartialEq)]
pub struct EnumType {
    /// Enum name
    pub name: String,
    /// Source-level type parameter names (e.g., ["T", "E"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// Variants: name and type (types may contain Var(id) for generics)
    pub variants: Vec<(String, EnumVariantType)>,
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

impl TypeScheme {
    /// Create a monomorphic type scheme (no quantified variables).
    /// Instantiation will return the type unchanged.
    pub fn mono(ty: Type) -> Self {
        TypeScheme {
            quantified: vec![],
            ty,
        }
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
