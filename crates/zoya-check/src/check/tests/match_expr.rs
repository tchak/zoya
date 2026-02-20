use zoya_ast::{BinOp, Expr, MatchArm, Path, Pattern};
use zoya_ir::{Type, TypeScheme};
use zoya_package::QualifiedPath;

use crate::check::{TypeEnv, check_expr};
use crate::unify::UnifyCtx;

#[test]
fn test_check_match_with_literals() {
    let mut env = TypeEnv::default();
    env.locals
        .insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Match {
        scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
        arms: vec![
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                result: Expr::String("zero".to_string()),
            },
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Int(1))),
                result: Expr::String("one".to_string()),
            },
            MatchArm {
                pattern: Pattern::Wildcard,
                result: Expr::String("other".to_string()),
            },
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::String);
}

#[test]
fn test_check_match_with_wildcard() {
    let mut env = TypeEnv::default();
    env.locals
        .insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Match {
        scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
        arms: vec![
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                result: Expr::Int(1),
            },
            MatchArm {
                pattern: Pattern::Wildcard,
                result: Expr::Int(2),
            },
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_match_with_variable_binding() {
    let mut env = TypeEnv::default();
    env.locals
        .insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Match {
        scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
        arms: vec![MatchArm {
            pattern: Pattern::Path(Path::simple("n".to_string())),
            result: Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("n".to_string()))),
                right: Box::new(Expr::Int(1)),
            },
        }],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_match_pattern_type_mismatch() {
    let mut env = TypeEnv::default();
    env.locals
        .insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Match {
        scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
        arms: vec![MatchArm {
            pattern: Pattern::Literal(Box::new(Expr::String("hello".to_string()))),
            result: Expr::Int(1),
        }],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("pattern"));
}

#[test]
fn test_check_match_arm_type_mismatch() {
    let mut env = TypeEnv::default();
    env.locals
        .insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Match {
        scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
        arms: vec![
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Int(0))),
                result: Expr::String("zero".to_string()),
            },
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Int(1))),
                result: Expr::Int(1), // Type mismatch: String vs Int
            },
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("match arms"));
}

#[test]
fn test_match_empty_arms_exhaustiveness_warning() {
    // While this test doesn't check for empty arms directly (since the usefulness
    // checker handles it), we test that match expressions with no matching arms
    // behave correctly type-wise. The usefulness checker tests handle exhaustiveness.
    let mut env = TypeEnv::default();
    env.locals
        .insert("x".to_string(), TypeScheme::mono(Type::Bool));

    let mut ctx = UnifyCtx::new();
    // Match Bool with only one arm (non-exhaustive) - usefulness checker should catch this
    let expr = Expr::Match {
        scrutinee: Box::new(Expr::Path(Path::simple("x".to_string()))),
        arms: vec![
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Bool(true))),
                result: Expr::Int(1),
            },
            MatchArm {
                pattern: Pattern::Literal(Box::new(Expr::Bool(false))),
                result: Expr::Int(0),
            },
        ],
    };
    let result = check_expr(&expr, &QualifiedPath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}
