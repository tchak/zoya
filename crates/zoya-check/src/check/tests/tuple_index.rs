use zoya_ast::{Expr, Item, Path, StructDef, StructKind, TupleElement, TypeAnnotation, Visibility};
use zoya_ir::{QualifiedPath, Type};

use crate::check::check;

use super::{build_test_package_with_expr, check_expr_with_env, find_test_function_in};

#[test]
fn test_tuple_index_basic() {
    let expr = Expr::TupleIndex {
        expr: Box::new(Expr::Tuple(vec![
            TupleElement::Item(Expr::Int(1)),
            TupleElement::Item(Expr::String("hello".into())),
        ])),
        index: 0,
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_tuple_index_second() {
    let expr = Expr::TupleIndex {
        expr: Box::new(Expr::Tuple(vec![
            TupleElement::Item(Expr::Int(1)),
            TupleElement::Item(Expr::String("hello".into())),
        ])),
        index: 1,
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_tuple_index_out_of_bounds() {
    let expr = Expr::TupleIndex {
        expr: Box::new(Expr::Tuple(vec![TupleElement::Item(Expr::Int(1))])),
        index: 2,
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("out of bounds"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn test_tuple_index_on_non_tuple_error() {
    let expr = Expr::TupleIndex {
        expr: Box::new(Expr::Int(42)),
        index: 0,
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("cannot use tuple index"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn test_tuple_struct_index() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Pair".to_string(),
        type_params: vec![],
        kind: StructKind::Tuple(vec![
            TypeAnnotation::Named(Path::simple("Int".to_string())),
            TypeAnnotation::Named(Path::simple("String".to_string())),
        ]),
    })];
    let test_expr = Expr::TupleIndex {
        expr: Box::new(Expr::Call {
            path: Path::simple("Pair".to_string()),
            args: vec![Expr::Int(42), Expr::String("hi".into())],
        }),
        index: 0,
    };
    let tree = build_test_package_with_expr(items, test_expr);
    let checked = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&checked, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_tuple_struct_index_out_of_bounds() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Wrapper".to_string(),
        type_params: vec![],
        kind: StructKind::Tuple(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
    })];
    let test_expr = Expr::TupleIndex {
        expr: Box::new(Expr::Call {
            path: Path::simple("Wrapper".to_string()),
            args: vec![Expr::Int(42)],
        }),
        index: 1,
    };
    let tree = build_test_package_with_expr(items, test_expr);
    let result = check(&tree, &[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("cannot use tuple index"),
        "unexpected error: {}",
        err.message
    );
}
