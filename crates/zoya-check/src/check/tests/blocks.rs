use zoya_ast::{BinOp, Expr, LetBinding, Path, Pattern, TypeAnnotation};
use zoya_ir::Type;

use crate::check::check;

use super::{build_test_package_with_expr, check_expr_with_env, find_test_function_in};
use zoya_package::QualifiedPath;

#[test]
fn test_check_let_binding_in_block() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            type_annotation: None,
            value: Box::new(Expr::Int(42)),
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_package_with_expr(vec![], test_expr);
    let checked_tree = check(&tree, &[]).unwrap();
    // Only the test_fn function should be present
    assert_eq!(checked_tree.items.len(), 1);
    let test_fn = find_test_function_in(&checked_tree, &QualifiedPath::root()).unwrap();
    // The test_fn function body is a block with the let binding
    // Since there's no result expression, the return type is Unit
    assert_eq!(test_fn.return_type, Type::Tuple(vec![]));
}

#[test]
fn test_check_let_binding_usage() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            type_annotation: None,
            value: Box::new(Expr::Int(42)),
        }],
        result: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Int(1)),
        }),
    };
    let tree = build_test_package_with_expr(vec![], test_expr);
    let checked_tree = check(&tree, &[]).unwrap();
    // Only test_fn function
    assert_eq!(checked_tree.items.len(), 1);
    let test_fn = find_test_function_in(&checked_tree, &QualifiedPath::root()).unwrap();
    // The expression x + 1 returns Int
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_check_let_with_type_annotation() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            type_annotation: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            value: Box::new(Expr::Int(42)),
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_package_with_expr(vec![], test_expr);
    let checked_tree = check(&tree, &[]).unwrap();
    // Only test_fn function
    assert_eq!(checked_tree.items.len(), 1);
    // Type checking succeeded
    assert!(find_test_function_in(&checked_tree, &QualifiedPath::root()).is_some());
}

#[test]
fn test_check_let_type_mismatch() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            type_annotation: Some(TypeAnnotation::Named(Path::simple("Float".to_string()))),
            value: Box::new(Expr::Int(42)),
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_package_with_expr(vec![], test_expr);
    let result = check(&tree, &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("let binding"));
}

#[test]
fn test_check_block_expression() {
    let expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            type_annotation: None,
            value: Box::new(Expr::Int(1)),
        }],
        result: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Int(2)),
        }),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_block_multiple_bindings() {
    let expr = Expr::Block {
        bindings: vec![
            LetBinding {
                pattern: Pattern::Path(Path::simple("x".to_string())),
                type_annotation: None,
                value: Box::new(Expr::Int(1)),
            },
            LetBinding {
                pattern: Pattern::Path(Path::simple("y".to_string())),
                type_annotation: None,
                value: Box::new(Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                    right: Box::new(Expr::Int(1)),
                }),
            },
        ],
        result: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Path(Path::simple("y".to_string()))),
        }),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert_eq!(result.ty(), Type::Int);
}
