use zoya_ast::{Expr, ListElement};
use zoya_ir::{EnumVariantType, QualifiedPath, Type};

use super::check_expr_with_env;

#[test]
fn test_list_index_returns_option() {
    let expr = Expr::ListIndex {
        expr: Box::new(Expr::List(vec![
            ListElement::Item(Expr::Int(1)),
            ListElement::Item(Expr::Int(2)),
        ])),
        index: Box::new(Expr::Int(0)),
    };
    let result = check_expr_with_env(&expr).unwrap();
    let ty = result.ty();
    assert_eq!(
        ty,
        Type::Enum {
            module: QualifiedPath::from("std::option"),
            name: "Option".to_string(),
            type_args: vec![Type::Int],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
            ],
        }
    );
}

#[test]
fn test_list_index_non_list_error() {
    let expr = Expr::ListIndex {
        expr: Box::new(Expr::Int(42)),
        index: Box::new(Expr::Int(0)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("cannot index into non-list type"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn test_list_index_non_int_index_error() {
    let expr = Expr::ListIndex {
        expr: Box::new(Expr::List(vec![ListElement::Item(Expr::Int(1))])),
        index: Box::new(Expr::Float(0.5)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("list index must be Int"),
        "unexpected error: {}",
        err
    );
}
