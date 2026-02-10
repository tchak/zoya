use zoya_ast::{Expr, Path, Visibility};
use zoya_ir::{Definition, QualifiedPath, StructType, Type};

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
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("missing field 'y'"));
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
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unknown field 'z'"));
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
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("field 'y'") && err.message.contains("expects type"));
}

#[test]
fn test_struct_construct_unknown_struct() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path::simple("UnknownStruct".to_string()),
        fields: vec![],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unknown identifier"));
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
    assert!(err.message.contains("cannot be used as a value"));
}
