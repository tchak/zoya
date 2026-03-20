use std::collections::HashMap;

use zoya_ast::{Expr, FunctionDef, Item, Visibility};
use zoya_ir::{CheckedPackage, TypeError, TypedExpr, TypedFunction};
use zoya_package::{Module, Package, QualifiedPath};

use crate::check::{TypeEnv, check_expr};
use crate::unify::UnifyCtx;

mod attributes;
mod binop;
mod blocks;
mod enums;
mod functions;
mod impl_blocks;
mod imports;
mod index;
mod interpolated_string;
mod lambdas;
mod list_spread;
mod match_expr;
mod methods;
mod misc;
mod primitives;
mod structs;
mod tuple_index;
mod type_aliases;
mod variables;

/// Helper to check an expression with default environment
pub fn check_expr_with_env(expr: &Expr) -> Result<TypedExpr, TypeError> {
    let mut ctx = UnifyCtx::new();
    check_expr(expr, &QualifiedPath::root(), &TypeEnv::default(), &mut ctx)
}

/// Build a test package from items only.
pub fn build_test_package(items: Vec<Item>) -> Package {
    let module = Module {
        items,
        path: QualifiedPath::root(),
        children: HashMap::new(),
    };
    let mut modules = HashMap::new();
    modules.insert(QualifiedPath::root(), module);
    Package {
        name: "test".to_string(),
        modules,
    }
}

/// Build a test package with items and a test expression.
/// The test expression is wrapped in a synthetic `test_fn` function.
pub fn build_test_package_with_expr(items: Vec<Item>, test_expr: Expr) -> Package {
    let mut all_items = items;
    all_items.push(Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![],
        visibility: Visibility::Public,
        name: "test_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: test_expr,
    }));
    build_test_package(all_items)
}

/// Find the `test_fn` function from checked package at the given module path
pub fn find_test_function_in<'a>(
    pkg: &'a CheckedPackage,
    module_path: &QualifiedPath,
) -> Option<&'a TypedFunction> {
    pkg.items
        .get(&module_path.child("test_fn"))
        .and_then(|v| v.first())
}
