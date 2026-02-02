use zoya_ast::{BinOp, Expr, FunctionDef, Item, MatchArm, Param, Path, Pattern, TypeAnnotation};
use zoya_ir::{CheckedItem, Definition, FunctionType, Type};
use zoya_module::ModulePath;

use crate::check::{check, check_expr, check_function, TypeEnv};
use crate::definition::function_type_from_def;
use crate::unify::UnifyCtx;

use super::{build_test_module, build_test_module_with_expr, find_test_function};

#[test]
fn test_check_function_call() {
    let mut env = TypeEnv::default();
    env.register(
        "root::square".to_string(),
        Definition::Function(FunctionType {
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_function_call_wrong_arg_type() {
    let mut env = TypeEnv::default();
    env.register(
        "root::square".to_string(),
        Definition::Function(FunctionType {
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("type mismatch"));
}

#[test]
fn test_check_function_call_wrong_arity() {
    let mut env = TypeEnv::default();
    env.register(
        "root::add".to_string(),
        Definition::Function(FunctionType {
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("expects 2 arguments"));
}

#[test]
fn test_check_generic_function_call() {
    let mut ctx = UnifyCtx::new();
    let t_var = ctx.fresh_var();
    let t_id = if let Type::Var(id) = t_var { id } else { panic!() };

    let mut env = TypeEnv::default();
    env.register(
        "root::identity".to_string(),
        Definition::Function(FunctionType {
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
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_generic_function_call_float() {
    let mut ctx = UnifyCtx::new();
    let t_var = ctx.fresh_var();
    let t_id = if let Type::Var(id) = t_var { id } else { panic!() };

    let mut env = TypeEnv::default();
    env.register(
        "root::identity".to_string(),
        Definition::Function(FunctionType {
            type_params: vec!["T".to_string()],
            type_var_ids: vec![t_id],
            params: vec![Type::Var(t_id)],
            return_type: Type::Var(t_id),
        }),
    );

    // identity(3.14) should return Float
    let expr = Expr::Call {
        path: Path::simple("identity".to_string()),
        args: vec![Expr::Float(3.14)],
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Float);
}

#[test]
fn test_check_function_def() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "double".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Var("x".to_string()),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Path(Path::simple("x".to_string()))),
        },
    };

    let result = check_function(&func, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.name, "double");
    assert_eq!(result.return_type, Type::Int);
}

#[test]
fn test_check_function_def_return_type_mismatch() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "wrong".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Var("x".to_string()),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Float".to_string()))),
        body: Expr::Path(Path::simple("x".to_string())), // Returns Int, not Float
    };

    let result = check_function(&func, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("declares return type"));
}

#[test]
fn test_check_function_def_with_call() {
    let mut env = TypeEnv::default();
    env.register(
        "root::add".to_string(),
        Definition::Function(FunctionType {
            type_params: vec![],
            type_var_ids: vec![],
            params: vec![Type::Int, Type::Int],
            return_type: Type::Int,
        }),
    );

    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "double".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Var("x".to_string()),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Call {
            path: Path::simple("add".to_string()),
            args: vec![Expr::Path(Path::simple("x".to_string())), Expr::Path(Path::simple("x".to_string()))],
        },
    };

    let result = check_function(&func, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.return_type, Type::Int);
}

#[test]
fn test_function_type_from_def() {
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "add".to_string(),
        type_params: vec![],
        params: vec![
            Param {
                pattern: Pattern::Var("x".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
            Param {
                pattern: Pattern::Var("y".to_string()),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
        ],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(0), // body doesn't matter for type extraction
    };

    let env = TypeEnv::default();
    let ft = function_type_from_def(&func, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(ft.params, vec![Type::Int, Type::Int]);
    assert_eq!(ft.return_type, Type::Int);
}

#[test]
fn test_function_type_from_def_generic() {
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "identity".to_string(),
        type_params: vec!["T".to_string()],
        params: vec![Param {
            pattern: Pattern::Var("x".to_string()),
            typ: TypeAnnotation::Named(Path::simple("T".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("T".to_string()))),
        body: Expr::Int(0),
    };

    let env = TypeEnv::default();
    let ft = function_type_from_def(&func, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(ft.type_params, vec!["T".to_string()]);
    assert_eq!(ft.type_var_ids.len(), 1);
    // Params and return type should use the same type variable
    assert!(matches!(ft.params[0], Type::Var(_)));
    assert!(matches!(ft.return_type, Type::Var(_)));
}

#[test]
fn test_check_function_definition() {
    let items = vec![Item::Function(FunctionDef {
        name: "foo".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];
    let tree = build_test_module(items);
    let checked_tree = check(&tree).unwrap();
    let root = checked_tree.root().unwrap();
    assert_eq!(root.items.len(), 1);
    assert!(matches!(root.items[0], CheckedItem::Function(_)));
}

#[test]
fn test_check_function_call_in_module() {
    let items = vec![Item::Function(FunctionDef {
        name: "double".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Var("x".to_string()),
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
    let tree = build_test_module_with_expr(items, test_expr);
    let checked_tree = check(&tree).unwrap();
    let root = checked_tree.root().unwrap();
    // double + __test
    assert_eq!(root.items.len(), 2);
    assert!(matches!(root.items[0], CheckedItem::Function(_)));
    let test_fn = find_test_function(&root.items).unwrap();
    // The call expression becomes the return value of __test (returns Int)
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_check_forward_reference() {
    // fn caller() -> Int callee()
    // fn callee() -> Int 42
    // Should succeed - caller can reference callee defined later
    let items = vec![
        Item::Function(FunctionDef {
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
            name: "callee".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(42),
        }),
    ];
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_ok(), "Forward reference should succeed: {:?}", result.err());
    let checked_tree = result.unwrap();
    let root = checked_tree.root().unwrap();
    assert_eq!(root.items.len(), 2);
    // Both should be functions
    assert!(matches!(root.items[0], CheckedItem::Function(_)));
    assert!(matches!(root.items[1], CheckedItem::Function(_)));
}

#[test]
fn test_check_mutual_recursion() {
    // fn is_even(n) -> Bool { match n { 0 => true, _ => is_odd(n-1) } }
    // fn is_odd(n) -> Bool { match n { 0 => false, _ => is_even(n-1) } }
    // Should succeed - both see each other
    let items = vec![
        Item::Function(FunctionDef {
            name: "is_even".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Var("n".to_string()),
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
            name: "is_odd".to_string(),
            type_params: vec![],
            params: vec![Param {
                pattern: Pattern::Var("n".to_string()),
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
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_ok(), "Mutual recursion should succeed: {:?}", result.err());
}

#[test]
fn test_check_module_with_test_expr() {
    // Items: fn f2() -> Int f1()  (forward ref), fn f1() -> Int 42
    // Test expr: { let x = 1; x + 1 }
    // Items are processed before test expr; forward refs work
    let items = vec![
        Item::Function(FunctionDef {
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
            name: "f1".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Int(42),
        }),
    ];
    let test_expr = Expr::Block {
        bindings: vec![zoya_ast::LetBinding {
            pattern: Pattern::Var("x".to_string()),
            type_annotation: None,
            value: Box::new(Expr::Int(1)),
        }],
        result: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Int(1)),
        }),
    };
    let tree = build_test_module_with_expr(items, test_expr);
    let result = check(&tree);
    assert!(result.is_ok(), "Mixed items and expr should succeed: {:?}", result.err());
    let checked_tree = result.unwrap();
    let root = checked_tree.root().unwrap();
    // f2 + f1 + __test = 3 functions
    assert_eq!(root.items.len(), 3);
    // Verify items (f2, f1)
    assert!(matches!(root.items[0], CheckedItem::Function(_)));
    assert!(matches!(root.items[1], CheckedItem::Function(_)));
    // Verify __test function has the correct return type
    let test_fn = find_test_function(&root.items).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_check_undefined_variable_error() {
    // fn bad() -> Int x
    // Should fail: "unknown identifier 'x'" (x is not defined anywhere)
    let items = vec![Item::Function(FunctionDef {
        name: "bad".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Path(Path::simple("x".to_string())),
    })];
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_err(), "Unknown variable should fail, but got: {:?}", result);
    let err_msg = result.unwrap_err().message;
    assert!(
        err_msg.contains("unknown identifier"),
        "Expected 'unknown identifier' but got: {}", err_msg
    );
}

#[test]
fn test_check_self_recursion() {
    // fn factorial(n) -> Int { match n { 0 => 1, _ => n * factorial(n-1) } }
    // Should succeed
    let items = vec![Item::Function(FunctionDef {
        name: "factorial".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Var("n".to_string()),
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
    let tree = build_test_module(items);
    let result = check(&tree);
    assert!(result.is_ok(), "Self-recursion should succeed: {:?}", result.err());
}

#[test]
fn test_function_def_invalid_name_pascal_case() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "MyFunction".to_string(), // Should be snake_case
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    };
    let result = check_function(&func, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("should be snake_case"));
}

#[test]
fn test_function_def_invalid_type_param() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "identity".to_string(),
        type_params: vec!["bad_type".to_string()], // Should be PascalCase
        params: vec![Param {
            pattern: Pattern::Var("x".to_string()),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Path(Path::simple("x".to_string())),
    };
    let result = check_function(&func, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("type parameter") && err.message.contains("should be PascalCase"));
}

#[test]
fn test_function_def_refutable_param_pattern() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let func = FunctionDef {
        name: "bad".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Literal(Box::new(Expr::Int(42))), // Refutable
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(0),
    };
    let result = check_function(&func, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("refutable pattern in function parameter"));
}
