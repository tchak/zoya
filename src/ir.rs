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

#[derive(Debug, Clone, PartialEq)]
pub enum TypedExpr {
    Int(i64),
    Float(f64),
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
}

impl TypedExpr {
    pub fn ty(&self) -> Type {
        match self {
            TypedExpr::Int(_) => Type::Int,
            TypedExpr::Float(_) => Type::Float,
            TypedExpr::Var { ty, .. } => ty.clone(),
            TypedExpr::Call { ty, .. } => ty.clone(),
            TypedExpr::UnaryOp { ty, .. } => ty.clone(),
            TypedExpr::BinOp { ty, .. } => ty.clone(),
        }
    }
}
