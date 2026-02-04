use zoya_ast::{BinOp, Expr, LambdaParam, Path, PathPrefix, Pattern, TuplePattern, TypeAnnotation};
use zoya_ir::{Type, TypeScheme};
use zoya_package::ModulePath;

use crate::check::{check_expr, TypeEnv};
use crate::unify::UnifyCtx;

use super::check_expr_with_env;

#[test]
fn test_check_lambda_basic() {
    let expr = Expr::Lambda {
        params: vec![LambdaParam {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        }],
        return_type: None,
        body: Box::new(Expr::Path(Path::simple("x".to_string()))),
    };
    let result = check_expr_with_env(&expr).unwrap();
    match result.ty() {
        Type::Function { params, ret } => {
            assert_eq!(params, vec![Type::Int]);
            assert_eq!(*ret, Type::Int);
        }
        _ => panic!("Expected function type"),
    }
}

#[test]
fn test_check_lambda_with_return_type() {
    let expr = Expr::Lambda {
        params: vec![LambdaParam {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Box::new(Expr::Path(Path::simple("x".to_string()))),
    };
    let result = check_expr_with_env(&expr).unwrap();
    match result.ty() {
        Type::Function { params, ret } => {
            assert_eq!(params, vec![Type::Int]);
            assert_eq!(*ret, Type::Int);
        }
        _ => panic!("Expected function type"),
    }
}

#[test]
fn test_check_lambda_return_type_mismatch() {
    let expr = Expr::Lambda {
        params: vec![LambdaParam {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("String".to_string()))),
        body: Box::new(Expr::Path(Path::simple("x".to_string()))),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("lambda body type") || err.message.contains("doesn't match declared return type"));
}

#[test]
fn test_check_lambda_refutable_param_error() {
    // Lambda with literal pattern (refutable) should fail
    let expr = Expr::Lambda {
        params: vec![LambdaParam {
            pattern: Pattern::Literal(Box::new(Expr::Int(42))),
            typ: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        }],
        return_type: None,
        body: Box::new(Expr::Int(1)),
    };
    let result = check_expr_with_env(&expr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("refutable pattern in lambda parameter"));
}

#[test]
fn test_check_lambda_tuple_param() {
    let expr = Expr::Lambda {
        params: vec![LambdaParam {
            pattern: Pattern::Tuple(TuplePattern::Exact(vec![
                Pattern::Path(Path::simple("x".to_string())),
                Pattern::Path(Path::simple("y".to_string())),
            ])),
            typ: Some(TypeAnnotation::Tuple(vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("Int".to_string())),
            ])),
        }],
        return_type: None,
        body: Box::new(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Path(Path::simple("x".to_string()))),
            right: Box::new(Expr::Path(Path::simple("y".to_string()))),
        }),
    };
    let result = check_expr_with_env(&expr).unwrap();
    assert!(matches!(result.ty(), Type::Function { .. }));
}

#[test]
fn test_call_lambda_variable() {
    let mut env = TypeEnv::default();
    env.locals.insert(
        "f".to_string(),
        TypeScheme::mono(Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::String),
        }),
    );

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("f".to_string()),
        args: vec![Expr::Int(42)],
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_call_non_function_error() {
    let mut env = TypeEnv::default();
    env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("x".to_string()),
        args: vec![Expr::Int(1)],
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("is not a function"));
}

#[test]
fn test_turbofish_on_lambda_error() {
    let mut env = TypeEnv::default();
    env.locals.insert(
        "f".to_string(),
        TypeScheme::mono(Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Int),
        }),
    );

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["f".to_string()],
            type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
        },
        args: vec![Expr::Int(42)],
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("cannot use turbofish on lambda"));
}

#[test]
fn test_call_lambda_wrong_arity() {
    let mut env = TypeEnv::default();
    env.locals.insert(
        "f".to_string(),
        TypeScheme::mono(Type::Function {
            params: vec![Type::Int, Type::Int],
            ret: Box::new(Type::Int),
        }),
    );

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("f".to_string()),
        args: vec![Expr::Int(42)], // Only 1 arg, needs 2
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("expects 2 arguments"));
}

#[test]
fn test_call_type_variable_infers_function() {
    // Simulates: let apply = |f, x| f(x)
    // f starts as a type variable, calling f(x) should infer f: ?a -> ?b
    let mut env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();

    // f has an unbound type variable
    let f_type = ctx.fresh_var();
    env.locals.insert("f".to_string(), TypeScheme::mono(f_type));
    env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

    let expr = Expr::Call {
        path: Path::simple("f".to_string()),
        args: vec![Expr::Path(Path::simple("x".to_string()))],
    };

    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(
        result.is_ok(),
        "calling type variable should unify to function: {:?}",
        result
    );
}

#[test]
fn test_higher_order_function_inference() {
    // |f, x| f(x) should infer: (a -> b, a) -> b
    let expr = Expr::Lambda {
        params: vec![
            LambdaParam {
                pattern: Pattern::Path(Path::simple("f".to_string())),
                typ: None,
            },
            LambdaParam {
                pattern: Pattern::Path(Path::simple("x".to_string())),
                typ: None,
            },
        ],
        return_type: None,
        body: Box::new(Expr::Call {
            path: Path::simple("f".to_string()),
            args: vec![Expr::Path(Path::simple("x".to_string()))],
        }),
    };

    let result = check_expr_with_env(&expr);
    assert!(
        result.is_ok(),
        "higher-order function should type check: {:?}",
        result
    );

    // Verify it's a function type
    let ty = result.unwrap().ty();
    assert!(matches!(ty, Type::Function { .. }));
}

#[test]
fn test_call_concrete_non_function_still_errors() {
    // Calling an Int should still fail
    let mut env = TypeEnv::default();
    env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Call {
        path: Path::simple("x".to_string()),
        args: vec![Expr::Int(1)],
    };

    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("is not a function"));
}
