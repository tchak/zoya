use zoya_ast::{Expr, Path, PathPrefix, Visibility};
use zoya_ir::{Definition, EnumType, EnumVariantType, QualifiedPath, Type};

use crate::check::{check_expr, TypeEnv};
use crate::unify::UnifyCtx;

fn qpath(path: &str) -> QualifiedPath {
    QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
}

fn env_with_message_enum() -> TypeEnv {
    let mut env = TypeEnv::default();
    let enum_type = EnumType {
        visibility: Visibility::Public,
        module: QualifiedPath::root(),
        name: "Message".to_string(),
        type_params: vec![],
        type_var_ids: vec![],
        variants: vec![
            ("Quit".to_string(), EnumVariantType::Unit),
            (
                "Move".to_string(),
                EnumVariantType::Struct(vec![
                    ("x".to_string(), Type::Int),
                    ("y".to_string(), Type::Int),
                ]),
            ),
            ("Write".to_string(), EnumVariantType::Tuple(vec![Type::String])),
        ],
    };
    env.register(qpath("root::Message"), Definition::Enum(enum_type.clone()));
    // Register each variant separately
    for (variant_name, variant_type) in &enum_type.variants {
        env.register(
            qpath(&format!("root::Message::{}", variant_name)),
            Definition::EnumVariant(enum_type.clone(), variant_type.clone()),
        );
    }
    env
}

#[test]
fn test_enum_tuple_construct_valid() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Write".to_string()],
            type_args: None,
        },
        args: vec![Expr::String("hello".to_string())],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Enum { name, .. } => assert_eq!(name, "Message"),
        _ => panic!("Expected enum type"),
    }
}

#[test]
fn test_enum_tuple_construct_unit_variant_with_args_error() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Quit".to_string()],
            type_args: None,
        },
        args: vec![Expr::Int(1)], // Quit is a unit variant
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unit variant"));
}

#[test]
fn test_enum_tuple_construct_struct_variant_with_tuple_syntax_error() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Move".to_string()],
            type_args: None,
        },
        args: vec![Expr::Int(1), Expr::Int(2)], // Move is a struct variant
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("struct variant"));
}

#[test]
fn test_enum_struct_construct_valid() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Move".to_string()],
            type_args: None,
        },
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            ("y".to_string(), Expr::Int(20)),
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    match result.ty() {
        Type::Enum { name, .. } => assert_eq!(name, "Message"),
        _ => panic!("Expected enum type"),
    }
}

#[test]
fn test_enum_struct_construct_unit_variant_error() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Quit".to_string()],
            type_args: None,
        },
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unit variant"));
}

#[test]
fn test_enum_struct_construct_tuple_variant_error() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Write".to_string()],
            type_args: None,
        },
        fields: vec![
            ("msg".to_string(), Expr::String("hi".to_string())),
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("tuple variant"));
}

#[test]
fn test_enum_struct_construct_missing_field() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Move".to_string()],
            type_args: None,
        },
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            // Missing y
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("missing field 'y'"));
}

#[test]
fn test_enum_struct_construct_unknown_field() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Move".to_string()],
            type_args: None,
        },
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            ("y".to_string(), Expr::Int(20)),
            ("z".to_string(), Expr::Int(30)), // Unknown
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unknown field 'z'"));
}

#[test]
fn test_enum_struct_construct_field_type_mismatch() {
    let env = env_with_message_enum();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Struct {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["Message".to_string(), "Move".to_string()],
            type_args: None,
        },
        fields: vec![
            ("x".to_string(), Expr::Int(10)),
            ("y".to_string(), Expr::String("wrong".to_string())), // Wrong type
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("field 'y'") && err.message.contains("expects"));
}
