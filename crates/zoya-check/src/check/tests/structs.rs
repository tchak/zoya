use zoya_ast::{Expr, Path, Visibility};
use zoya_ir::{Definition, QualifiedPath, StructType, StructTypeKind, Type, TypeScheme};

use crate::check::{TypeEnv, check_expr};
use crate::unify::UnifyCtx;

fn qpath(path: &str) -> QualifiedPath {
    QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
}

fn env_with_point_struct() -> TypeEnv {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::Point"),
        Definition::Struct(StructType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            name: "Point".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            kind: StructTypeKind::Named,
            fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        }),
    );
    env
}

#[test]
fn test_struct_construct_valid() {
    let env = env_with_point_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            ("y".to_string(), Expr::Int(20)),
        ],
        spread: None,
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Struct { name, .. } => assert_eq!(name, "Point"),
        _ => panic!("Expected struct type"),
    }
}

#[test]
fn test_struct_construct_missing_field() {
    let env = env_with_point_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            // Missing y field
        ],
        spread: None,
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("missing field 'y'"));
}

#[test]
fn test_struct_construct_extra_field() {
    let env = env_with_point_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            ("y".to_string(), Expr::Int(20)),
            ("z".to_string(), Expr::Int(30)), // Extra field
        ],
        spread: None,
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("unknown field 'z'"));
}

#[test]
fn test_struct_construct_field_type_mismatch() {
    let env = env_with_point_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            ("y".to_string(), Expr::String("wrong".to_string())), // Wrong type
        ],
        spread: None,
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("field 'y'") && err.to_string().contains("expected"));
}

#[test]
fn test_struct_construct_unknown_struct() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("UnknownStruct".to_string()),
        fields: vec![],
        spread: None,
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("unknown identifier"));
}

fn env_with_empty_struct() -> TypeEnv {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::Empty"),
        Definition::Struct(StructType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            name: "Empty".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            kind: StructTypeKind::Unit,
            fields: vec![],
        }),
    );
    env
}

#[test]
fn test_unit_struct_bare_path_construct() {
    let env = env_with_empty_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Path(Path::simple("Empty".to_string()));
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Struct { name, fields, .. } => {
            assert_eq!(name, "Empty");
            assert!(fields.is_empty());
        }
        _ => panic!("Expected struct type"),
    }
}

#[test]
fn test_non_unit_struct_bare_path_error() {
    let env = env_with_point_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Path(Path::simple("Point".to_string()));
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("cannot be used as a value"));
}

// Tuple struct tests

fn env_with_tuple_struct() -> TypeEnv {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::Pair"),
        Definition::Struct(StructType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            name: "Pair".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            kind: StructTypeKind::Tuple,
            fields: vec![
                ("$0".to_string(), Type::Int),
                ("$1".to_string(), Type::String),
            ],
        }),
    );
    env
}

#[test]
fn test_tuple_struct_construct() {
    let env = env_with_tuple_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("Pair".to_string()),
        args: vec![Expr::Int(1), Expr::String("hello".to_string())],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Struct { name, fields, .. } => {
            assert_eq!(name, "Pair");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "$0");
            assert_eq!(fields[1].0, "$1");
        }
        _ => panic!("Expected struct type"),
    }
}

#[test]
fn test_tuple_struct_wrong_arity() {
    let env = env_with_tuple_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("Pair".to_string()),
        args: vec![Expr::Int(1)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
}

#[test]
fn test_tuple_struct_wrong_type() {
    let env = env_with_tuple_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("Pair".to_string()),
        args: vec![Expr::String("wrong".to_string()), Expr::Int(1)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
}

#[test]
fn test_tuple_struct_brace_syntax_error() {
    let env = env_with_tuple_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Pair".to_string()),
        fields: vec![
            ("$0".to_string(), Expr::Int(1)),
            ("$1".to_string(), Expr::String("hello".to_string())),
        ],
        spread: None,
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
}

#[test]
fn test_tuple_struct_bare_path_error() {
    let env = env_with_tuple_struct();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Path(Path::simple("Pair".to_string()));
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
}

// Spread tests

fn env_with_point_and_binding() -> TypeEnv {
    let mut env = env_with_point_struct();
    env.locals.insert(
        "p".to_string(),
        TypeScheme::mono(Type::Struct {
            module: QualifiedPath::root(),
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        }),
    );
    env
}

#[test]
fn test_struct_spread_override_one_field() {
    let env = env_with_point_and_binding();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![("x".to_string(), Expr::Int(10))],
        spread: Some(Box::new(Expr::Path(Path::simple("p".to_string())))),
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Struct { name, .. } => assert_eq!(name, "Point"),
        _ => panic!("Expected struct type"),
    }
}

#[test]
fn test_struct_spread_no_explicit_fields() {
    let env = env_with_point_and_binding();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![],
        spread: Some(Box::new(Expr::Path(Path::simple("p".to_string())))),
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Struct { name, .. } => assert_eq!(name, "Point"),
        _ => panic!("Expected struct type"),
    }
}

#[test]
fn test_struct_spread_type_mismatch() {
    let mut env = env_with_point_struct();
    env.locals
        .insert("s".to_string(), TypeScheme::mono(Type::String));
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![("x".to_string(), Expr::Int(10))],
        spread: Some(Box::new(Expr::Path(Path::simple("s".to_string())))),
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("spread"));
}

#[test]
fn test_struct_spread_unknown_explicit_field() {
    let env = env_with_point_and_binding();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![("z".to_string(), Expr::Int(10))],
        spread: Some(Box::new(Expr::Path(Path::simple("p".to_string())))),
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("unknown field 'z'"));
}

#[test]
fn test_struct_spread_all_fields_explicit() {
    let env = env_with_point_and_binding();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("Point".to_string()),
        fields: vec![
            ("x".to_string(), Expr::Int(1)),
            ("y".to_string(), Expr::Int(2)),
        ],
        spread: Some(Box::new(Expr::Path(Path::simple("p".to_string())))),
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Struct { name, .. } => assert_eq!(name, "Point"),
        _ => panic!("Expected struct type"),
    }
}
