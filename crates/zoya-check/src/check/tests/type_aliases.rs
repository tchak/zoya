use zoya_ast::{Expr, FunctionDef, Item, Path, TypeAliasDef, TypeAnnotation, Visibility};

use crate::check::check;

use super::build_test_module;

#[test]
fn test_type_alias_simple() {
    // type UserId = Int
    // fn get_id() -> UserId { 42 }
    let items = vec![
        Item::TypeAlias(TypeAliasDef {
            name: "UserId".to_string(),
            type_params: vec![],
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }),
        Item::Function(FunctionDef {
            visibility: Visibility::Public,
            name: "get_id".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("UserId".to_string()))),
            body: Expr::Int(42),
        }),
    ];
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_ok());
}

#[test]
fn test_type_alias_generic() {
    // type Pair<A, B> = (A, B)
    // fn make_pair() -> Pair<Int, Bool> { (1, true) }
    let items = vec![
        Item::TypeAlias(TypeAliasDef {
            name: "Pair".to_string(),
            type_params: vec!["A".to_string(), "B".to_string()],
            typ: TypeAnnotation::Tuple(vec![
                TypeAnnotation::Named(Path::simple("A".to_string())),
                TypeAnnotation::Named(Path::simple("B".to_string())),
            ]),
        }),
        Item::Function(FunctionDef {
            visibility: Visibility::Public,
            name: "make_pair".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Parameterized(
                Path::simple("Pair".to_string()),
                vec![
                    TypeAnnotation::Named(Path::simple("Int".to_string())),
                    TypeAnnotation::Named(Path::simple("Bool".to_string())),
                ],
            )),
            body: Expr::Tuple(vec![Expr::Int(1), Expr::Bool(true)]),
        }),
    ];
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_ok());
}

#[test]
fn test_type_alias_non_pascal_case_error() {
    // type userId = Int  -- should fail
    let items = vec![Item::TypeAlias(TypeAliasDef {
        name: "userId".to_string(),
        type_params: vec![],
        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
    })];
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("PascalCase"));
}

#[test]
fn test_type_alias_wrong_arity_error() {
    // type Pair<A, B> = (A, B)
    // fn bad() -> Pair<Int> { ... }  -- should fail, needs 2 args
    let items = vec![
        Item::TypeAlias(TypeAliasDef {
            name: "Pair".to_string(),
            type_params: vec!["A".to_string(), "B".to_string()],
            typ: TypeAnnotation::Tuple(vec![
                TypeAnnotation::Named(Path::simple("A".to_string())),
                TypeAnnotation::Named(Path::simple("B".to_string())),
            ]),
        }),
        Item::Function(FunctionDef {
            visibility: Visibility::Public,
            name: "bad".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Parameterized(
                Path::simple("Pair".to_string()),
                vec![TypeAnnotation::Named(Path::simple("Int".to_string()))],
            )),
            body: Expr::Tuple(vec![Expr::Int(1), Expr::Int(2)]),
        }),
    ];
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("type argument"));
}
