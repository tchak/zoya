use crate::ast::{BinOp, StructDef, UnaryOp};
use crate::types::Type;

/// A checked item from the type checker
#[derive(Debug, Clone, PartialEq)]
pub enum CheckedItem {
    Function(TypedFunction),
    Struct(StructDef), // Structs are declarations, passed through as-is
}

/// Typed function definition
#[derive(Debug, Clone, PartialEq)]
pub struct TypedFunction {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub body: TypedExpr,
    pub return_type: Type,
}

/// Typed let binding
#[derive(Debug, Clone, PartialEq)]
pub struct TypedLetBinding {
    pub name: String,
    pub value: TypedExpr,
    pub ty: Type,
}

/// Typed pattern in a match arm
#[derive(Debug, Clone, PartialEq)]
pub enum TypedPattern {
    Literal(TypedExpr),
    Var { name: String, ty: Type },
    Wildcard,
    ListEmpty,
    ListExact { patterns: Vec<TypedPattern>, len: usize },
    ListPrefix { patterns: Vec<TypedPattern>, min_len: usize },
    ListSuffix { patterns: Vec<TypedPattern>, min_len: usize },
    ListPrefixSuffix {
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        min_len: usize,
    },
    TupleEmpty,
    TupleExact { patterns: Vec<TypedPattern>, len: usize },
    TuplePrefix { patterns: Vec<TypedPattern>, total_len: usize },
    TupleSuffix { patterns: Vec<TypedPattern>, total_len: usize },
    TuplePrefixSuffix {
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        total_len: usize,
    },
    /// Struct pattern: `Point { x, y }` or `Point { x: px, .. }`
    /// Fields are in the order they appear in the struct definition, not the pattern.
    /// For partial patterns, missing fields are omitted from the vec.
    StructExact {
        name: String,
        /// (field_name, pattern) pairs for all struct fields
        fields: Vec<(String, TypedPattern)>,
    },
    StructPartial {
        name: String,
        /// (field_name, pattern) pairs for matched fields only
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
    Int32(i32),
    #[allow(dead_code)] // Used in tests and through function parameters
    Int64(i64),
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
        name: String,
        ty: Type,
    },
    Call {
        func: String,
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
        params: Vec<(String, Type)>,
        body: Box<TypedExpr>,
        ty: Type,
    },
    /// Struct constructor: `Point { x: 1, y: 2 }`
    StructConstruct {
        name: String,
        fields: Vec<(String, TypedExpr)>, // field name -> typed value
        ty: Type,
    },
    /// Field access: `point.x`
    FieldAccess {
        expr: Box<TypedExpr>,
        field: String,
        ty: Type,
    },
}

impl TypedExpr {
    pub fn ty(&self) -> Type {
        match self {
            TypedExpr::Int32(_) => Type::Int32,
            TypedExpr::Int64(_) => Type::Int64,
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
        }
    }
}
