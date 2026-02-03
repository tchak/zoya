//! Tests for use/import statements

use std::collections::HashMap;

use zoya_ast::{
    EnumDef, EnumVariant, EnumVariantKind, Expr, FunctionDef, Item, MatchArm, Path, PathPrefix,
    Pattern, StructFieldPattern, TuplePattern, TypeAnnotation, UseDecl, UsePath, Visibility,
};
use zoya_ir::Type;
use zoya_module::{Module, ModulePath, ModuleTree};

use crate::check::check;

use super::find_test_function;

/// Build a multi-module test tree with the given modules.
/// Properly sets up parent-child relationships.
fn build_multi_module_tree(modules_data: Vec<(ModulePath, Vec<Item>, Vec<UseDecl>)>) -> ModuleTree {
    let mut modules = HashMap::new();

    // First pass: insert all modules with empty children
    for (path, items, uses) in &modules_data {
        modules.insert(
            path.clone(),
            Module {
                items: items.clone(),
                uses: uses.clone(),
                path: path.clone(),
                children: HashMap::new(),
            },
        );
    }

    // Second pass: set up parent-child relationships
    for (path, _, _) in &modules_data {
        if !path.is_root() {
            if let Some(parent_path) = path.parent() {
                let child_name = path.segments().last().unwrap().clone();
                if let Some(parent) = modules.get_mut(&parent_path) {
                    parent.children.insert(child_name, path.clone());
                }
            }
        }
    }

    ModuleTree { modules }
}

fn make_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
    UseDecl {
        path: UsePath {
            prefix,
            segments: segments.iter().map(|s| s.to_string()).collect(),
        },
    }
}

// ===== Basic Import Tests =====

#[test]
fn test_import_function_from_submodule() {
    // root module uses root::utils::helper
    // utils module has pub fn helper() -> Int
    let utils_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    // Test function calls helper (imported)
    let root_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "__test".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Call {
            path: Path::simple("helper".to_string()), // Uses import
            args: vec![],
        },
    })];

    let root_uses = vec![make_use(PathPrefix::Root, &["utils", "helper"])];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
        (ModulePath::root().child("utils"), utils_items, vec![]),
    ]);

    let result = check(&tree).unwrap();
    let root_module = result.modules.get(&ModulePath::root()).unwrap();
    let test_fn = find_test_function(&root_module.items).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_import_private_function_fails() {
    // utils module has private fn secret() -> Int
    let utils_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Private,
        name: "secret".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];

    let root_items = vec![];
    let root_uses = vec![make_use(PathPrefix::Root, &["utils", "secret"])];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
        (ModulePath::root().child("utils"), utils_items, vec![]),
    ]);

    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("private"));
}

#[test]
fn test_import_not_found_fails() {
    let root_items = vec![];
    let root_uses = vec![make_use(PathPrefix::Root, &["utils", "nonexistent"])];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
    ]);

    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("cannot find"));
}

#[test]
fn test_duplicate_import_fails() {
    // Two different functions, but same local name
    let utils_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(1),
    })];

    let other_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(2),
    })];

    let root_items = vec![];
    let root_uses = vec![
        make_use(PathPrefix::Root, &["utils", "helper"]),
        make_use(PathPrefix::Root, &["other", "helper"]),
    ];

    let mut modules = HashMap::new();
    let root_children: HashMap<String, ModulePath> = [
        ("utils".to_string(), ModulePath::root().child("utils")),
        ("other".to_string(), ModulePath::root().child("other")),
    ].into_iter().collect();

    modules.insert(
        ModulePath::root(),
        Module {
            items: root_items,
            uses: root_uses,
            path: ModulePath::root(),
            children: root_children,
        },
    );
    modules.insert(
        ModulePath::root().child("utils"),
        Module {
            items: utils_items,
            uses: vec![],
            path: ModulePath::root().child("utils"),
            children: HashMap::new(),
        },
    );
    modules.insert(
        ModulePath::root().child("other"),
        Module {
            items: other_items,
            uses: vec![],
            path: ModulePath::root().child("other"),
            children: HashMap::new(),
        },
    );

    let tree = ModuleTree { modules };
    let result = check(&tree);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("already imported"));
}

// ===== Shadowing Priority Tests =====

#[test]
fn test_local_shadows_import() {
    // Local variable should shadow imported function
    let utils_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "x".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(1),
    })];

    // Test function has local `x` that shadows the import
    let root_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "__test".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Block {
            bindings: vec![zoya_ast::LetBinding {
                pattern: zoya_ast::Pattern::Var("x".to_string()),
                type_annotation: None,
                value: Box::new(Expr::Bool(true)),
            }],
            result: Box::new(Expr::Path(Path::simple("x".to_string()))),
        },
    })];

    let root_uses = vec![make_use(PathPrefix::Root, &["utils", "x"])];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
        (ModulePath::root().child("utils"), utils_items, vec![]),
    ]);

    let result = check(&tree).unwrap();
    let root_module = result.modules.get(&ModulePath::root()).unwrap();
    let test_fn = find_test_function(&root_module.items).unwrap();
    // Should be Bool (from local), not Int (from import)
    assert_eq!(test_fn.return_type, Type::Bool);
}

#[test]
fn test_import_shadows_module_level_definition() {
    // Import should take priority over a function in the current module
    let utils_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "foo".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(1), // Returns Int
    })];

    // Root has its own `foo` function that returns Bool
    // But also imports `foo` from utils
    let root_items = vec![
        Item::Function(FunctionDef {
            visibility: Visibility::Public,
            name: "foo".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
            body: Expr::Bool(true), // Returns Bool
        }),
        Item::Function(FunctionDef {
            visibility: Visibility::Public,
            name: "__test".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("foo".to_string()),
                args: vec![],
            },
        }),
    ];

    let root_uses = vec![make_use(PathPrefix::Root, &["utils", "foo"])];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
        (ModulePath::root().child("utils"), utils_items, vec![]),
    ]);

    let result = check(&tree).unwrap();
    let root_module = result.modules.get(&ModulePath::root()).unwrap();
    let test_fn = find_test_function(&root_module.items).unwrap();
    // Should be Int (from import), not Bool (from local function)
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== Use with self:: and super:: =====

#[test]
fn test_import_with_super_prefix() {
    // Child module imports from parent using super::
    let root_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "parent_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let child_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "__test".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Call {
            path: Path::simple("parent_fn".to_string()), // Uses import from super::
            args: vec![],
        },
    })];

    let child_uses = vec![make_use(PathPrefix::Super, &["parent_fn"])];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, vec![]),
        (ModulePath::root().child("child"), child_items, child_uses),
    ]);

    let result = check(&tree).unwrap();
    let child_module = result.modules.get(&ModulePath::root().child("child")).unwrap();
    let test_fn = find_test_function(&child_module.items).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== Pattern Import Tests =====

#[test]
fn test_imported_enum_variant_in_match_pattern() {
    // types module has enum Option<T> { None, Some(T) }
    let types_items = vec![Item::Enum(EnumDef {
        name: "Option".to_string(),
        type_params: vec!["T".to_string()],
        variants: vec![
            EnumVariant {
                name: "None".to_string(),
                kind: EnumVariantKind::Unit,
            },
            EnumVariant {
                name: "Some".to_string(),
                kind: EnumVariantKind::Tuple(vec![TypeAnnotation::Named(Path::simple(
                    "T".to_string(),
                ))]),
            },
        ],
    })];

    // Root module imports Some and None, uses them in expressions and patterns
    // fn __test() -> Int
    //     match Some(42) { Some(x) => x, None => 0 }
    let root_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "__test".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Match {
            scrutinee: Box::new(Expr::Call {
                path: Path::simple("Some".to_string()), // Uses import in expression
                args: vec![Expr::Int(42)],
            }),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Call {
                        path: Path::simple("Some".to_string()), // Uses import in pattern
                        args: TuplePattern::Exact(vec![Pattern::Var("x".to_string())]),
                    },
                    result: Expr::Path(Path::simple("x".to_string())),
                },
                MatchArm {
                    pattern: Pattern::Path(Path::simple("None".to_string())), // Uses import in pattern
                    result: Expr::Int(0),
                },
            ],
        },
    })];

    let root_uses = vec![
        make_use(PathPrefix::Root, &["types", "Option", "Some"]),
        make_use(PathPrefix::Root, &["types", "Option", "None"]),
    ];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
        (ModulePath::root().child("types"), types_items, vec![]),
    ]);

    let result = check(&tree).unwrap();
    let root_module = result.modules.get(&ModulePath::root()).unwrap();
    let test_fn = find_test_function(&root_module.items).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_imported_enum_variant_in_struct_pattern() {
    // types module has enum Message { Move { x: Int, y: Int }, Quit }
    let types_items = vec![Item::Enum(EnumDef {
        name: "Message".to_string(),
        type_params: vec![],
        variants: vec![
            EnumVariant {
                name: "Move".to_string(),
                kind: EnumVariantKind::Struct(vec![
                    zoya_ast::StructFieldDef {
                        name: "x".to_string(),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                    zoya_ast::StructFieldDef {
                        name: "y".to_string(),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                ]),
            },
            EnumVariant {
                name: "Quit".to_string(),
                kind: EnumVariantKind::Unit,
            },
        ],
    })];

    // Root module imports Move and Quit, uses them in expressions and patterns
    // fn __test() -> Int
    //     match Move { x: 1, y: 2 } { Move { x, y } => x + y, Quit => 0 }
    let root_items = vec![Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: "__test".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Match {
            scrutinee: Box::new(Expr::Struct {
                path: Path::simple("Move".to_string()), // Uses import in expression
                fields: vec![
                    ("x".to_string(), Expr::Int(1)),
                    ("y".to_string(), Expr::Int(2)),
                ],
            }),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Struct {
                        path: Path::simple("Move".to_string()), // Uses import in pattern
                        fields: vec![
                            StructFieldPattern {
                                field_name: "x".to_string(),
                                pattern: Box::new(Pattern::Var("x".to_string())),
                            },
                            StructFieldPattern {
                                field_name: "y".to_string(),
                                pattern: Box::new(Pattern::Var("y".to_string())),
                            },
                        ],
                        is_partial: false,
                    },
                    result: Expr::BinOp {
                        op: zoya_ast::BinOp::Add,
                        left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                        right: Box::new(Expr::Path(Path::simple("y".to_string()))),
                    },
                },
                MatchArm {
                    pattern: Pattern::Path(Path::simple("Quit".to_string())), // Uses import in pattern
                    result: Expr::Int(0),
                },
            ],
        },
    })];

    let root_uses = vec![
        make_use(PathPrefix::Root, &["types", "Message", "Move"]),
        make_use(PathPrefix::Root, &["types", "Message", "Quit"]),
    ];

    let tree = build_multi_module_tree(vec![
        (ModulePath::root(), root_items, root_uses),
        (ModulePath::root().child("types"), types_items, vec![]),
    ]);

    let result = check(&tree).unwrap();
    let root_module = result.modules.get(&ModulePath::root()).unwrap();
    let test_fn = find_test_function(&root_module.items).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}
