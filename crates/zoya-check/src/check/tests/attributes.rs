use zoya_ast::{Attribute, Expr, FunctionDef, Item, StructDef, StructKind, Visibility};

use crate::check::check;

use super::build_test_package;

#[test]
fn test_test_attr_on_fn_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_something".to_string(),
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
fn test_test_attr_on_struct_is_error() {
    let items = vec![Item::Struct(StructDef {
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
