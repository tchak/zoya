use zoya_ast::{Expr, Path};
use zoya_ir::{Definition, StructType, Type};
use zoya_module::ModulePath;

use crate::check::{check_expr, TypeEnv};
use crate::unify::UnifyCtx;

fn env_with_point_struct() -> TypeEnv {
    let mut env = TypeEnv::default();
    env.register(
        "root::Point".to_string(),
        Definition::Struct(StructType {
            name: "Point".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            fields: vec![
                ("x".to_string(), Type::Int),
                ("y".to_string(), Type::Int),
            ],
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unknown struct"));
}
