/// Path prefix for module resolution
/// Examples: `root::foo`, `self::bar`, `super::baz`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathPrefix {
    #[default]
    None,   // Relative path (child module or current scope)
    Root,   // root::
    Self_,  // self::
    Super,  // super::
}

impl std::fmt::Display for PathPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathPrefix::None => Ok(()),
            PathPrefix::Root => write!(f, "root::"),
            PathPrefix::Self_ => write!(f, "self::"),
            PathPrefix::Super => write!(f, "super::"),
        }
    }
}

/// A path representing a potentially qualified name with optional type arguments
/// Examples: `foo`, `Foo::Bar`, `Option::None::<Int>`, `root::utils::helper`
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Path prefix for module resolution (root::, self::, super::, or none)
    pub prefix: PathPrefix,
    pub segments: Vec<String>,
    /// Optional explicit type arguments (turbofish syntax)
    /// e.g., `Option::None::<Int>` has type_args = Some([Named("Int")])
    pub type_args: Option<Vec<TypeAnnotation>>,
}

impl Path {
    /// Create a single-segment path (e.g., `foo`)
    pub fn simple(name: String) -> Self {
        Path {
            prefix: PathPrefix::None,
            segments: vec![name],
            type_args: None,
        }
    }

    /// Check if this is a simple (single-segment) path
    #[allow(dead_code)]
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
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.prefix, self.segments.join("::"))?;
        if let Some(ref args) = self.type_args {
            write!(f, "::<")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", arg)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

/// Top-level item (function, enum, struct, type alias, use declaration, etc.)
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
    Enum(EnumDef),
    TypeAlias(TypeAliasDef),
    Use(UseDecl),
}

impl Item {
    /// Returns the name of this item, or `None` for items without a name (e.g., `Use`)
    pub fn name(&self) -> Option<&str> {
        match self {
            Item::Function(f) => Some(&f.name),
            Item::Struct(s) => Some(&s.name),
            Item::Enum(e) => Some(&e.name),
            Item::TypeAlias(t) => Some(&t.name),
            Item::Use(_) => None,
        }
    }
}

/// Struct definition: `[pub] struct Name<T, U> { field: Type, ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub visibility: Visibility,
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

/// Enum definition: `[pub] enum Option<T> { None, Some(T), Move { x: Int, y: Int } }`
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub visibility: Visibility,
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

/// Type alias definition: `[pub] type Name<T, U> = TypeAnnotation`
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDef {
    pub visibility: Visibility,
    pub name: String,
    pub type_params: Vec<String>,
    pub typ: TypeAnnotation,
}

/// Visibility of an item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    /// Private to the module (default)
    #[default]
    Private,
    /// Public, accessible from other modules
    Public,
}

/// Function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub visibility: Visibility,
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Expr,
}

/// Function parameter: `pattern: Type`
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub pattern: Pattern,
    pub typ: TypeAnnotation,
}

/// Type annotation in source code
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Named(Path),                                      // Int, T, module::MyType
    Parameterized(Path, Vec<TypeAnnotation>),         // List<Int>, module::Map<K, V>
    Tuple(Vec<TypeAnnotation>),                       // (Int, String, Bool)
    Function(Vec<TypeAnnotation>, Box<TypeAnnotation>), // (Int, String) -> Bool
}

impl std::fmt::Display for TypeAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeAnnotation::Named(path) => write!(f, "{}", path),
            TypeAnnotation::Parameterized(path, params) => {
                write!(f, "{}<", path)?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ">")
            }
            TypeAnnotation::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
            TypeAnnotation::Function(params, ret) => {
                if params.len() == 1 {
                    write!(f, "{} -> {}", params[0], ret)
                } else {
                    write!(f, "(")?;
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", p)?;
                    }
                    write!(f, ") -> {}", ret)
                }
            }
        }
    }
}

/// Let binding: `let pattern = expr` or `let x: Type = expr`
/// Type annotations are only allowed on simple variable patterns.
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub pattern: Pattern,
    pub type_annotation: Option<TypeAnnotation>,
    pub value: Box<Expr>,
}

/// Lambda parameter: `pattern` or `pattern: Type`
#[derive(Debug, Clone, PartialEq)]
pub struct LambdaParam {
    pub pattern: Pattern,
    pub typ: Option<TypeAnnotation>,
}

/// Pattern in a match arm
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Literal(Box<Expr>),    // 0, "hello", true
    Wildcard,              // _ (matches all)
    List(ListPattern),     // [], [a, b], [x, ..]
    Tuple(TuplePattern),   // (), (a, b), (x, ..)

    // Unified path-based patterns (like expressions)
    /// Path pattern for unit constructors: `Option::None`, `root::Color::Red`
    Path(Path),
    /// Call pattern for tuple constructors: `Option::Some(x)`, `root::Result::Ok(v)`
    Call {
        path: Path,
        args: TuplePattern, // Reuse TuplePattern for rest support
    },
    /// Struct pattern: `Point { x }`, `Message::Move { x, .. }`
    /// Works for both struct types and enum struct variants - type checker resolves which
    Struct {
        path: Path,
        fields: Vec<StructFieldPattern>,
        is_partial: bool,
    },

    As {
        name: String,          // n @ pattern - binds entire matched value to `n`
        pattern: Box<Pattern>,
    },
}

/// A field pattern in a struct pattern
#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldPattern {
    pub field_name: String,   // the struct field being matched
    pub pattern: Box<Pattern>, // the pattern for this field (Var(same_name) for shorthand)
}

/// List pattern variants
#[derive(Debug, Clone, PartialEq)]
pub enum ListPattern {
    Empty, // []
    Exact(Vec<Pattern>), // [a, b, c] - match exactly N elements
    Prefix {
        // [a, b, ..] or [a, b, rest @ ..]
        patterns: Vec<Pattern>,
        rest_binding: Option<String>, // name for rest @ ..
    },
    Suffix {
        // [.., x, y] or [rest @ .., x, y]
        patterns: Vec<Pattern>,
        rest_binding: Option<String>,
    },
    PrefixSuffix {
        // [a, .., z] or [a, rest @ .., z]
        prefix: Vec<Pattern>,
        suffix: Vec<Pattern>,
        rest_binding: Option<String>,
    },
}

/// Tuple pattern variants
#[derive(Debug, Clone, PartialEq)]
pub enum TuplePattern {
    Empty, // ()
    Exact(Vec<Pattern>), // (a, b, c) - match exactly N elements
    Prefix {
        // (a, b, ..) or (a, b, rest @ ..)
        patterns: Vec<Pattern>,
        rest_binding: Option<String>,
    },
    Suffix {
        // (.., y, z) or (rest @ .., y, z)
        patterns: Vec<Pattern>,
        rest_binding: Option<String>,
    },
    PrefixSuffix {
        // (a, .., z) or (a, rest @ .., z)
        prefix: Vec<Pattern>,
        suffix: Vec<Pattern>,
        rest_binding: Option<String>,
    },
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

/// A statement in REPL input (expression or let binding)
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    Let(LetBinding),
}

/// Module declaration: `mod name`
#[derive(Debug, Clone, PartialEq)]
pub struct ModDecl {
    pub name: String,
}

/// Use declaration: `use root::foo::bar`
#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub path: UsePath,
}

/// Path in a use declaration
#[derive(Debug, Clone, PartialEq)]
pub struct UsePath {
    pub prefix: PathPrefix,
    pub segments: Vec<String>,
}

/// A parsed module file containing mod declarations and items
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub mods: Vec<ModDecl>,
    pub items: Vec<Item>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Path tests

    #[test]
    fn test_path_simple() {
        let path = Path::simple("foo".to_string());
        assert_eq!(path.segments, vec!["foo"]);
        assert!(path.type_args.is_none());
    }

    #[test]
    fn test_path_is_simple() {
        let simple = Path::simple("x".to_string());
        assert!(simple.is_simple());

        let qualified = Path {
            prefix: PathPrefix::None,
            segments: vec!["Mod".to_string(), "Type".to_string()],
            type_args: None,
        };
        assert!(!qualified.is_simple());
    }

    #[test]
    fn test_path_as_simple() {
        let simple = Path::simple("foo".to_string());
        assert_eq!(simple.as_simple(), Some("foo"));

        let qualified = Path {
            prefix: PathPrefix::None,
            segments: vec!["Mod".to_string(), "Type".to_string()],
            type_args: None,
        };
        assert_eq!(qualified.as_simple(), None);
    }

    #[test]
    fn test_path_display_simple() {
        let path = Path::simple("foo".to_string());
        assert_eq!(path.to_string(), "foo");
    }

    #[test]
    fn test_path_display_qualified() {
        let path = Path {
            prefix: PathPrefix::None,
            segments: vec!["Option".to_string(), "Some".to_string()],
            type_args: None,
        };
        assert_eq!(path.to_string(), "Option::Some");
    }

    #[test]
    fn test_path_display_with_turbofish() {
        let path = Path {
            prefix: PathPrefix::None,
            segments: vec!["Option".to_string(), "None".to_string()],
            type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
        };
        assert_eq!(path.to_string(), "Option::None::<Int>");
    }

    #[test]
    fn test_path_display_turbofish_multiple_args() {
        let path = Path {
            prefix: PathPrefix::None,
            segments: vec!["Result".to_string()],
            type_args: Some(vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
            ]),
        };
        assert_eq!(path.to_string(), "Result::<Int, String>");
    }

    // PathPrefix tests

    #[test]
    fn test_path_prefix_display() {
        assert_eq!(PathPrefix::None.to_string(), "");
        assert_eq!(PathPrefix::Root.to_string(), "root::");
        assert_eq!(PathPrefix::Self_.to_string(), "self::");
        assert_eq!(PathPrefix::Super.to_string(), "super::");
    }

    #[test]
    fn test_path_with_prefix_display() {
        let path = Path {
            prefix: PathPrefix::Root,
            segments: vec!["utils".to_string(), "foo".to_string()],
            type_args: None,
        };
        assert_eq!(path.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_path_self_prefix_display() {
        let path = Path {
            prefix: PathPrefix::Self_,
            segments: vec!["bar".to_string()],
            type_args: None,
        };
        assert_eq!(path.to_string(), "self::bar");
    }

    #[test]
    fn test_path_super_prefix_display() {
        let path = Path {
            prefix: PathPrefix::Super,
            segments: vec!["helper".to_string()],
            type_args: None,
        };
        assert_eq!(path.to_string(), "super::helper");
    }

    // TypeAnnotation tests

    #[test]
    fn test_type_annotation_display_named() {
        let ta = TypeAnnotation::Named(Path::simple("Int".to_string()));
        assert_eq!(ta.to_string(), "Int");
    }

    #[test]
    fn test_type_annotation_display_parameterized() {
        let ta = TypeAnnotation::Parameterized(
            Path::simple("List".to_string()),
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
        );
        assert_eq!(ta.to_string(), "List<Int>");
    }

    #[test]
    fn test_type_annotation_display_parameterized_multiple() {
        let ta = TypeAnnotation::Parameterized(
            Path::simple("Map".to_string()),
            vec![
                TypeAnnotation::Named(Path::simple("String".to_string())),
                TypeAnnotation::Named(Path::simple("Int".to_string())),
            ],
        );
        assert_eq!(ta.to_string(), "Map<String, Int>");
    }

    #[test]
    fn test_type_annotation_display_tuple_empty() {
        let ta = TypeAnnotation::Tuple(vec![]);
        assert_eq!(ta.to_string(), "()");
    }

    #[test]
    fn test_type_annotation_display_tuple_single() {
        let ta = TypeAnnotation::Tuple(vec![TypeAnnotation::Named(Path::simple(
            "Int".to_string(),
        ))]);
        assert_eq!(ta.to_string(), "(Int)");
    }

    #[test]
    fn test_type_annotation_display_tuple_multiple() {
        let ta = TypeAnnotation::Tuple(vec![
            TypeAnnotation::Named(Path::simple("Int".to_string())),
            TypeAnnotation::Named(Path::simple("String".to_string())),
            TypeAnnotation::Named(Path::simple("Bool".to_string())),
        ]);
        assert_eq!(ta.to_string(), "(Int, String, Bool)");
    }

    #[test]
    fn test_type_annotation_display_function_single_param() {
        let ta = TypeAnnotation::Function(
            vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
            Box::new(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
        );
        assert_eq!(ta.to_string(), "Int -> Bool");
    }

    #[test]
    fn test_type_annotation_display_function_multiple_params() {
        let ta = TypeAnnotation::Function(
            vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
            ],
            Box::new(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
        );
        assert_eq!(ta.to_string(), "(Int, String) -> Bool");
    }
}
