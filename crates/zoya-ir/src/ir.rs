use std::fmt;

use zoya_ast::{BinOp, UnaryOp};
use zoya_package::QualifiedPath;

use crate::types::{Definition, Type};

/// HTTP method for route functions
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// Parse an attribute name into an HTTP method, if it matches.
    pub fn from_attr_name(name: &str) -> Option<Self> {
        match name {
            "get" => Some(HttpMethod::Get),
            "post" => Some(HttpMethod::Post),
            "put" => Some(HttpMethod::Put),
            "patch" => Some(HttpMethod::Patch),
            "delete" => Some(HttpMethod::Delete),
            _ => None,
        }
    }

    /// Returns the attribute name for this HTTP method.
    pub fn attr_name(&self) -> &'static str {
        match self {
            HttpMethod::Get => "get",
            HttpMethod::Post => "post",
            HttpMethod::Put => "put",
            HttpMethod::Patch => "patch",
            HttpMethod::Delete => "delete",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
        }
    }
}

/// A validated URL pathname for HTTP routes.
/// Must start with `/` and contain valid path segments.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pathname(String);

impl Pathname {
    /// Create a new Pathname, validating that it starts with `/`
    /// and contains only valid path segments.
    pub fn new(path: &str) -> Result<Self, String> {
        if !path.starts_with('/') {
            return Err(format!("pathname '{}' must start with '/'", path));
        }
        if path.len() > 1 {
            for segment in path[1..].split('/') {
                if segment.is_empty() {
                    return Err(format!("pathname '{}' contains an empty segment", path));
                }
                for ch in segment.chars() {
                    if !ch.is_alphanumeric() && ch != '-' && ch != '_' && ch != ':' {
                        return Err(format!(
                            "pathname '{}' contains invalid character '{}' in segment '{}'",
                            path, ch, segment
                        ));
                    }
                }
            }
        }
        Ok(Pathname(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Pathname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The kind of a function definition
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FunctionKind {
    #[default]
    Regular,
    Builtin,
    Test,
    Job,
    Http(HttpMethod, Pathname),
}

/// Typed function definition
#[derive(Debug, Clone, PartialEq)]
pub struct TypedFunction {
    pub name: String,
    pub params: Vec<(TypedPattern, Type)>,
    pub body: TypedExpr,
    pub return_type: Type,
    pub kind: FunctionKind,
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
    EnumUnit {
        path: QualifiedPath,
    },
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
    /// Tuple struct pattern (exact): `Pair(a, b)`
    StructTupleExact {
        path: QualifiedPath,
        patterns: Vec<TypedPattern>,
        total_fields: usize,
    },
    /// Tuple struct pattern (prefix): `Triple(a, ..)`
    StructTuplePrefix {
        path: QualifiedPath,
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_fields: usize,
    },
    /// Tuple struct pattern (suffix): `Triple(.., c)`
    StructTupleSuffix {
        path: QualifiedPath,
        patterns: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_fields: usize,
    },
    /// Tuple struct pattern (prefix+suffix): `Triple(a, .., c)`
    StructTuplePrefixSuffix {
        path: QualifiedPath,
        prefix: Vec<TypedPattern>,
        suffix: Vec<TypedPattern>,
        rest_binding: Option<(String, Type)>,
        total_fields: usize,
    },
}

/// Typed match arm
#[derive(Debug, Clone, PartialEq)]
pub struct TypedMatchArm {
    pub pattern: TypedPattern,
    pub result: TypedExpr,
}

/// Element in a typed list expression
#[derive(Debug, Clone, PartialEq)]
pub enum TypedListElement {
    Item(TypedExpr),
    Spread(TypedExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedStringPart {
    Literal(String),
    Expr(Box<TypedExpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedExpr {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    InterpolatedString(Vec<TypedStringPart>),
    List {
        elements: Vec<TypedListElement>,
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
    Lambda {
        params: Vec<(TypedPattern, Type)>,
        body: Box<TypedExpr>,
        ty: Type,
    },
    /// Struct constructor: `Point { x: 1, y: 2 }` or with spread `Point { x: 1, ..p }`
    StructConstruct {
        path: QualifiedPath,
        fields: Vec<(String, TypedExpr)>, // field name -> typed value
        spread: Option<Box<TypedExpr>>,   // optional spread: `..expr`
        ty: Type,
    },
    /// Tuple struct constructor: `Pair(1, "hello")`
    StructTupleConstruct {
        path: QualifiedPath,
        args: Vec<TypedExpr>,
        ty: Type,
    },
    /// Field access: `point.x`
    FieldAccess {
        expr: Box<TypedExpr>,
        field: String,
        ty: Type,
    },
    /// Tuple/tuple struct index access: `tuple.0`, `pair.1`
    TupleIndex {
        expr: Box<TypedExpr>,
        index: usize,
        ty: Type,
    },
    /// Enum variant constructor: `Option::Some(42)`, `Option::None`, `Message::Move { x: 1 }`
    EnumConstruct {
        path: QualifiedPath,
        fields: TypedEnumConstructFields,
        ty: Type,
    },
    /// List index access: `list[0]` -> Option<T>
    ListIndex {
        expr: Box<TypedExpr>,
        index: Box<TypedExpr>,
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
            TypedExpr::InterpolatedString(_) => Type::String,
            TypedExpr::List { ty, .. } => ty.clone(),
            TypedExpr::Tuple { ty, .. } => ty.clone(),
            TypedExpr::Var { ty, .. } => ty.clone(),
            TypedExpr::Call { ty, .. } => ty.clone(),
            TypedExpr::UnaryOp { ty, .. } => ty.clone(),
            TypedExpr::BinOp { ty, .. } => ty.clone(),
            TypedExpr::Block { result, .. } => result.ty(),
            TypedExpr::Match { ty, .. } => ty.clone(),
            TypedExpr::Lambda { ty, .. } => ty.clone(),
            TypedExpr::StructConstruct { ty, .. } => ty.clone(),
            TypedExpr::StructTupleConstruct { ty, .. } => ty.clone(),
            TypedExpr::FieldAccess { ty, .. } => ty.clone(),
            TypedExpr::TupleIndex { ty, .. } => ty.clone(),
            TypedExpr::EnumConstruct { ty, .. } => ty.clone(),
            TypedExpr::ListIndex { ty, .. } => ty.clone(),
        }
    }
}

/// The complete checked package
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedPackage {
    pub name: String,
    pub items: std::collections::HashMap<QualifiedPath, TypedFunction>,
    pub definitions: std::collections::HashMap<QualifiedPath, Definition>,
    pub reexports: std::collections::HashMap<QualifiedPath, QualifiedPath>,
}

impl CheckedPackage {
    /// Return sorted paths of all `#[test]` functions in this package.
    pub fn tests(&self) -> Vec<QualifiedPath> {
        let mut tests: Vec<QualifiedPath> = self
            .items
            .iter()
            .filter(|(_, func)| func.kind == FunctionKind::Test)
            .map(|(path, _)| path.clone())
            .collect();
        tests.sort_by_key(|a| a.to_string());
        tests
    }

    /// Return sorted paths of all `#[job]` functions in this package.
    pub fn jobs(&self) -> Vec<QualifiedPath> {
        let mut jobs: Vec<QualifiedPath> = self
            .items
            .iter()
            .filter(|(_, func)| func.kind == FunctionKind::Job)
            .map(|(path, _)| path.clone())
            .collect();
        jobs.sort_by_key(|a| a.to_string());
        jobs
    }

    /// Return sorted paths of all public, non-test, non-job functions in this package.
    pub fn fns(&self) -> Vec<QualifiedPath> {
        let mut fns: Vec<QualifiedPath> = self
            .items
            .iter()
            .filter(|(path, func)| {
                matches!(func.kind, FunctionKind::Regular | FunctionKind::Builtin)
                    && self.definitions.contains_key(path)
            })
            .map(|(path, _)| path.clone())
            .collect();
        fns.sort_by_key(|a| a.to_string());
        fns
    }

    /// Return sorted (path, method, pathname) tuples for all HTTP route functions.
    pub fn routes(&self) -> Vec<(QualifiedPath, &HttpMethod, &Pathname)> {
        let mut routes: Vec<(QualifiedPath, &HttpMethod, &Pathname)> = self
            .items
            .iter()
            .filter_map(|(path, func)| {
                if let FunctionKind::Http(ref method, ref pathname) = func.kind {
                    Some((path.clone(), method, pathname))
                } else {
                    None
                }
            })
            .collect();
        routes.sort_by_key(|(path, _, _)| path.to_string());
        routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pathname_valid_root() {
        assert!(Pathname::new("/").is_ok());
    }

    #[test]
    fn test_pathname_valid_path() {
        let p = Pathname::new("/users").unwrap();
        assert_eq!(p.as_str(), "/users");
    }

    #[test]
    fn test_pathname_valid_nested() {
        assert!(Pathname::new("/users/profile").is_ok());
    }

    #[test]
    fn test_pathname_valid_with_param() {
        assert!(Pathname::new("/users/:id").is_ok());
    }

    #[test]
    fn test_pathname_valid_with_hyphens_underscores() {
        assert!(Pathname::new("/my-route/sub_path").is_ok());
    }

    #[test]
    fn test_pathname_must_start_with_slash() {
        let err = Pathname::new("users").unwrap_err();
        assert!(err.contains("must start with '/'"));
    }

    #[test]
    fn test_pathname_no_empty_segments() {
        let err = Pathname::new("/users//profile").unwrap_err();
        assert!(err.contains("empty segment"));
    }

    #[test]
    fn test_pathname_invalid_chars() {
        let err = Pathname::new("/users?q=1").unwrap_err();
        assert!(err.contains("invalid character"));
    }

    #[test]
    fn test_pathname_display() {
        let p = Pathname::new("/test").unwrap();
        assert_eq!(format!("{}", p), "/test");
    }

    #[test]
    fn test_http_method_from_attr_name() {
        assert_eq!(HttpMethod::from_attr_name("get"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_attr_name("post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::from_attr_name("put"), Some(HttpMethod::Put));
        assert_eq!(HttpMethod::from_attr_name("patch"), Some(HttpMethod::Patch));
        assert_eq!(
            HttpMethod::from_attr_name("delete"),
            Some(HttpMethod::Delete)
        );
        assert_eq!(HttpMethod::from_attr_name("options"), None);
        assert_eq!(HttpMethod::from_attr_name("head"), None);
        assert_eq!(HttpMethod::from_attr_name("unknown"), None);
    }

    #[test]
    fn test_http_method_attr_name() {
        assert_eq!(HttpMethod::Get.attr_name(), "get");
        assert_eq!(HttpMethod::Post.attr_name(), "post");
        assert_eq!(HttpMethod::Put.attr_name(), "put");
        assert_eq!(HttpMethod::Patch.attr_name(), "patch");
        assert_eq!(HttpMethod::Delete.attr_name(), "delete");
    }
}
