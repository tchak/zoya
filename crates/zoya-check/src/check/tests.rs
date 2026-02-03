use std::collections::HashMap;

use zoya_ast::{Expr, FunctionDef, Item, Visibility};
use zoya_ir::{CheckedItem, TypeError, TypedExpr, TypedFunction};
use zoya_module::{Module, ModulePath, ModuleTree};

use crate::check::{check_expr, TypeEnv};
use crate::unify::UnifyCtx;

mod binop;
mod blocks;
mod enums;
mod functions;
mod lambdas;
mod match_expr;
mod methods;
mod misc;
mod primitives;
mod structs;
mod type_aliases;
mod variables;

/// Helper to check an expression with default environment
pub fn check_expr_with_env(expr: &Expr) -> Result<TypedExpr, TypeError> {
    let mut ctx = UnifyCtx::new();
    check_expr(expr, &ModulePath::root(), &TypeEnv::default(), &mut ctx)
}

/// Build a test module from items only.
pub fn build_test_module(items: Vec<Item>) -> ModuleTree {
    let module = Module {
        items,
        path: ModulePath::root(),
        children: HashMap::new(),
    };
    let mut modules = HashMap::new();
    modules.insert(ModulePath::root(), module);
    ModuleTree { modules }
}

/// Build a test module with items and a test expression.
/// The test expression is wrapped in a synthetic `__test` function.
pub fn build_test_module_with_expr(items: Vec<Item>, test_expr: Expr) -> ModuleTree {
    let mut all_items = items;
    all_items.push(Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "__test".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: test_expr,
    }));
    build_test_module(all_items)
}

/// Find the `__test` function from checked items
pub fn find_test_function(items: &[CheckedItem]) -> Option<&TypedFunction> {
    for item in items {
        if let CheckedItem::Function(f) = item {
            if f.name == "__test" {
                return Some(f);
            }
        }
    }
    None
}
