use zoya_ast::{BinOp, EnumDef, StructDef, TypeAliasDef, UnaryOp};
use crate::types::Type;

/// A resolved qualified path (e.g., `Option::Some`, `Color::Red`)
/// Unlike ast::Path, this has no type_args - type information
/// is stored in the TypedExpr::ty field after type checking.
#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedPath {
    pub segments: Vec<String>,
}

impl QualifiedPath {
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    pub fn simple(name: String) -> Self {
        Self { segments: vec![name] }
    }

    pub fn last(&self) -> &str {
        self.segments.last().expect("path cannot be empty")
    }
}

impl std::fmt::Display for QualifiedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.segments.join("::"))
    }
}

/// A checked item from the type checker
#[derive(Debug, Clone, PartialEq)]
pub enum CheckedItem {
    Function(Box<TypedFunction>),
    Struct(StructDef),       // Structs are declarations, passed through as-is
    Enum(EnumDef),           // Enums are declarations, passed through as-is
    TypeAlias(TypeAliasDef), // Type aliases are transparent, passed through as-is
}

/// Type-checked statement result for REPL (expression or let binding)
#[derive(Debug, Clone, PartialEq)]
pub enum CheckedStmt {
    Expr(TypedExpr),
    Let(TypedLetBinding),
}

/// Typed function definition
#[derive(Debug, Clone, PartialEq)]
pub struct TypedFunction {
    pub name: String,
    pub params: Vec<(TypedPattern, Type)>,
    pub body: TypedExpr,
    pub return_type: Type,
}

/// Typed let binding
#[derive(Debug, Clone, PartialEq)]
pub struct TypedLetBinding {
    pub pattern: TypedPattern,
    pub value: TypedExpr,
    pub ty: Type,
}

/// Typed pattern in a match arm
#[derive(Debug, Clone, PartialEq)]
pub enum TypedPattern {
    Literal(TypedExpr),
    Var {
        name: String,
        ty: Type,
    },
    Wildcard,
    /// As pattern: `n @ pattern` binds the entire matched value to `n`
    As {
        name: String,
        ty: Type,
        pattern: Box<TypedPattern>,
    },
    ListEmpty,
    ListExact {
        patterns: Vec<TypedPattern>,
        len: usize,
    },
    ListPrefix {
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        min_len: usize,
    },
    ListSuffix {
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        min_len: usize,
    },
    ListPrefixSuffix {
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        min_len: usize,
    },
    TupleEmpty,
    TupleExact {
        patterns: Vec<TypedPattern>,
        len: usize,
    },
    TuplePrefix {
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_len: usize,
    },
    TupleSuffix {
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_len: usize,
    },
    TuplePrefixSuffix {
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_len: usize,
    },
    /// Struct pattern: `Point { x, y }` or `Point { x: px, .. }`
    /// Fields are in the order they appear in the struct definition, not the pattern.
    /// For partial patterns, missing fields are omitted from the vec.
    StructExact {
        path: QualifiedPath,
        /// (field_name, pattern) pairs for all struct fields
        fields: Vec<(String, TypedPattern)>,
    },
    StructPartial {
        path: QualifiedPath,
        /// (field_name, pattern) pairs for matched fields only
        fields: Vec<(String, TypedPattern)>,
    },
    /// Enum unit variant pattern: `Option::None`
    EnumUnit { path: QualifiedPath },
    /// Enum tuple variant pattern (exact): `Option::Some(x)`
    EnumTupleExact {
        path: QualifiedPath,
        patterns: Vec<TypedPattern>,
        total_fields: usize,
    },
    /// Enum tuple variant pattern (prefix): `Result::Ok(a, ..)` or `Result::Ok(a, rest @ ..)`
    EnumTuplePrefix {
        path: QualifiedPath,
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_fields: usize,
    },
    /// Enum tuple variant pattern (suffix): `Result::Err(.., msg)` or `Result::Err(rest @ .., msg)`
    EnumTupleSuffix {
        path: QualifiedPath,
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_fields: usize,
    },
    /// Enum tuple variant pattern (prefix+suffix): `Triple::Make(a, .., c)` or `Triple::Make(a, rest @ .., c)`
    EnumTuplePrefixSuffix {
        path: QualifiedPath,
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_fields: usize,
    },
    /// Enum struct variant pattern (exact): `Message::Move { x, y }`
    EnumStructExact {
        path: QualifiedPath,
        fields: Vec<(String, TypedPattern)>,
    },
    /// Enum struct variant pattern (partial): `Message::Move { x, .. }`
    EnumStructPartial {
        path: QualifiedPath,
        fields: Vec<(String, TypedPattern)>,
    },
}

/// Typed match arm
#[derive(Debug, Clone, PartialEq)]
pub struct TypedMatchArm {
    pub pattern: TypedPattern,
    pub result: TypedExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedExpr {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List {
        elements: Vec<TypedExpr>,
        ty: Type,
    },
    Tuple {
        elements: Vec<TypedExpr>,
        ty: Type,
    },
    Var {
        path: QualifiedPath,
        ty: Type,
    },
    Call {
        path: QualifiedPath,
        args: Vec<TypedExpr>,
        ty: Type,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<TypedExpr>,
        ty: Type,
    },
    BinOp {
        op: BinOp,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
        ty: Type,
    },
    Block {
        bindings: Vec<TypedLetBinding>,
        result: Box<TypedExpr>,
    },
    Match {
        scrutinee: Box<TypedExpr>,
        arms: Vec<TypedMatchArm>,
        ty: Type,
    },
    MethodCall {
        receiver: Box<TypedExpr>,
        method: String,
        args: Vec<TypedExpr>,
        ty: Type,
    },
    Lambda {
        params: Vec<(TypedPattern, Type)>,
        body: Box<TypedExpr>,
        ty: Type,
    },
    /// Struct constructor: `Point { x: 1, y: 2 }`
    StructConstruct {
        path: QualifiedPath,
        fields: Vec<(String, TypedExpr)>, // field name -> typed value
        ty: Type,
    },
    /// Field access: `point.x`
    FieldAccess {
        expr: Box<TypedExpr>,
        field: String,
        ty: Type,
    },
    /// Enum variant constructor: `Option::Some(42)`, `Option::None`, `Message::Move { x: 1 }`
    EnumConstruct {
        path: QualifiedPath,
        fields: TypedEnumConstructFields,
        ty: Type,
    },
}

/// Typed fields for enum variant construction
#[derive(Debug, Clone, PartialEq)]
pub enum TypedEnumConstructFields {
    /// Unit variant: `Option::None`
    Unit,
    /// Tuple variant: `Option::Some(42)` or `Result::Ok(1, 2)`
    Tuple(Vec<TypedExpr>),
    /// Struct variant: `Message::Move { x: 1, y: 2 }`
    Struct(Vec<(String, TypedExpr)>),
}

impl TypedExpr {
    pub fn ty(&self) -> Type {
        match self {
            TypedExpr::Int(_) => Type::Int,
            TypedExpr::BigInt(_) => Type::BigInt,
            TypedExpr::Float(_) => Type::Float,
            TypedExpr::Bool(_) => Type::Bool,
            TypedExpr::String(_) => Type::String,
            TypedExpr::List { ty, .. } => ty.clone(),
            TypedExpr::Tuple { ty, .. } => ty.clone(),
            TypedExpr::Var { ty, .. } => ty.clone(),
            TypedExpr::Call { ty, .. } => ty.clone(),
            TypedExpr::UnaryOp { ty, .. } => ty.clone(),
            TypedExpr::BinOp { ty, .. } => ty.clone(),
            TypedExpr::Block { result, .. } => result.ty(),
            TypedExpr::Match { ty, .. } => ty.clone(),
            TypedExpr::MethodCall { ty, .. } => ty.clone(),
            TypedExpr::Lambda { ty, .. } => ty.clone(),
            TypedExpr::StructConstruct { ty, .. } => ty.clone(),
            TypedExpr::FieldAccess { ty, .. } => ty.clone(),
            TypedExpr::EnumConstruct { ty, .. } => ty.clone(),
        }
    }
}

/// A checked module containing type-checked items
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedModule {
    pub items: Vec<CheckedItem>,
}

/// The complete checked module tree
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedModuleTree {
    pub modules: std::collections::HashMap<zoya_module::ModulePath, CheckedModule>,
}

impl CheckedModuleTree {
    /// Get the root module
    pub fn root(&self) -> Option<&CheckedModule> {
        self.modules.get(&zoya_module::ModulePath::root())
    }
}
