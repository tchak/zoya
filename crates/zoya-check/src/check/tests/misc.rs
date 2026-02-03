use std::collections::HashMap;

use zoya_ast::{Expr, LetBinding, ListPattern, Path, PathPrefix, Pattern, TuplePattern, TypeAnnotation};
use zoya_ir::{Definition, FunctionType, QualifiedPath, Type, TypeScheme, Visibility};
use zoya_module::ModulePath;

fn qpath(path: &str) -> QualifiedPath {
    QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
}

use crate::check::{check, check_expr, substitute_type_vars, TypeEnv};
use crate::unify::UnifyCtx;

use super::build_test_module_with_expr;

// ===== Type Substitution Tests =====

#[test]
fn test_substitute_type_vars_in_list() {
    let mut ctx = UnifyCtx::new();
    let var = ctx.fresh_var();
    let Type::Var(id) = var else { panic!("expected type var") };

    let mut mapping = HashMap::new();
    mapping.insert(id, Type::Int);

    let ty = Type::List(Box::new(Type::Var(id)));
    let result = substitute_type_vars(&ty, &mapping);
    assert_eq!(result, Type::List(Box::new(Type::Int)));
}

#[test]
fn test_substitute_type_vars_in_tuple() {
    let mut ctx = UnifyCtx::new();
    let var = ctx.fresh_var();
    let Type::Var(id) = var else { panic!("expected type var") };

    let mut mapping = HashMap::new();
    mapping.insert(id, Type::String);

    let ty = Type::Tuple(vec![Type::Var(id), Type::Int]);
    let result = substitute_type_vars(&ty, &mapping);
    assert_eq!(result, Type::Tuple(vec![Type::String, Type::Int]));
}

#[test]
fn test_substitute_type_vars_in_function() {
    let mut ctx = UnifyCtx::new();
    let var = ctx.fresh_var();
    let Type::Var(id) = var else { panic!("expected type var") };

    let mut mapping = HashMap::new();
    mapping.insert(id, Type::Bool);

    let ty = Type::Function {
        params: vec![Type::Var(id)],
        ret: Box::new(Type::Var(id)),
    };
    let result = substitute_type_vars(&ty, &mapping);
    assert_eq!(
        result,
        Type::Function {
            params: vec![Type::Bool],
            ret: Box::new(Type::Bool),
        }
    );
}

#[test]
fn test_substitute_type_vars_nested() {
    let mut ctx = UnifyCtx::new();
    let var = ctx.fresh_var();
    let Type::Var(id) = var else { panic!("expected type var") };

    let mut mapping = HashMap::new();
    mapping.insert(id, Type::Float);

    // List<(T, T)> -> List<(Float, Float)>
    let ty = Type::List(Box::new(Type::Tuple(vec![Type::Var(id), Type::Var(id)])));
    let result = substitute_type_vars(&ty, &mapping);
    assert_eq!(
        result,
        Type::List(Box::new(Type::Tuple(vec![Type::Float, Type::Float])))
    );
}

// ===== Let Pattern Irrefutability Tests =====

#[test]
fn test_let_literal_pattern_rejected() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Literal(Box::new(Expr::Int(42))),
            type_annotation: None,
            value: Box::new(Expr::Int(42)),
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_module_with_expr(vec![], test_expr);
    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("refutable"));
}

#[test]
fn test_let_list_pattern_rejected() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::List(ListPattern::Exact(vec![Pattern::Path(Path::simple("x".to_string()))])),
            type_annotation: None,
            value: Box::new(Expr::List(vec![Expr::Int(1)])),
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_module_with_expr(vec![], test_expr);
    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("refutable"));
}

#[test]
fn test_let_call_pattern_rejected() {
    // Don't need to set up actual enum type - irrefutability check happens before type checking
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Call {
                path: Path {
                    prefix: PathPrefix::None,
                    segments: vec!["Option".to_string(), "Some".to_string()],
                    type_args: None,
                },
                args: TuplePattern::Exact(vec![Pattern::Path(Path::simple("x".to_string()))]),
            },
            type_annotation: None,
            value: Box::new(Expr::Int(42)), // Doesn't matter, will fail at irrefutability check first
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_module_with_expr(vec![], test_expr);
    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("refutable"));
}

#[test]
fn test_let_tuple_pattern_irrefutable() {
    let test_expr = Expr::Block {
        bindings: vec![LetBinding {
            pattern: Pattern::Tuple(TuplePattern::Exact(vec![
                Pattern::Path(Path::simple("a".to_string())),
                Pattern::Path(Path::simple("b".to_string())),
            ])),
            type_annotation: None,
            value: Box::new(Expr::Tuple(vec![Expr::Int(1), Expr::Int(2)])),
        }],
        result: Box::new(Expr::Tuple(vec![])),
    };
    let tree = build_test_module_with_expr(vec![], test_expr);
    let result = check(&tree);
    // Type checking should succeed
    assert!(result.is_ok());
}

// ===== Turbofish Tests =====

#[test]
fn test_turbofish_correct_count() {
    let mut ctx = UnifyCtx::new();
    let t_var = ctx.fresh_var();
    let t_id = if let Type::Var(id) = t_var { id } else { panic!() };

    let mut env = TypeEnv::default();
    env.register(
        qpath("root::identity"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            type_params: vec!["T".to_string()],
            type_var_ids: vec![t_id],
            params: vec![Type::Var(t_id)],
            return_type: Type::Var(t_id),
        }),
    );

    // identity::<Int>(42)
    let expr = Expr::Call {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["identity".to_string()],
            type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
        },
        args: vec![Expr::Int(42)],
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx).unwrap();
    assert_eq!(result.ty(), Type::Int);
}

#[test]
fn test_turbofish_wrong_count_error() {
    let mut ctx = UnifyCtx::new();
    let t_var = ctx.fresh_var();
    let t_id = if let Type::Var(id) = t_var { id } else { panic!() };

    let mut env = TypeEnv::default();
    env.register(
        qpath("root::identity"),
        Definition::Function(FunctionType {
            visibility: Visibility::Public,
            type_params: vec!["T".to_string()],
            type_var_ids: vec![t_id],
            params: vec![Type::Var(t_id)],
            return_type: Type::Var(t_id),
        }),
    );

    // identity::<Int, String>(42) - wrong number of type args
    let expr = Expr::Call {
        path: Path {
            prefix: PathPrefix::None,
            segments: vec!["identity".to_string()],
            type_args: Some(vec![
                TypeAnnotation::Named(Path::simple("Int".to_string())),
                TypeAnnotation::Named(Path::simple("String".to_string())),
            ]),
        },
        args: vec![Expr::Int(42)],
    };
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("expects 1 type argument(s), got 2"));
}

#[test]
fn test_turbofish_on_variable_error() {
    let mut env = TypeEnv::default();
    env.locals.insert("x".to_string(), TypeScheme::mono(Type::Int));

    let mut ctx = UnifyCtx::new();
    let expr = Expr::Path(Path {
        prefix: PathPrefix::None,
        segments: vec!["x".to_string()],
        type_args: Some(vec![TypeAnnotation::Named(Path::simple("Int".to_string()))]),
    });
    let result = check_expr(&expr, &ModulePath::root(), &env, &mut ctx);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("cannot use turbofish on variable"));
}
