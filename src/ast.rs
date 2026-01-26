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
    Named(String),                              // Int32, Float, T, etc.
    Parameterized(String, Vec<TypeAnnotation>), // List<Int32>, Map<K, V>, etc.
    Tuple(Vec<TypeAnnotation>),                 // (Int32, String, Bool)
}

/// Let binding: `let x = expr` or `let x: Type = expr`
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub value: Box<Expr>,
}

/// Pattern in a match arm
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Literal(Box<Expr>),  // 0, "hello", true
    Var(String),         // x (binds value)
    Wildcard,            // _ (matches all)
    List(ListPattern),   // [], [a, b], [x, ..]
    Tuple(TuplePattern), // (), (a, b), (x, ..)
}

/// List pattern variants
#[derive(Debug, Clone, PartialEq)]
pub enum ListPattern {
    Empty,                                    // []
    Exact(Vec<Pattern>),                      // [a, b, c] - match exactly N elements
    Prefix(Vec<Pattern>),                     // [a, b, ..] - match at least N elements at start
    Suffix(Vec<Pattern>),                     // [.., x, y] - match at least N elements at end
    PrefixSuffix(Vec<Pattern>, Vec<Pattern>), // [a, .., z] - match first and last elements
}

/// Tuple pattern variants
#[derive(Debug, Clone, PartialEq)]
pub enum TuplePattern {
    Empty,                                    // ()
    Exact(Vec<Pattern>),                      // (a, b, c) - match exactly N elements
    Prefix(Vec<Pattern>),                     // (a, b, ..) - match first N, skip rest
    Suffix(Vec<Pattern>),                     // (.., y, z) - skip first, match last N
    PrefixSuffix(Vec<Pattern>, Vec<Pattern>), // (a, .., z) - match first and last
}

/// Match arm: pattern => result
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub result: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
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
    Block {
        bindings: Vec<LetBinding>,
        result: Box<Expr>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
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
    // Comparison operators
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

/// A statement in REPL input (item, expression, or let binding)
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Item(Item),
    Expr(Expr),
    Let(LetBinding),
}
