use crate::ast::{BinOp, EnumDef, StructDef, UnaryOp};
use crate::types::Type;

/// A checked item from the type checker
#[derive(Debug, Clone, PartialEq)]
pub enum CheckedItem {
    Function(TypedFunction),
    Struct(StructDef), // Structs are declarations, passed through as-is
    Enum(EnumDef),     // Enums are declarations, passed through as-is
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
    Var {
        name: String,
        ty: Type,
    },
    Wildcard,
    ListEmpty,
    ListExact {
        patterns: Vec<TypedPattern>,
        len: usize,
    },
    ListPrefix {
        patterns: Vec<TypedPattern>,
        min_len: usize,
    },
    ListSuffix {
        patterns: Vec<TypedPattern>,
        min_len: usize,
    },
    ListPrefixSuffix {
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        min_len: usize,
    },
    TupleEmpty,
    TupleExact {
        patterns: Vec<TypedPattern>,
        len: usize,
    },
    TuplePrefix {
        patterns: Vec<TypedPattern>,
        total_len: usize,
    },
    TupleSuffix {
        patterns: Vec<TypedPattern>,
        total_len: usize,
    },
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
    /// Enum unit variant pattern: `Option::None`
    EnumUnit {
        enum_name: String,
        variant_name: String,
    },
    /// Enum tuple variant pattern (exact): `Option::Some(x)`
    EnumTupleExact {
        enum_name: String,
        variant_name: String,
        patterns: Vec<TypedPattern>,
        total_fields: usize,
    },
    /// Enum tuple variant pattern (prefix): `Result::Ok(a, ..)`
    EnumTuplePrefix {
        enum_name: String,
        variant_name: String,
        patterns: Vec<TypedPattern>,
        total_fields: usize,
    },
    /// Enum tuple variant pattern (suffix): `Result::Err(.., msg)`
    EnumTupleSuffix {
        enum_name: String,
        variant_name: String,
        patterns: Vec<TypedPattern>,
        total_fields: usize,
    },
    /// Enum tuple variant pattern (prefix+suffix): `Triple::Make(a, .., c)`
    EnumTuplePrefixSuffix {
        enum_name: String,
        variant_name: String,
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        total_fields: usize,
    },
    /// Enum struct variant pattern (exact): `Message::Move { x, y }`
    EnumStructExact {
        enum_name: String,
        variant_name: String,
        fields: Vec<(String, TypedPattern)>,
    },
    /// Enum struct variant pattern (partial): `Message::Move { x, .. }`
    EnumStructPartial {
        enum_name: String,
        variant_name: String,
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
    /// Enum variant constructor: `Option::Some(42)`, `Option::None`, `Message::Move { x: 1 }`
    EnumConstruct {
        enum_name: String,
        variant_name: String,
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
