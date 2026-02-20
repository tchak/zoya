use zoya_ast::{Expr, ListElement};
use zoya_ir::{Type, TypedExpr, TypedListElement};

use super::check_expr_with_env;

#[test]
fn test_list_spread_basic() {
    // [..xs] where xs: List<Int>
    // We can test by spreading a list literal: [..[1, 2]]
    let inner = Expr::List(vec![
        ListElement::Item(Expr::Int(1)),
        ListElement::Item(Expr::Int(2)),
    ]);
    let expr = Expr::List(vec![ListElement::Spread(inner)]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
    match &result {
        TypedExpr::List { elements, .. } => {
            assert_eq!(elements.len(), 1);
            assert!(matches!(&elements[0], TypedListElement::Spread(_)));
        }
        _ => panic!("expected List"),
    }
}

#[test]
fn test_list_spread_with_items() {
    // [1, ..[2, 3], 4]
    let spread_list = Expr::List(vec![
        ListElement::Item(Expr::Int(2)),
        ListElement::Item(Expr::Int(3)),
    ]);
    let expr = Expr::List(vec![
        ListElement::Item(Expr::Int(1)),
        ListElement::Spread(spread_list),
        ListElement::Item(Expr::Int(4)),
    ]);
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::List(Box::new(Type::Int)));
    match &result {
        TypedExpr::List { elements, .. } => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(&elements[0], TypedListElement::Item(_)));
            assert!(matches!(&elements[1], TypedListElement::Spread(_)));
            assert!(matches!(&elements[2], TypedListElement::Item(_)));
        }
        _ => panic!("expected List"),
    }
}

#[test]
fn test_list_spread_non_list_error() {
    // [..42] should fail — 42 is Int, not List<_>
    let expr = Expr::List(vec![ListElement::Spread(Expr::Int(42))]);
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("type mismatch"));
}

#[test]
fn test_list_spread_type_mismatch_error() {
    // [1, ..[true]] — item is Int, spread is List<Bool>
    let spread_list = Expr::List(vec![ListElement::Item(Expr::Bool(true))]);
    let expr = Expr::List(vec![
        ListElement::Item(Expr::Int(1)),
        ListElement::Spread(spread_list),
    ]);
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("type mismatch"));
}
