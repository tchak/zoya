use zoya_ast::{
    Attribute, BinOp, Expr, FunctionDef, Item, MatchArm, Param, Path, Pattern, TypeAnnotation,
    Visibility,
};
use zoya_ir::{Definition, FunctionKind, FunctionType, QualifiedPath, Type};

use crate::check::{TypeEnv, check, check_expr, check_function};
use crate::definition::function_type_from_def;
use crate::unify::UnifyCtx;

use super::{build_test_package, build_test_package_with_expr, find_test_function_in};

fn qpath(path: &str) -> QualifiedPath {
    QualifiedPath::from(path)
}

#[test]
fn test_check_function_call() {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::square"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            type_params: vec![],
            type_var_ids: vec![],
            params: vec![Type::Int],
            return_type: Type::Int,
        }),
    );

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("square".to_string()),
        args: vec![Expr::Int(5)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_function_call_wrong_arg_type() {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::square"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            type_params: vec![],
            type_var_ids: vec![],
            params: vec![Type::Int],
            return_type: Type::Int,
        }),
    );

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("square".to_string()),
        args: vec![Expr::Float(5.0)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("type mismatch"));
}

#[test]
fn test_check_function_call_wrong_arity() {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::add"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            type_params: vec![],
            type_var_ids: vec![],
            params: vec![Type::Int, Type::Int],
            return_type: Type::Int,
        }),
    );

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("add".to_string()),
        args: vec![Expr::Int(1)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("expects 2 arguments")
    );
}

#[test]
fn test_check_generic_function_call() {
    let mut ctx = UnifyCtx::new();
    let t_var = ctx.fresh_var();
    let t_id = if let Type::Var(id) = t_var {
        id
    } else {
        panic!()
    };

    let mut env = TypeEnv::default();
    env.register(
        qpath("root::identity"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            type_params: vec!["T".to_string()],
            type_var_ids: vec![t_id],
            params: vec![Type::Var(t_id)],
            return_type: Type::Var(t_id),
        }),
    );

    // identity(42) should return Int
    let expr = Expr::Call {
        path: Path::simple("identity".to_string()),
        args: vec![Expr::Int(42)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_generic_function_call_float() {
    let mut ctx = UnifyCtx::new();
    let t_var = ctx.fresh_var();
    let t_id = if let Type::Var(id) = t_var {
        id
    } else {
        panic!()
    };

    let mut env = TypeEnv::default();
    env.register(
        qpath("root::identity"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            type_params: vec!["T".to_string()],
            type_var_ids: vec![t_id],
            params: vec![Type::Var(t_id)],
            return_type: Type::Var(t_id),
        }),
    );

    // identity(3.15) should return Float
    let expr = Expr::Call {
        path: Path::simple("identity".to_string()),
        args: vec![Expr::Float(3.15)],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Float);
}

#[test]
fn test_check_function_def() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "double".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Path(Path::simple("x".to_string()))),
        },
    };

    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test").unwrap();
    assert_eq!(result.name, "double");
    assert_eq!(result.return_type, Type::Int);
}

#[test]
fn test_check_function_def_return_type_mismatch() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "wrong".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Float".to_string()))),
        body: Expr::Path(Path::simple("x".to_string())), // Returns Int, not Float
    };

    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("expected"));
}

#[test]
fn test_check_function_def_with_call() {
    let mut env = TypeEnv::default();
    env.register(
        qpath("root::add"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            module: QualifiedPath::root(),
            type_params: vec![],
            type_var_ids: vec![],
            params: vec![Type::Int, Type::Int],
            return_type: Type::Int,
        }),
    );

    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "double".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Call {
            path: Path::simple("add".to_string()),
            args: vec![
                Expr::Path(Path::simple("x".to_string())),
                Expr::Path(Path::simple("x".to_string())),
            ],
        },
    };

    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test").unwrap();
    assert_eq!(result.return_type, Type::Int);
}

#[test]
fn test_function_type_from_def() {
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "add".to_string(),
        type_params: vec![],
        params: vec![
            Param {
                pattern: Pattern::Path(Path::simple("x".to_string())),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
            Param {
                pattern: Pattern::Path(Path::simple("y".to_string())),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
        ],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(0), // body doesn't matter for type extraction
    };

    let env = TypeEnv::default();
    let ft = function_type_from_def(&func, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(ft.params, vec![Type::Int, Type::Int]);
    assert_eq!(ft.return_type, Type::Int);
}

#[test]
fn test_function_type_from_def_generic() {
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "identity".to_string(),
        type_params: vec!["T".to_string()],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("T".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("T".to_string()))),
        body: Expr::Int(0),
    };

    let env = TypeEnv::default();
    let ft = function_type_from_def(&func, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(ft.type_params, vec!["T".to_string()]);
    assert_eq!(ft.type_var_ids.len(), 1);
    // Params and return type should use the same type variable
    assert!(matches!(ft.params[0], Type::Var(_)));
    assert!(matches!(ft.return_type, Type::Var(_)));
}

#[test]
fn test_check_function_definition() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "foo".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];
    let tree = build_test_package(items);
    let checked_tree = check(&tree, &[]).unwrap();
    assert_eq!(checked_tree.items.len(), 1);
}

#[test]
fn test_check_function_call_in_module() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "double".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Path(Path::simple("x".to_string()))),
        },
    })];
    let test_expr = Expr::Call {
        path: Path::simple("double".to_string()),
        args: vec![Expr::Int(5)],
    };
    let tree = build_test_package_with_expr(items, test_expr);
    let checked_tree = check(&tree, &[]).unwrap();
    // double + test_fn
    assert_eq!(checked_tree.items.len(), 2);
    let test_fn = find_test_function_in(&checked_tree, &QualifiedPath::root()).unwrap();
    // The call expression becomes the return value of test_fn (returns Int)
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_check_forward_reference() {
    // fn caller() -> Int callee()
    // fn callee() -> Int 42
    // Should succeed - caller can reference callee defined later
    let items = vec![
        Item::Function(FunctionDef {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Public,
            name: "caller".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("callee".to_string()),
                args: vec![],
            },
        }),
        Item::Function(FunctionDef {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Public,
            name: "callee".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(42),
        }),
    ];
    let tree = build_test_package(items);
    let result = check(&tree, &[]);
    assert!(
        result.is_ok(),
        "Forward reference should succeed: {:?}",
        result.err()
    );
    let checked_tree = result.unwrap();
    assert_eq!(checked_tree.items.len(), 2);
}

#[test]
fn test_check_mutual_recursion() {
    // fn is_even(n) -> Bool { match n { 0 => true, _ => is_odd(n-1) } }
    // fn is_odd(n) -> Bool { match n { 0 => false, _ => is_even(n-1) } }
    // Should succeed - both see each other
    let items = vec![
        Item::Function(FunctionDef {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Public,
            name: "is_even".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Path(Path::simple("n".to_string())),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
            body: Expr::Match {
                scrutinee: Box::new(Expr::Path(Path::simple("n".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                        result: Expr::Bool(true),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        result: Expr::Call {
                            path: Path::simple("is_odd".to_string()),
                            args: vec![Expr::BinOp {
                                op: BinOp::Sub,
                                left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                                right: Box::new(Expr::Int(1)),
                            }],
                        },
                    },
                ],
            },
        }),
        Item::Function(FunctionDef {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Public,
            name: "is_odd".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Path(Path::simple("n".to_string())),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            }],
            return_type: Some(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
            body: Expr::Match {
                scrutinee: Box::new(Expr::Path(Path::simple("n".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                        result: Expr::Bool(false),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        result: Expr::Call {
                            path: Path::simple("is_even".to_string()),
                            args: vec![Expr::BinOp {
                                op: BinOp::Sub,
                                left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                                right: Box::new(Expr::Int(1)),
                            }],
                        },
                    },
                ],
            },
        }),
    ];
    let tree = build_test_package(items);
    let result = check(&tree, &[]);
    assert!(
        result.is_ok(),
        "Mutual recursion should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_check_module_with_test_expr() {
    // Items: fn f2() -> Int f1()  (forward ref), fn f1() -> Int 42
    // Test expr: { let x = 1; x + 1 }
    // Items are processed before test expr; forward refs work
    let items = vec![
        Item::Function(FunctionDef {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Public,
            name: "f2".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("f1".to_string()),
                args: vec![],
            },
        }),
        Item::Function(FunctionDef {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Public,
            name: "f1".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(42),
        }),
    ];
    let test_expr = Expr::Block {
        bindings: vec![zoya_ast::LetBinding {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            type_annotation: None,
            value: Box::new(Expr::Int(1)),
        }],
        result: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Int(1)),
        }),
    };
    let tree = build_test_package_with_expr(items, test_expr);
    let result = check(&tree, &[]);
    assert!(
        result.is_ok(),
        "Mixed items and expr should succeed: {:?}",
        result.err()
    );
    let checked_tree = result.unwrap();
    // f2 + f1 + test_fn = 3 functions
    assert_eq!(checked_tree.items.len(), 3);
    // Verify test_fn function has the correct return type
    let test_fn = find_test_function_in(&checked_tree, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_check_undefined_variable_error() {
    // fn bad() -> Int x
    // Should fail: "unknown identifier 'x'" (x is not defined anywhere)
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "bad".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Path(Path::simple("x".to_string())),
    })];
    let tree = build_test_package(items);
    let result = check(&tree, &[]);
    assert!(
        result.is_err(),
        "Unknown variable should fail, but got: {:?}",
        result
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("unknown identifier"),
        "Expected 'unknown identifier' but got: {}",
        err_msg
    );
}

#[test]
fn test_check_self_recursion() {
    // fn factorial(n) -> Int { match n { 0 => 1, _ => n * factorial(n-1) } }
    // Should succeed
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "factorial".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("n".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Match {
            scrutinee: Box::new(Expr::Path(Path::simple("n".to_string()))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                    result: Expr::Int(1),
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    result: Expr::BinOp {
                        op: BinOp::Mul,
                        left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                        right: Box::new(Expr::Call {
                            path: Path::simple("factorial".to_string()),
                            args: vec![Expr::BinOp {
                                op: BinOp::Sub,
                                left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                                right: Box::new(Expr::Int(1)),
                            }],
                        }),
                    },
                },
            ],
        },
    })];
    let tree = build_test_package(items);
    let result = check(&tree, &[]);
    assert!(
        result.is_ok(),
        "Self-recursion should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_function_def_invalid_name_pascal_case() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "MyFunction".to_string(), // Should be snake_case
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("should be snake_case"));
}

#[test]
fn test_function_def_invalid_type_param() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "identity".to_string(),
        type_params: vec!["bad_type".to_string()], // Should be PascalCase
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Path(Path::simple("x".to_string())),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("type parameter")
            && err.to_string().contains("should be PascalCase")
    );
}

#[test]
fn test_function_def_refutable_param_pattern() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "bad".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Literal(Box::new(Expr::Int(42))), // Refutable
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(0),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("refutable pattern in function parameter")
    );
}

#[test]
fn test_builtin_not_allowed_outside_std() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "builtin".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_builtin".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Tuple(vec![]),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "test");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("can only be used in the standard library")
    );
}

#[test]
fn test_builtin_requires_explicit_return_type() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "builtin".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_builtin".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "std");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("must have an explicit return type")
    );
}

#[test]
fn test_builtin_requires_unit_body() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "builtin".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_builtin".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "std");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("must have a unit body"));
}

#[test]
fn test_builtin_valid_in_std() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "builtin".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_builtin".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Tuple(vec![]),
    };
    let result = check_function(&func, &QualifiedPath::root(), &env, &mut ctx, "std");
    assert!(result.is_ok());
    let typed = result.unwrap();
    assert_eq!(typed.kind, FunctionKind::Builtin);
    assert_eq!(typed.return_type, Type::Int);
}
