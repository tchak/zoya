use crate::ast::{BinOp, UnaryOp};
use crate::types::Type;

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
}

impl TypedExpr {
    pub fn ty(&self) -> Type {
        match self {
            TypedExpr::Int32(_) => Type::Int32,
            TypedExpr::Int64(_) => Type::Int64,
            TypedExpr::Float(_) => Type::Float,
            TypedExpr::Bool(_) => Type::Bool,
            TypedExpr::String(_) => Type::String,
            TypedExpr::Var { ty, .. } => ty.clone(),
            TypedExpr::Call { ty, .. } => ty.clone(),
            TypedExpr::UnaryOp { ty, .. } => ty.clone(),
            TypedExpr::BinOp { ty, .. } => ty.clone(),
            TypedExpr::Block { result, .. } => result.ty(),
            TypedExpr::Match { ty, .. } => ty.clone(),
        }
    }
}
