/// Top-level item (function, enum, struct, etc.)
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
}

/// Function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Expr,
}

/// Function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub typ: TypeAnnotation,
}

/// Type annotation in source code
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Named(String), // Int, Float, T, etc.
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Var(String),
    Call {
        func: String,
        args: Vec<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}
