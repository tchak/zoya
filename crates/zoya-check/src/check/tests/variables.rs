use zoya_ast::{BinOp, Expr, Path};
use zoya_ir::{Type, TypeScheme};
use zoya_module::ModulePath;

use crate::check::{check_expr, TypeEnv};
use crate::unify::UnifyCtx;

#[test]
fn test_check_variable() {
    let mut env = TypeEnv::default();
    env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Path(Path::simple("x".to_string()));
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_check_unknown_variable() {
    let env = TypeEnv::default();
    let mut ctx = UnifyCtx::new();
    let expr = Expr::Path(Path::simple("x".to_string()));
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("unknown variable"));
}

#[test]
fn test_check_variable_in_expression() {
    let mut env = TypeEnv::default();
    env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));
    env.locals.insert("y".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::BinOp {
        op: BinOp::Add,
        left: Box::new(Expr::Path(Path::simple("x".to_string()))),
        right: Box::new(Expr::Path(Path::simple("y".to_string()))),
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}
