use crate::ast::{BinOp, UnaryOp};
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub enum TypedExpr {
    Int(i64),
    Float(f64),
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
            TypedExpr::UnaryOp { ty, .. } => *ty,
            TypedExpr::BinOp { ty, .. } => *ty,
        }
    }
}
