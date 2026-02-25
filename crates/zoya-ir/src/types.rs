use std::collections::HashMap;
use std::fmt;

use zoya_ast::Visibility;
use zoya_package::QualifiedPath;

use crate::ir::CheckedPackage;

/// Unique identifier for a type variable
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TypeVarId(pub usize);

impl fmt::Display for TypeVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "?{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Type {
    Int,
    BigInt,
    Float,
    Bool,
    String,
    List(Box<Type>),            // List with element type
    Set(Box<Type>),             // Set with element type
    Dict(Box<Type>, Box<Type>), // Dict with key and value types
    Task(Box<Type>),            // Task with result type (lazy async)
    Tuple(Vec<Type>),           // Tuple with element types (heterogeneous, fixed size)
    Var(TypeVarId),             // Unification type variable
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// Named struct type with instantiated type parameters
    Struct {
        /// Module where this struct is defined
        module: QualifiedPath,
        name: String,
        type_args: Vec<Type>,
        /// Field names and their instantiated types
        fields: Vec<(String, Type)>,
    },
    /// Named enum type with instantiated type parameters
    Enum {
        /// Module where this enum is defined
        module: QualifiedPath,
        name: String,
        type_args: Vec<Type>,
        /// Variant names and their types (for exhaustiveness checking)
        variants: Vec<(String, EnumVariantType)>,
    },
}

/// Type information for an enum variant
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EnumVariantType {
    /// Unit variant: `None`
    Unit,
    /// Tuple variant: `Some(T)` - types are the fields in order
    Tuple(Vec<Type>),
    /// Struct variant: `Move { x: Int, y: Int }` - field names and types
    Struct(Vec<(String, Type)>),
}

impl Type {
    /// Format this type as a human-readable string (e.g., `Int`, `List<String>`).
    pub fn pretty(&self) -> String {
        crate::display::pretty_type(self)
    }

    /// Remap all embedded module paths, replacing "root" with a new name.
    pub fn with_root(self, name: &str) -> Self {
        match self {
            Type::Struct {
                module,
                name: n,
                type_args,
                fields,
            } => Type::Struct {
                module: module.with_root(name),
                name: n,
                type_args: type_args.into_iter().map(|t| t.with_root(name)).collect(),
                fields: fields
                    .into_iter()
                    .map(|(f, t)| (f, t.with_root(name)))
                    .collect(),
            },
            Type::Enum {
                module,
                name: n,
                type_args,
                variants,
            } => Type::Enum {
                module: module.with_root(name),
                name: n,
                type_args: type_args.into_iter().map(|t| t.with_root(name)).collect(),
                variants: variants
                    .into_iter()
                    .map(|(v, vt)| (v, vt.with_root(name)))
                    .collect(),
            },
            Type::List(elem) => Type::List(Box::new(elem.with_root(name))),
            Type::Set(elem) => Type::Set(Box::new(elem.with_root(name))),
            Type::Task(elem) => Type::Task(Box::new(elem.with_root(name))),
            Type::Dict(key, val) => {
                Type::Dict(Box::new(key.with_root(name)), Box::new(val.with_root(name)))
            }
            Type::Tuple(elems) => {
                Type::Tuple(elems.into_iter().map(|t| t.with_root(name)).collect())
            }
            Type::Function { params, ret } => Type::Function {
                params: params.into_iter().map(|t| t.with_root(name)).collect(),
                ret: Box::new(ret.with_root(name)),
            },
            other => other,
        }
    }
}

impl EnumVariantType {
    /// Remap all embedded module paths, replacing "root" with a new name.
    pub fn with_root(self, name: &str) -> Self {
        match self {
            EnumVariantType::Unit => EnumVariantType::Unit,
            EnumVariantType::Tuple(types) => {
                EnumVariantType::Tuple(types.into_iter().map(|t| t.with_root(name)).collect())
            }
            EnumVariantType::Struct(fields) => EnumVariantType::Struct(
                fields
                    .into_iter()
                    .map(|(f, t)| (f, t.with_root(name)))
                    .collect(),
            ),
        }
    }
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
            Type::Set(elem) => write!(f, "Set<{}>", elem),
            Type::Task(elem) => write!(f, "Task<{}>", elem),
            Type::Dict(key, val) => write!(f, "Dict<{}, {}>", key, val),
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

impl FunctionType {
    /// Format the function signature for display: `(Int, String) -> Bool`
    pub fn pretty(&self) -> String {
        let params: Vec<String> = self.params.iter().map(|ty| ty.pretty()).collect();
        let ret = self.return_type.pretty();
        format!("({}) -> {}", params.join(", "), ret)
    }
}

/// The kind of a struct type
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Impl method type definition (stored in type environment)
#[derive(Debug, Clone, PartialEq)]
pub struct ImplMethodType {
    /// Visibility of the method
    pub visibility: Visibility,
    /// Module where the impl block is defined
    pub module: QualifiedPath,
    /// Name of the target type (e.g., "Point")
    pub target_type_name: String,
    /// Type params on the impl block (e.g., ["T"] for `impl<T> Wrapper<T>`)
    pub impl_type_params: Vec<String>,
    /// TypeVarIds for impl type params
    pub impl_type_var_ids: Vec<TypeVarId>,
    /// Whether this is a method (has self) or associated function
    pub has_self: bool,
    /// Method's own type params (e.g., ["U"] for `fn map<U>(self, f: T -> U) -> U`)
    pub type_params: Vec<String>,
    /// TypeVarIds for method's own type params
    pub type_var_ids: Vec<TypeVarId>,
    /// All parameter types (including self as first if has_self)
    pub params: Vec<Type>,
    /// Return type
    pub return_type: Type,
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
    ImplMethod(ImplMethodType),
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

    pub fn as_impl_method(&self) -> Option<&ImplMethodType> {
        match self {
            Definition::ImplMethod(m) => Some(m),
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
            Definition::ImplMethod(_) => "impl method",
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
            Definition::ImplMethod(m) => &m.module,
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
            Definition::ImplMethod(m) => m.visibility,
        }
    }

    /// Remap the module field, replacing "root" with a new name.
    pub fn with_root(self, root: &str) -> Self {
        match self {
            Definition::Function(f) => Definition::Function(FunctionType {
                module: f.module.with_root(root),
                params: f.params.into_iter().map(|t| t.with_root(root)).collect(),
                return_type: f.return_type.with_root(root),
                ..f
            }),
            Definition::Struct(s) => Definition::Struct(StructType {
                module: s.module.with_root(root),
                fields: s
                    .fields
                    .into_iter()
                    .map(|(n, t)| (n, t.with_root(root)))
                    .collect(),
                ..s
            }),
            Definition::Enum(e) => Definition::Enum(EnumType {
                module: e.module.with_root(root),
                variants: e
                    .variants
                    .into_iter()
                    .map(|(n, vt)| (n, vt.with_root(root)))
                    .collect(),
                ..e
            }),
            Definition::EnumVariant(parent_enum, variant) => Definition::EnumVariant(
                EnumType {
                    module: parent_enum.module.with_root(root),
                    variants: parent_enum
                        .variants
                        .into_iter()
                        .map(|(n, vt)| (n, vt.with_root(root)))
                        .collect(),
                    ..parent_enum
                },
                variant.with_root(root),
            ),
            Definition::TypeAlias(a) => Definition::TypeAlias(TypeAliasType {
                module: a.module.with_root(root),
                typ: a.typ.with_root(root),
                ..a
            }),
            Definition::Module(m) => Definition::Module(ModuleType {
                module: m.module.with_root(root),
                ..m
            }),
            Definition::ImplMethod(m) => Definition::ImplMethod(ImplMethodType {
                module: m.module.with_root(root),
                params: m.params.into_iter().map(|t| t.with_root(root)).collect(),
                return_type: m.return_type.with_root(root),
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

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TypeError {
    #[error("type mismatch: {expected} vs {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("{context}: expected {expected}, got {actual}: {detail}")]
    TypeMismatchIn {
        context: String,
        expected: String,
        actual: String,
        detail: String,
    },

    #[error("unknown identifier: {name}")]
    UnboundVariable { name: String },

    #[error("unknown path: {path}")]
    UnboundPath { path: String },

    #[error("no method '{method}' on type {on_type}")]
    UnboundMethod { method: String, on_type: String },

    #[error("{kind} '{name}' expects {expected} {what}, got {actual}")]
    ArityMismatch {
        kind: String,
        name: String,
        expected: usize,
        actual: usize,
        what: String,
    },

    #[error("tuple length mismatch: {expected} vs {actual}")]
    TupleLengthMismatch { expected: usize, actual: usize },

    #[error("{kind} {name} expects {expected} type argument(s), got {actual}")]
    TypeArgCount {
        kind: String,
        name: String,
        expected: usize,
        actual: usize,
    },

    #[error("{kind} '{name}' is private to module '{module}'")]
    PrivateAccess {
        kind: String,
        name: String,
        module: String,
    },

    #[error("pub use cannot re-export private item '{name}'")]
    PrivateReExport { name: String },

    #[error("missing field '{field}' in {context}")]
    MissingField { field: String, context: String },

    #[error("unknown field '{field}' in {context}")]
    UnknownField { field: String, context: String },

    #[error("refutable pattern in {context}: {detail}")]
    RefutablePattern { context: String, detail: String },

    #[error("duplicate binding '{name}' in pattern")]
    DuplicateBinding { name: String },

    #[error("non-exhaustive match: missing pattern(s) {patterns}{hint}")]
    NonExhaustiveMatch { patterns: String, hint: String },

    #[error("unreachable pattern(s): arm(s) {arms}")]
    UnreachablePattern { arms: String },

    #[error("{kind} '{name}' should be {convention} (e.g., '{suggestion}')")]
    NamingConvention {
        kind: String,
        name: String,
        convention: String,
        suggestion: String,
    },

    #[error("{kind} '{name}' {problem}")]
    KindMisuse {
        kind: String,
        name: String,
        problem: String,
    },

    #[error("enum variant {variant} {problem}")]
    VariantMismatch { variant: String, problem: String },

    #[error("{message}")]
    InvalidAttribute { message: String },

    #[error("cannot find '{name}' to import")]
    UnboundImport { name: String },

    #[error("'{name}' is already imported (from '{original}')")]
    DuplicateImport { name: String, original: String },

    #[error("{message}")]
    ImportValidation { message: String },

    #[error("{operator} only work on {expected_types}, not {actual_type}")]
    InvalidOperatorType {
        operator: String,
        expected_types: String,
        actual_type: String,
    },

    #[error("infinite type: {lhs} = {rhs}")]
    InfiniteType { lhs: String, rhs: String },

    #[error("circular type alias detected: {name}")]
    CircularTypeAlias { name: String },

    #[error("circular re-export detected: {path}")]
    CircularReExport { path: String },

    #[error("{message}")]
    InvalidImpl { message: String },

    #[error("duplicate definition for '{name}' on type '{on_type}'")]
    DuplicateDefinition { name: String, on_type: String },

    #[error("{message}")]
    InvalidIndex { message: String },

    #[error("{message}")]
    InvalidInterpolation { message: String },

    #[error("{message}")]
    PathResolution { message: String },

    #[error("match expression must have at least one arm")]
    EmptyMatch,

    #[error(
        "'{name}' is an associated function on '{on_type}', not a method; call it as {on_type}::{name}()"
    )]
    AssociatedFunctionAsMethod { name: String, on_type: String },

    #[error("`Self` can only be used inside an impl block")]
    SelfOutsideImpl,
}

/// Lookup table for resolving recursive type stubs.
///
/// During two-phase type registration, inner references to recursive types
/// carry empty variants/fields. This table maps qualified paths to their real
/// definitions so consumers can inflate these stubs at any nesting depth.
type EnumInfo = (Vec<TypeVarId>, Vec<(String, EnumVariantType)>);
type StructInfo = (Vec<TypeVarId>, Vec<(String, Type)>);

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DefinitionLookup {
    enums: HashMap<QualifiedPath, EnumInfo>,
    structs: HashMap<QualifiedPath, StructInfo>,
    functions: HashMap<QualifiedPath, FunctionType>,
}

impl DefinitionLookup {
    /// Build lookup from the global definitions table.
    pub fn from_definitions(definitions: &HashMap<QualifiedPath, Definition>) -> Self {
        let mut lookup = Self::empty();
        lookup.add_definitions(definitions);
        lookup
    }

    /// Build lookup from a package and its dependencies.
    pub fn from_packages(package: &CheckedPackage, deps: &[&CheckedPackage]) -> Self {
        let mut lookup = Self::from_definitions(&package.definitions);
        for dep in deps {
            lookup.add_definitions(&dep.definitions);
        }
        lookup
    }

    /// Add definitions from another definitions map.
    fn add_definitions(&mut self, definitions: &HashMap<QualifiedPath, Definition>) {
        for (path, def) in definitions {
            match def {
                Definition::Enum(enum_type) if !enum_type.variants.is_empty() => {
                    self.enums.insert(
                        path.clone(),
                        (enum_type.type_var_ids.clone(), enum_type.variants.clone()),
                    );
                }
                Definition::Struct(struct_type) if !struct_type.fields.is_empty() => {
                    self.structs.insert(
                        path.clone(),
                        (struct_type.type_var_ids.clone(), struct_type.fields.clone()),
                    );
                }
                Definition::Function(func_type) => {
                    self.functions.insert(path.clone(), func_type.clone());
                }
                _ => {}
            }
        }
    }

    /// Create an empty lookup (for tests that don't need recursive type resolution).
    pub fn empty() -> Self {
        DefinitionLookup {
            enums: HashMap::new(),
            structs: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Look up a function definition by its qualified path.
    pub fn get_function(&self, path: &QualifiedPath) -> Option<&FunctionType> {
        self.functions.get(path)
    }

    /// Resolve enum variants: if the type carries empty variants (a stub),
    /// look up the real variants and apply type argument substitution.
    pub fn resolve_enum_variants(
        &self,
        module: &QualifiedPath,
        name: &str,
        variants: &[(String, EnumVariantType)],
        type_args: &[Type],
    ) -> Vec<(String, EnumVariantType)> {
        if !variants.is_empty() {
            return variants.to_vec();
        }
        let key = module.child(name);
        if let Some((type_var_ids, real_variants)) = self.enums.get(&key) {
            if type_args.is_empty() || type_var_ids.is_empty() {
                return real_variants.clone();
            }
            let mapping: HashMap<TypeVarId, Type> = type_var_ids
                .iter()
                .zip(type_args.iter())
                .map(|(id, ty)| (*id, ty.clone()))
                .collect();
            real_variants
                .iter()
                .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, &mapping)))
                .collect()
        } else {
            variants.to_vec()
        }
    }

    /// Resolve struct fields: if the type carries empty fields (a stub),
    /// look up the real fields and apply type argument substitution.
    pub fn resolve_struct_fields(
        &self,
        module: &QualifiedPath,
        name: &str,
        fields: &[(String, Type)],
        type_args: &[Type],
    ) -> Vec<(String, Type)> {
        if !fields.is_empty() {
            return fields.to_vec();
        }
        let key = module.child(name);
        if let Some((type_var_ids, real_fields)) = self.structs.get(&key) {
            if type_args.is_empty() || type_var_ids.is_empty() {
                return real_fields.clone();
            }
            let mapping: HashMap<TypeVarId, Type> = type_var_ids
                .iter()
                .zip(type_args.iter())
                .map(|(id, ty)| (*id, ty.clone()))
                .collect();
            real_fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, &mapping)))
                .collect()
        } else {
            fields.to_vec()
        }
    }
}

/// Substitute type variables in a type using a mapping (recursive).
pub fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(elem) => Type::List(Box::new(substitute_type_vars(elem, mapping))),
        Type::Set(elem) => Type::Set(Box::new(substitute_type_vars(elem, mapping))),
        Type::Task(elem) => Type::Task(Box::new(substitute_type_vars(elem, mapping))),
        Type::Dict(key, val) => Type::Dict(
            Box::new(substitute_type_vars(key, mapping)),
            Box::new(substitute_type_vars(val, mapping)),
        ),
        Type::Tuple(elems) => Type::Tuple(
            elems
                .iter()
                .map(|e| substitute_type_vars(e, mapping))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|p| substitute_type_vars(p, mapping))
                .collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::Struct {
            module,
            name,
            type_args,
            fields,
        } => Type::Struct {
            module: module.clone(),
            name: name.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        },
        Type::Enum {
            module,
            name,
            type_args,
            variants,
        } => Type::Enum {
            module: module.clone(),
            name: name.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            variants: variants
                .iter()
                .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, mapping)))
                .collect(),
        },
        // Concrete types don't contain type vars
        Type::Int | Type::BigInt | Type::Float | Type::Bool | Type::String => ty.clone(),
    }
}

/// Substitute type variables in an enum variant type.
pub fn substitute_variant_type_vars(
    vt: &EnumVariantType,
    mapping: &HashMap<TypeVarId, Type>,
) -> EnumVariantType {
    match vt {
        EnumVariantType::Unit => EnumVariantType::Unit,
        EnumVariantType::Tuple(types) => EnumVariantType::Tuple(
            types
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
        ),
        EnumVariantType::Struct(fields) => EnumVariantType::Struct(
            fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        ),
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
    fn test_display_dict() {
        let dict = Type::Dict(Box::new(Type::String), Box::new(Type::Int));
        assert_eq!(dict.to_string(), "Dict<String, Int>");

        let nested = Type::Dict(
            Box::new(Type::String),
            Box::new(Type::List(Box::new(Type::Int))),
        );
        assert_eq!(nested.to_string(), "Dict<String, List<Int>>");
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
            module: QualifiedPath::root(),
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![],
        };
        assert_eq!(s.to_string(), "Point");
    }

    #[test]
    fn test_display_struct_with_type_args() {
        let s = Type::Struct {
            module: QualifiedPath::root(),
            name: "Pair".to_string(),
            type_args: vec![Type::Int, Type::String],
            fields: vec![],
        };
        assert_eq!(s.to_string(), "Pair<Int, String>");
    }

    #[test]
    fn test_display_enum_no_type_args() {
        let e = Type::Enum {
            module: QualifiedPath::root(),
            name: "Color".to_string(),
            type_args: vec![],
            variants: vec![],
        };
        assert_eq!(e.to_string(), "Color");
    }

    #[test]
    fn test_display_enum_with_type_args() {
        let e = Type::Enum {
            module: QualifiedPath::root(),
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![],
        };
        assert_eq!(e.to_string(), "Option<Int>");
    }

    #[test]
    fn test_display_type_error() {
        let err = TypeError::TypeMismatch {
            expected: "Int".to_string(),
            actual: "String".to_string(),
        };
        assert_eq!(err.to_string(), "type mismatch: Int vs String");
    }

    #[test]
    fn test_type_scheme_mono() {
        let scheme = TypeScheme::mono(Type::Int);
        assert!(scheme.quantified.is_empty());
        assert_eq!(scheme.ty, Type::Int);
    }
}
