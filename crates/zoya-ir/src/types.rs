use std::fmt;

use zoya_ast::Visibility;
use zoya_package::QualifiedPath;

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
                    let param_strs: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                    write!(f, "({}) -> {}", param_strs.join(", "), ret)
                }
            }
            Type::Struct {
                name, type_args, ..
            } => {
                if type_args.is_empty() {
                    write!(f, "{}", name)
                } else {
                    let args: Vec<String> = type_args.iter().map(|t| t.to_string()).collect();
                    write!(f, "{}<{}>", name, args.join(", "))
                }
            }
            Type::Enum {
                name, type_args, ..
            } => {
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
    /// Visibility of the function
    pub visibility: Visibility,
    /// Module where this function is defined
    pub module: QualifiedPath,
    /// Source-level type parameter names (e.g., ["T", "U"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// Parameter types
    pub params: Vec<Type>,
    /// Return type
    pub return_type: Type,
}

/// The kind of a struct type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructTypeKind {
    /// Unit struct: `struct Empty`
    Unit,
    /// Tuple struct: `struct Pair(Int, String)` — fields stored as `$0`, `$1`, etc.
    Tuple,
    /// Named-field struct: `struct Point { x: Int, y: Int }`
    Named,
}

/// Struct type definition (stored in type environment)
#[derive(Debug, Clone, PartialEq)]
pub struct StructType {
    /// Visibility of the struct
    pub visibility: Visibility,
    /// Module where this struct is defined
    pub module: QualifiedPath,
    /// Struct name
    pub name: String,
    /// Source-level type parameter names (e.g., ["T", "U"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// The kind of struct (Unit, Tuple, or Named)
    pub kind: StructTypeKind,
    /// Fields: name and type (types may contain Var(id) for generics)
    /// For tuple structs, field names are "$0", "$1", etc.
    pub fields: Vec<(String, Type)>,
}

/// Enum type definition (stored in type environment)
#[derive(Debug, Clone, PartialEq)]
pub struct EnumType {
    /// Visibility of the enum
    pub visibility: Visibility,
    /// Module where this enum is defined
    pub module: QualifiedPath,
    /// Enum name
    pub name: String,
    /// Source-level type parameter names (e.g., ["T", "E"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// Variants: name and type (types may contain Var(id) for generics)
    pub variants: Vec<(String, EnumVariantType)>,
}

/// Type alias definition (stored in type environment)
/// Type aliases are transparent - they resolve to their underlying type
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasType {
    /// Visibility of the type alias
    pub visibility: Visibility,
    /// Module where this type alias is defined
    pub module: QualifiedPath,
    /// Alias name
    pub name: String,
    /// Source-level type parameter names (e.g., ["T", "U"])
    pub type_params: Vec<String>,
    /// TypeVarIds corresponding to each type parameter
    pub type_var_ids: Vec<TypeVarId>,
    /// The underlying type this alias resolves to
    pub typ: Type,
}

/// Module type definition (stored in type environment)
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleType {
    /// Visibility of the module
    pub visibility: Visibility,
    /// Parent module path
    pub module: QualifiedPath,
    /// Module name
    pub name: String,
}

/// A named definition in the global namespace
#[derive(Debug, Clone, PartialEq)]
pub enum Definition {
    Function(FunctionType),
    Struct(StructType),
    Enum(EnumType),
    EnumVariant(EnumType, EnumVariantType),
    TypeAlias(TypeAliasType),
    Module(ModuleType),
}

impl Definition {
    pub fn as_function(&self) -> Option<&FunctionType> {
        match self {
            Definition::Function(f) => Some(f),
            _ => None,
        }
    }

    pub fn as_struct(&self) -> Option<&StructType> {
        match self {
            Definition::Struct(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_enum(&self) -> Option<&EnumType> {
        match self {
            Definition::Enum(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_type_alias(&self) -> Option<&TypeAliasType> {
        match self {
            Definition::TypeAlias(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_module(&self) -> Option<&ModuleType> {
        match self {
            Definition::Module(m) => Some(m),
            _ => None,
        }
    }

    pub fn is_module(&self) -> bool {
        matches!(self, Definition::Module(_))
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Definition::Function(_) => "function",
            Definition::Struct(_) => "struct",
            Definition::Enum(_) => "enum",
            Definition::EnumVariant(..) => "enum variant",
            Definition::TypeAlias(_) => "type alias",
            Definition::Module(_) => "module",
        }
    }

    pub fn module(&self) -> &QualifiedPath {
        match self {
            Definition::Function(f) => &f.module,
            Definition::Struct(s) => &s.module,
            Definition::Enum(e) => &e.module,
            Definition::EnumVariant(parent_enum, _) => &parent_enum.module,
            Definition::TypeAlias(a) => &a.module,
            Definition::Module(m) => &m.module,
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self {
            Definition::Function(f) => f.visibility,
            Definition::Struct(s) => s.visibility,
            Definition::Enum(e) => e.visibility,
            Definition::EnumVariant(parent_enum, _) => parent_enum.visibility,
            Definition::TypeAlias(a) => a.visibility,
            Definition::Module(m) => m.visibility,
        }
    }

    /// Remap the module field, replacing "root" with a new name.
    pub fn with_root(self, name: &str) -> Self {
        match self {
            Definition::Function(f) => Definition::Function(FunctionType {
                module: f.module.with_root(name),
                ..f
            }),
            Definition::Struct(s) => Definition::Struct(StructType {
                module: s.module.with_root(name),
                ..s
            }),
            Definition::Enum(e) => Definition::Enum(EnumType {
                module: e.module.with_root(name),
                ..e
            }),
            Definition::EnumVariant(parent_enum, variant) => Definition::EnumVariant(
                EnumType {
                    module: parent_enum.module.with_root(name),
                    ..parent_enum
                },
                variant,
            ),
            Definition::TypeAlias(a) => Definition::TypeAlias(TypeAliasType {
                module: a.module.with_root(name),
                ..a
            }),
            Definition::Module(m) => Definition::Module(ModuleType {
                module: m.module.with_root(name),
                ..m
            }),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_type_var_id() {
        assert_eq!(TypeVarId(0).to_string(), "?0");
        assert_eq!(TypeVarId(42).to_string(), "?42");
    }

    #[test]
    fn test_display_primitive_types() {
        assert_eq!(Type::Int.to_string(), "Int");
        assert_eq!(Type::BigInt.to_string(), "BigInt");
        assert_eq!(Type::Float.to_string(), "Float");
        assert_eq!(Type::Bool.to_string(), "Bool");
        assert_eq!(Type::String.to_string(), "String");
    }

    #[test]
    fn test_display_list() {
        let list_int = Type::List(Box::new(Type::Int));
        assert_eq!(list_int.to_string(), "List<Int>");

        let list_list = Type::List(Box::new(Type::List(Box::new(Type::String))));
        assert_eq!(list_list.to_string(), "List<List<String>>");
    }

    #[test]
    fn test_display_tuple() {
        let empty = Type::Tuple(vec![]);
        assert_eq!(empty.to_string(), "()");

        let single = Type::Tuple(vec![Type::Int]);
        assert_eq!(single.to_string(), "(Int)");

        let pair = Type::Tuple(vec![Type::Int, Type::String]);
        assert_eq!(pair.to_string(), "(Int, String)");

        let triple = Type::Tuple(vec![Type::Int, Type::Bool, Type::Float]);
        assert_eq!(triple.to_string(), "(Int, Bool, Float)");
    }

    #[test]
    fn test_display_var() {
        let var = Type::Var(TypeVarId(5));
        assert_eq!(var.to_string(), "?5");
    }

    #[test]
    fn test_display_function_no_params() {
        let func = Type::Function {
            params: vec![],
            ret: Box::new(Type::Int),
        };
        assert_eq!(func.to_string(), "() -> Int");
    }

    #[test]
    fn test_display_function_one_param() {
        let func = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        assert_eq!(func.to_string(), "Int -> Bool");
    }

    #[test]
    fn test_display_function_multiple_params() {
        let func = Type::Function {
            params: vec![Type::Int, Type::String],
            ret: Box::new(Type::Bool),
        };
        assert_eq!(func.to_string(), "(Int, String) -> Bool");
    }

    #[test]
    fn test_display_struct_no_type_args() {
        let s = Type::Struct {
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![],
        };
        assert_eq!(s.to_string(), "Point");
    }

    #[test]
    fn test_display_struct_with_type_args() {
        let s = Type::Struct {
            name: "Pair".to_string(),
            type_args: vec![Type::Int, Type::String],
            fields: vec![],
        };
        assert_eq!(s.to_string(), "Pair<Int, String>");
    }

    #[test]
    fn test_display_enum_no_type_args() {
        let e = Type::Enum {
            name: "Color".to_string(),
            type_args: vec![],
            variants: vec![],
        };
        assert_eq!(e.to_string(), "Color");
    }

    #[test]
    fn test_display_enum_with_type_args() {
        let e = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![],
        };
        assert_eq!(e.to_string(), "Option<Int>");
    }

    #[test]
    fn test_display_type_error() {
        let err = TypeError {
            message: "type mismatch".to_string(),
        };
        assert_eq!(err.to_string(), "type mismatch");
    }

    #[test]
    fn test_type_scheme_mono() {
        let scheme = TypeScheme::mono(Type::Int);
        assert!(scheme.quantified.is_empty());
        assert_eq!(scheme.ty, Type::Int);
    }
}
