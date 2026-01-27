/// A path representing a potentially qualified name
/// Examples: `foo`, `Foo::Bar`, `mod::Type::variant`
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub segments: Vec<String>,
}

impl Path {
    /// Create a single-segment path (e.g., `foo`)
    pub fn simple(name: String) -> Self {
        Path {
            segments: vec![name],
        }
    }

    /// Check if this is a simple (single-segment) path
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// Get the single segment if this is a simple path
    pub fn as_simple(&self) -> Option<&str> {
        if self.segments.len() == 1 {
            Some(&self.segments[0])
        } else {
            None
        }
    }

    /// Get the last segment (useful for display and some lookups)
    pub fn last(&self) -> &str {
        self.segments
            .last()
            .expect("Path must have at least one segment")
    }

    /// Format path as string with :: separators
    pub fn to_string(&self) -> String {
        self.segments.join("::")
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.segments.join("::"))
    }
}

/// Top-level item (function, enum, struct, etc.)
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
    Enum(EnumDef),
}

/// Struct definition: `struct Name<T, U> { field: Type, ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<StructFieldDef>,
}

/// A field in a struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldDef {
    pub name: String,
    pub typ: TypeAnnotation,
}

/// Enum definition: `enum Option<T> { None, Some(T), Move { x: Int, y: Int } }`
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: Vec<EnumVariant>,
}

/// An enum variant
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub kind: EnumVariantKind,
}

/// The kind of data an enum variant carries
#[derive(Debug, Clone, PartialEq)]
pub enum EnumVariantKind {
    /// Unit variant: `None`
    Unit,
    /// Tuple variant: `Some(T)` or `Pair(T, U)`
    Tuple(Vec<TypeAnnotation>),
    /// Struct variant: `Move { x: Int, y: Int }`
    Struct(Vec<StructFieldDef>),
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
    Named(String),                                      // Int32, Float, T, etc.
    Parameterized(String, Vec<TypeAnnotation>),         // List<Int32>, Map<K, V>, etc.
    Tuple(Vec<TypeAnnotation>),                         // (Int32, String, Bool)
    Function(Vec<TypeAnnotation>, Box<TypeAnnotation>), // (Int32, String) -> Bool
}

/// Let binding: `let x = expr` or `let x: Type = expr`
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub value: Box<Expr>,
}

/// Lambda parameter: `x` or `x: Type`
#[derive(Debug, Clone, PartialEq)]
pub struct LambdaParam {
    pub name: String,
    pub typ: Option<TypeAnnotation>,
}

/// Pattern in a match arm
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Literal(Box<Expr>),    // 0, "hello", true
    Var(String),           // x (binds value)
    Wildcard,              // _ (matches all)
    List(ListPattern),     // [], [a, b], [x, ..]
    Tuple(TuplePattern),   // (), (a, b), (x, ..)
    Struct(StructPattern), // Point { x, y }, Point { x: px, .. }
    Enum(EnumPattern),     // Option::Some(x), Option::None, Message::Move { x, .. }
}

/// Enum pattern: `Option::Some(x)`, `Option::None`, `Message::Move { x, .. }`
#[derive(Debug, Clone, PartialEq)]
pub struct EnumPattern {
    pub path: Path,
    pub fields: EnumPatternFields,
}

/// Fields in an enum pattern
#[derive(Debug, Clone, PartialEq)]
pub enum EnumPatternFields {
    /// Unit variant: `Option::None`
    Unit,
    /// Tuple variant: `Option::Some(x)` or `Pair(a, b)`, reusing TuplePattern for rest support
    Tuple(TuplePattern),
    /// Struct variant: `Move { x, y }` or `Move { x, .. }`
    Struct {
        fields: Vec<StructFieldPattern>,
        is_partial: bool,
    },
}

/// A field pattern in a struct pattern
#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldPattern {
    pub field_name: String,   // the struct field being matched
    pub pattern: Box<Pattern>, // the pattern for this field (Var(same_name) for shorthand)
}

/// Struct pattern variants
#[derive(Debug, Clone, PartialEq)]
pub enum StructPattern {
    /// `Point { x, y }` - exact field match (all fields must be present)
    Exact {
        path: Path,
        fields: Vec<StructFieldPattern>,
    },
    /// `Point { x, .. }` - partial match with rest (some fields can be omitted)
    Partial {
        path: Path,
        fields: Vec<StructFieldPattern>,
    },
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
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    /// A path expression: `foo`, `Option::None`, `Mod::Type`
    Path(Path),
    /// Function/constructor call: `foo(1)`, `Option::Some(1)`
    Call {
        path: Path,
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
    Lambda {
        params: Vec<LambdaParam>,
        return_type: Option<TypeAnnotation>,
        body: Box<Expr>,
    },
    /// Struct/enum struct variant constructor: `Point { x: 1 }`, `Message::Move { x: 1 }`
    Struct {
        path: Path,
        fields: Vec<(String, Expr)>,
    },
    /// Field access: `point.x` (distinct from method call)
    FieldAccess {
        expr: Box<Expr>,
        field: String,
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
