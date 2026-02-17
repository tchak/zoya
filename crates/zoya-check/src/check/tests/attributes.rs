use zoya_ast::{
    Attribute, EnumDef, Expr, FunctionDef, Item, Param, Path, Pattern, StructDef, StructKind,
    TypeAnnotation, Visibility,
};
use zoya_package::QualifiedPath;

use crate::check::check;

use super::build_test_package;

#[test]
fn test_test_attr_on_fn_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_something".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_test_attr_on_struct_is_error() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("#[test]"));
}

#[test]
fn test_test_attr_on_struct_error_message() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("#[test]"));
    assert!(err.message.contains("struct"));
}

#[test]
fn test_test_attr_on_enum_is_error() {
    let items = vec![Item::Enum(zoya_ast::EnumDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Bar".to_string(),
        type_params: vec![],
        variants: vec![],
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("#[test]"));
    assert!(err.message.contains("enum"));
}

#[test]
fn test_unknown_attr_on_fn_is_silently_discarded() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "unknown".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "foo".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_test_attr_with_params_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_bad".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("cannot have parameters"));
}

#[test]
fn test_test_attr_wrong_return_type_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_bad".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("must return () or Result"));
}

#[test]
fn test_builtin_and_test_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "builtin".to_string(),
                args: None,
            },
            Attribute {
                name: "test".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "test_bad".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("cannot have both"));
}

#[test]
fn test_private_test_fn_appears_in_definitions() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Private,
        name: "test_something".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("test_something");
    assert!(
        checked.definitions.contains_key(&path),
        "private #[test] function should appear in definitions"
    );
}

#[test]
fn test_test_fn_has_is_test_flag() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_something".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("test_something");
    let func = checked.items.get(&path).unwrap();
    assert!(func.is_test);
}

#[test]
fn test_task_attr_on_fn_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_task_attr_on_struct_is_error() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("#[task]"));
    assert!(err.message.contains("struct"));
}

#[test]
fn test_task_attr_on_enum_is_error() {
    let items = vec![Item::Enum(EnumDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Bar".to_string(),
        type_params: vec![],
        variants: vec![],
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("#[task]"));
    assert!(err.message.contains("enum"));
}

#[test]
fn test_builtin_and_task_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "builtin".to_string(),
                args: None,
            },
            Attribute {
                name: "task".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("cannot have both"));
    assert!(err.message.contains("#[builtin]"));
    assert!(err.message.contains("#[task]"));
}

#[test]
fn test_test_and_task_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "test".to_string(),
                args: None,
            },
            Attribute {
                name: "task".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.message.contains("cannot have both"));
    assert!(err.message.contains("#[test]"));
    assert!(err.message.contains("#[task]"));
}

#[test]
fn test_private_task_fn_appears_in_definitions() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Private,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("my_task");
    assert!(
        checked.definitions.contains_key(&path),
        "private #[task] function should appear in definitions"
    );
}

#[test]
fn test_task_fn_has_is_task_flag() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("my_task");
    let func = checked.items.get(&path).unwrap();
    assert!(func.is_task);
}

#[test]
fn test_task_fn_with_params_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: None,
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_task_fn_with_any_return_type_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}
