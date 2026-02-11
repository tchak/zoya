//! Tests for use/import statements

use std::collections::HashMap;

use zoya_ast::{
    EnumDef, EnumVariant, EnumVariantKind, Expr, FunctionDef, Item, MatchArm, Path, PathPrefix,
    Pattern, StructFieldPattern, TuplePattern, TypeAnnotation, UseDecl, UsePath, UseTarget,
    Visibility,
};
use zoya_ir::Type;
use zoya_package::{Module, Package, QualifiedPath};

use crate::check::check;

use super::find_test_function_in;

/// Build a multi-module test package with the given modules.
/// Properly sets up parent-child relationships.
fn build_multi_module_package(modules_data: Vec<(QualifiedPath, Vec<Item>)>) -> Package {
    let mut modules = HashMap::new();

    // First pass: insert all modules with empty children
    for (path, items) in &modules_data {
        modules.insert(
            path.clone(),
            Module {
                items: items.clone(),
                path: path.clone(),
                children: HashMap::new(),
            },
        );
    }

    // Second pass: set up parent-child relationships
    for (path, _) in &modules_data {
        if *path != QualifiedPath::root()
            && let Some(parent_path) = path.parent()
        {
            let child_name = path.segments().last().unwrap().clone();
            if let Some(parent) = modules.get_mut(&parent_path) {
                parent
                    .children
                    .insert(child_name, (path.clone(), Visibility::Public));
            }
        }
    }

    Package {
        name: "test".to_string(),
        output: None,
        modules,
    }
}

fn make_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
    UseDecl {
        attributes: vec![],
        visibility: Visibility::Private,
        path: UsePath {
            prefix,
            segments: segments.iter().map(|s| s.to_string()).collect(),
            target: UseTarget::Single { alias: None },
        },
    }
}

// ===== Basic Import Tests =====

#[test]
fn test_import_function_from_submodule() {
    // root module uses root::utils::helper
    // utils module has pub fn helper() -> Int
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    // Test function calls helper (imported)
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "test_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Call {
            path: Path::simple("helper".to_string()), // Uses import
            args: vec![],
        },
    })];

    let mut root_items_with_uses =
        vec![Item::Use(make_use(PathPrefix::Root, &["utils", "helper"]))];
    root_items_with_uses.extend(root_items);

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items_with_uses),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_import_private_function_fails() {
    // utils module has private fn secret() -> Int
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Private,
        name: "secret".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];

    let root_items = vec![Item::Use(make_use(PathPrefix::Root, &["utils", "secret"]))];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("private"));
}

#[test]
fn test_import_private_struct_fails() {
    // utils module has private struct Secret
    let utils_items = vec![Item::Struct(zoya_ast::StructDef {
        attributes: vec![],
        visibility: Visibility::Private,
        name: "Secret".to_string(),
        type_params: vec![],
        kind: zoya_ast::StructKind::Unit,
    })];

    let root_items = vec![Item::Use(make_use(PathPrefix::Root, &["utils", "Secret"]))];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("private"));
}

#[test]
fn test_import_not_found_fails() {
    let root_items = vec![Item::Use(make_use(
        PathPrefix::Root,
        &["utils", "nonexistent"],
    ))];

    let tree = build_multi_module_package(vec![(QualifiedPath::root(), root_items)]);

    let result = check(&tree, &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("cannot find"));
}

#[test]
fn test_duplicate_import_fails() {
    // Two different functions, but same local name
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(1),
    })];

    let other_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(2),
    })];

    let root_items = vec![
        Item::Use(make_use(PathPrefix::Root, &["utils", "helper"])),
        Item::Use(make_use(PathPrefix::Root, &["other", "helper"])),
    ];

    let mut modules = HashMap::new();
    let root_children: HashMap<String, (QualifiedPath, Visibility)> = [
        (
            "utils".to_string(),
            (QualifiedPath::root().child("utils"), Visibility::Public),
        ),
        (
            "other".to_string(),
            (QualifiedPath::root().child("other"), Visibility::Public),
        ),
    ]
    .into_iter()
    .collect();

    modules.insert(
        QualifiedPath::root(),
        Module {
            items: root_items,
            path: QualifiedPath::root(),
            children: root_children,
        },
    );
    modules.insert(
        QualifiedPath::root().child("utils"),
        Module {
            items: utils_items,
            path: QualifiedPath::root().child("utils"),
            children: HashMap::new(),
        },
    );
    modules.insert(
        QualifiedPath::root().child("other"),
        Module {
            items: other_items,
            path: QualifiedPath::root().child("other"),
            children: HashMap::new(),
        },
    );

    let pkg = Package {
        name: "test".to_string(),
        output: None,
        modules,
    };
    let result = check(&pkg, &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("already imported"));
}

// ===== Shadowing Priority Tests =====

#[test]
fn test_local_shadows_import() {
    // Local variable should shadow imported function
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "x".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(1),
    })];

    // Test function has local `x` that shadows the import
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "test_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Block {
            bindings: vec![zoya_ast::LetBinding {
                pattern: zoya_ast::Pattern::Path(Path::simple("x".to_string())),
                type_annotation: None,
                value: Box::new(Expr::Bool(true)),
            }],
            result: Box::new(Expr::Path(Path::simple("x".to_string()))),
        },
    })];

    let mut root_items_with_uses = vec![Item::Use(make_use(PathPrefix::Root, &["utils", "x"]))];
    root_items_with_uses.extend(root_items);

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items_with_uses),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    // Should be Bool (from local), not Int (from import)
    assert_eq!(test_fn.return_type, Type::Bool);
}

#[test]
fn test_import_shadows_module_level_definition() {
    // Import should take priority over a function in the current module
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
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
            attributes: vec![],
            visibility: Visibility::Public,
            name: "foo".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Bool".to_string()))),
            body: Expr::Bool(true), // Returns Bool
        }),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("foo".to_string()),
                args: vec![],
            },
        }),
    ];

    let mut root_items_with_uses = vec![Item::Use(make_use(PathPrefix::Root, &["utils", "foo"]))];
    root_items_with_uses.extend(root_items);

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items_with_uses),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    // Should be Int (from import), not Bool (from local function)
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== Use with self:: and super:: =====

#[test]
fn test_import_with_super_prefix() {
    // Child module imports from parent using super::
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "parent_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let child_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "test_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Call {
            path: Path::simple("parent_fn".to_string()), // Uses import from super::
            args: vec![],
        },
    })];

    let mut child_items_with_uses = vec![Item::Use(make_use(PathPrefix::Super, &["parent_fn"]))];
    child_items_with_uses.extend(child_items);

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("child"), child_items_with_uses),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root().child("child")).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== Pattern Import Tests =====

#[test]
fn test_imported_enum_variant_in_match_pattern() {
    // types module has enum Option<T> { None, Some(T) }
    let types_items = vec![Item::Enum(EnumDef {
        attributes: vec![],
        visibility: Visibility::Public,
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
    // fn test_fn() -> Int
    //     match Some(42) { Some(x) => x, None => 0 }
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "test_fn".to_string(),
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
                        args: TuplePattern::Exact(vec![Pattern::Path(Path::simple(
                            "x".to_string(),
                        ))]),
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

    let mut root_items_with_uses = vec![
        Item::Use(make_use(PathPrefix::Root, &["types", "Option", "Some"])),
        Item::Use(make_use(PathPrefix::Root, &["types", "Option", "None"])),
    ];
    root_items_with_uses.extend(root_items);

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items_with_uses),
        (QualifiedPath::root().child("types"), types_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_imported_enum_variant_in_struct_pattern() {
    // types module has enum Message { Move { x: Int, y: Int }, Quit }
    let types_items = vec![Item::Enum(EnumDef {
        attributes: vec![],
        visibility: Visibility::Public,
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
    // fn test_fn() -> Int
    //     match Move { x: 1, y: 2 } { Move { x, y } => x + y, Quit => 0 }
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "test_fn".to_string(),
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
                                pattern: Box::new(Pattern::Path(Path::simple("x".to_string()))),
                            },
                            StructFieldPattern {
                                field_name: "y".to_string(),
                                pattern: Box::new(Pattern::Path(Path::simple("y".to_string()))),
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

    let mut root_items_with_uses = vec![
        Item::Use(make_use(PathPrefix::Root, &["types", "Message", "Move"])),
        Item::Use(make_use(PathPrefix::Root, &["types", "Message", "Quit"])),
    ];
    root_items_with_uses.extend(root_items);

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items_with_uses),
        (QualifiedPath::root().child("types"), types_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== pub use Re-export Tests =====

fn make_pub_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
    UseDecl {
        attributes: vec![],
        visibility: Visibility::Public,
        path: UsePath {
            prefix,
            segments: segments.iter().map(|s| s.to_string()).collect(),
            target: UseTarget::Single { alias: None },
        },
    }
}

#[test]
fn test_pub_use_reexport_function() {
    // Module A has pub fn helper() -> Int
    // Module B does pub use root::a::helper
    // Root module uses root::b::helper (the re-exported path)
    let a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let b_items = vec![Item::Use(make_pub_use(PathPrefix::Root, &["a", "helper"]))];

    let root_items = vec![
        Item::Use(make_use(PathPrefix::Root, &["b", "helper"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("helper".to_string()),
                args: vec![],
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("a"), a_items),
        (QualifiedPath::root().child("b"), b_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_pub_use_reexport_enum() {
    // Module types has pub enum Color { Red, Blue }
    // Module reexporter does pub use root::types::Color
    // Root module uses root::reexporter::Color::Red
    let types_items = vec![Item::Enum(EnumDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Color".to_string(),
        type_params: vec![],
        variants: vec![
            EnumVariant {
                name: "Red".to_string(),
                kind: EnumVariantKind::Unit,
            },
            EnumVariant {
                name: "Blue".to_string(),
                kind: EnumVariantKind::Unit,
            },
        ],
    })];

    let reexporter_items = vec![Item::Use(make_pub_use(
        PathPrefix::Root,
        &["types", "Color"],
    ))];

    let root_items = vec![
        Item::Use(make_use(PathPrefix::Root, &["reexporter", "Color", "Red"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Expr::Path(Path::simple("Red".to_string())),
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("types"), types_items),
        (QualifiedPath::root().child("reexporter"), reexporter_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert!(matches!(test_fn.return_type, Type::Enum { ref name, .. } if name == "Color"));
}

#[test]
fn test_pub_use_cannot_reexport_private() {
    // Module a has fn secret() -> Int (private)
    // Module b tries pub use root::a::secret — should fail
    let a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Private,
        name: "secret".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];

    let b_items = vec![Item::Use(make_pub_use(PathPrefix::Root, &["a", "secret"]))];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("a"), a_items),
        (QualifiedPath::root().child("b"), b_items),
    ]);

    let result = check(&tree, &[]);
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("pub use cannot re-export private"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn test_private_use_does_not_reexport() {
    // Module a has pub fn helper() -> Int
    // Module b has private use root::a::helper (no pub)
    // Root tries to use root::b::helper — should fail because it's not re-exported
    let a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let b_items = vec![Item::Use(make_use(PathPrefix::Root, &["a", "helper"]))];

    let root_items = vec![
        Item::Use(make_use(PathPrefix::Root, &["b", "helper"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Expr::Call {
                path: Path::simple("helper".to_string()),
                args: vec![],
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("a"), a_items),
        (QualifiedPath::root().child("b"), b_items),
    ]);

    let result = check(&tree, &[]);
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("cannot find"), "unexpected error: {}", msg);
}

// ===== pub use Module Re-export Tests =====

#[test]
fn test_pub_use_reexport_module() {
    // Module A has pub fn helper() -> Int
    // Module B does pub use root::a (re-exports the module)
    // Root does use root::b::a and calls a::helper()
    let a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let b_items = vec![Item::Use(make_pub_use(PathPrefix::Root, &["a"]))];

    let root_items = vec![
        Item::Use(make_use(PathPrefix::Root, &["b", "a"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path {
                    prefix: PathPrefix::None,
                    segments: vec!["a".to_string(), "helper".to_string()],
                    type_args: None,
                },
                args: vec![],
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("a"), a_items),
        (QualifiedPath::root().child("b"), b_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_pub_use_reexport_module_item_import() {
    // Module A has pub fn helper() -> Int
    // Module B does pub use root::a (re-exports the module)
    // Root does use root::b::a::helper (imports item through re-exported module)
    let a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let b_items = vec![Item::Use(make_pub_use(PathPrefix::Root, &["a"]))];

    let root_items = vec![
        Item::Use(make_use(PathPrefix::Root, &["b", "a", "helper"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Call {
                path: Path::simple("helper".to_string()),
                args: vec![],
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("a"), a_items),
        (QualifiedPath::root().child("b"), b_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== External Visibility Filtering Tests =====

/// Build a multi-module test package where child module visibility can be controlled.
/// Each entry is (path, items, visibility_of_this_module_in_parent).
/// The root module always exists and its visibility argument is ignored.
fn build_package_with_visibility(
    modules_data: Vec<(QualifiedPath, Vec<Item>, Visibility)>,
) -> Package {
    let mut modules = HashMap::new();

    for (path, items, _) in &modules_data {
        modules.insert(
            path.clone(),
            Module {
                items: items.clone(),
                path: path.clone(),
                children: HashMap::new(),
            },
        );
    }

    for (path, _, vis) in &modules_data {
        if *path != QualifiedPath::root()
            && let Some(parent_path) = path.parent()
        {
            let child_name = path.segments().last().unwrap().clone();
            if let Some(parent) = modules.get_mut(&parent_path) {
                parent.children.insert(child_name, (path.clone(), *vis));
            }
        }
    }

    Package {
        name: "test".to_string(),
        output: None,
        modules,
    }
}

#[test]
fn test_external_visibility_pub_items_in_pub_modules() {
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let helper_path = QualifiedPath::root().child("utils").child("helper");
    assert!(
        result.definitions.contains_key(&helper_path),
        "pub fn in pub module should be externally visible"
    );
}

#[test]
fn test_external_visibility_private_items_excluded() {
    let utils_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Private,
        name: "secret".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let secret_path = QualifiedPath::root().child("utils").child("secret");
    assert!(
        !result.definitions.contains_key(&secret_path),
        "private fn should not be externally visible"
    );
}

#[test]
fn test_external_visibility_pub_item_in_private_module_excluded() {
    let internal_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "helper".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];

    let tree = build_package_with_visibility(vec![
        (QualifiedPath::root(), vec![], Visibility::Public),
        (
            QualifiedPath::root().child("internal"),
            internal_items,
            Visibility::Private,
        ),
    ]);

    let result = check(&tree, &[]).unwrap();
    let helper_path = QualifiedPath::root().child("internal").child("helper");
    assert!(
        !result.definitions.contains_key(&helper_path),
        "pub fn in private module should not be externally visible"
    );
}

#[test]
fn test_external_visibility_pub_enum_variants_retained() {
    let utils_items = vec![Item::Enum(EnumDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Color".to_string(),
        type_params: vec![],
        variants: vec![
            EnumVariant {
                name: "Red".to_string(),
                kind: EnumVariantKind::Unit,
            },
            EnumVariant {
                name: "Blue".to_string(),
                kind: EnumVariantKind::Unit,
            },
        ],
    })];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("utils"), utils_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let color_path = QualifiedPath::root().child("utils").child("Color");
    let red_path = color_path.child("Red");
    let blue_path = color_path.child("Blue");

    assert!(
        result.definitions.contains_key(&color_path),
        "pub enum should be externally visible"
    );
    assert!(
        result.definitions.contains_key(&red_path),
        "variant of pub enum in pub module should be externally visible"
    );
    assert!(
        result.definitions.contains_key(&blue_path),
        "variant of pub enum in pub module should be externally visible"
    );
}

#[test]
fn test_external_visibility_deeply_nested_pub_items() {
    let deep_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "deep_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(1),
    })];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("a"), vec![]),
        (QualifiedPath::root().child("a").child("b"), vec![]),
        (
            QualifiedPath::root().child("a").child("b").child("c"),
            deep_items,
        ),
    ]);

    let result = check(&tree, &[]).unwrap();
    let deep_fn_path = QualifiedPath::root()
        .child("a")
        .child("b")
        .child("c")
        .child("deep_fn");
    assert!(
        result.definitions.contains_key(&deep_fn_path),
        "pub fn through all-pub modules should be externally visible"
    );
}

#[test]
fn test_external_visibility_private_module_blocks_deeply_nested() {
    let deep_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "deep_fn".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(1),
    })];

    let tree = build_package_with_visibility(vec![
        (QualifiedPath::root(), vec![], Visibility::Public),
        (QualifiedPath::root().child("a"), vec![], Visibility::Public),
        (
            QualifiedPath::root().child("a").child("b"),
            vec![],
            Visibility::Private,
        ),
        (
            QualifiedPath::root().child("a").child("b").child("c"),
            deep_items,
            Visibility::Public,
        ),
    ]);

    let result = check(&tree, &[]).unwrap();
    let deep_fn_path = QualifiedPath::root()
        .child("a")
        .child("b")
        .child("c")
        .child("deep_fn");
    assert!(
        !result.definitions.contains_key(&deep_fn_path),
        "pub fn behind a private module should not be externally visible"
    );
}

#[test]
fn test_external_visibility_root_level_pub_item() {
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "main".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(0),
    })];

    let tree = build_multi_module_package(vec![(QualifiedPath::root(), root_items)]);

    let result = check(&tree, &[]).unwrap();
    let main_path = QualifiedPath::root().child("main");
    assert!(
        result.definitions.contains_key(&main_path),
        "pub fn at root level should be externally visible"
    );
}

#[test]
fn test_external_visibility_root_level_private_item_excluded() {
    let root_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Private,
        name: "internal".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(0),
    })];

    let tree = build_multi_module_package(vec![(QualifiedPath::root(), root_items)]);

    let result = check(&tree, &[]).unwrap();
    let internal_path = QualifiedPath::root().child("internal");
    assert!(
        !result.definitions.contains_key(&internal_path),
        "private fn at root level should not be externally visible"
    );
}

#[test]
fn test_external_visibility_modules_themselves() {
    let tree = build_package_with_visibility(vec![
        (QualifiedPath::root(), vec![], Visibility::Public),
        (
            QualifiedPath::root().child("public_mod"),
            vec![],
            Visibility::Public,
        ),
        (
            QualifiedPath::root().child("private_mod"),
            vec![],
            Visibility::Private,
        ),
    ]);

    let result = check(&tree, &[]).unwrap();
    let pub_mod_path = QualifiedPath::root().child("public_mod");
    let priv_mod_path = QualifiedPath::root().child("private_mod");

    assert!(
        result.definitions.contains_key(&pub_mod_path),
        "pub module should be externally visible"
    );
    assert!(
        !result.definitions.contains_key(&priv_mod_path),
        "private module should not be externally visible"
    );
}

// ===== Glob import with re-exported enum variants =====

fn make_use_glob(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
    UseDecl {
        attributes: vec![],
        visibility: Visibility::Private,
        path: UsePath {
            prefix,
            segments: segments.iter().map(|s| s.to_string()).collect(),
            target: UseTarget::Glob,
        },
    }
}

fn make_pub_use_glob(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
    UseDecl {
        attributes: vec![],
        visibility: Visibility::Public,
        path: UsePath {
            prefix,
            segments: segments.iter().map(|s| s.to_string()).collect(),
            target: UseTarget::Glob,
        },
    }
}

#[test]
fn test_glob_import_includes_reexported_variants() {
    // Module "types" has pub enum Color { Red, Blue } + pub use self::Color::*
    // Root does use root::types::* and uses Red directly
    let types_items = vec![
        Item::Enum(EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Color".to_string(),
            type_params: vec![],
            variants: vec![
                EnumVariant {
                    name: "Red".to_string(),
                    kind: EnumVariantKind::Unit,
                },
                EnumVariant {
                    name: "Blue".to_string(),
                    kind: EnumVariantKind::Unit,
                },
            ],
        }),
        Item::Use(make_pub_use_glob(PathPrefix::Self_, &["Color"])),
    ];

    let root_items = vec![
        Item::Use(make_use_glob(PathPrefix::Root, &["types"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Match {
                scrutinee: Box::new(Expr::Path(Path::simple("Red".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Path(Path::simple("Red".to_string())),
                        result: Expr::Int(1),
                    },
                    MatchArm {
                        pattern: Pattern::Path(Path::simple("Blue".to_string())),
                        result: Expr::Int(2),
                    },
                ],
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("types"), types_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

#[test]
fn test_cascading_glob_reexport_across_same_depth_modules() {
    // Module "types" has pub enum Color { Red, Blue } + pub use self::Color::*
    // Module "reexporter" does pub use root::types::* (depends on types' re-exports)
    // Root does use root::reexporter::* and uses Red directly
    // This must work regardless of whether "types" or "reexporter" is processed first
    // in Phase 1.5a (they are at the same depth).
    let types_items = vec![
        Item::Enum(EnumDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Color".to_string(),
            type_params: vec![],
            variants: vec![
                EnumVariant {
                    name: "Red".to_string(),
                    kind: EnumVariantKind::Unit,
                },
                EnumVariant {
                    name: "Blue".to_string(),
                    kind: EnumVariantKind::Unit,
                },
            ],
        }),
        Item::Use(make_pub_use_glob(PathPrefix::Self_, &["Color"])),
    ];

    let reexporter_items = vec![Item::Use(make_pub_use_glob(PathPrefix::Root, &["types"]))];

    let root_items = vec![
        Item::Use(make_use_glob(PathPrefix::Root, &["reexporter"])),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
            body: Expr::Match {
                scrutinee: Box::new(Expr::Path(Path::simple("Red".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Path(Path::simple("Red".to_string())),
                        result: Expr::Int(1),
                    },
                    MatchArm {
                        pattern: Pattern::Path(Path::simple("Blue".to_string())),
                        result: Expr::Int(2),
                    },
                ],
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), root_items),
        (QualifiedPath::root().child("types"), types_items),
        (QualifiedPath::root().child("reexporter"), reexporter_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root()).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}

// ===== Cross-Module Type Resolution Tests =====
// These tests verify that same-depth sibling modules can reference each other's
// types regardless of iteration order (the 3-pass registration fix).

#[test]
fn test_cross_module_function_returns_sibling_struct() {
    // Module "a" has: pub fn make_point() -> root::b::Point { ... }
    // Module "b" has: pub struct Point { x: Int, y: Int }
    // This tests that function signatures in module "a" can reference
    // types from sibling module "b" regardless of processing order.

    let mod_b_items = vec![Item::Struct(zoya_ast::StructDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Point".to_string(),
        type_params: vec![],
        kind: zoya_ast::StructKind::Named(vec![
            zoya_ast::StructFieldDef {
                name: "x".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
            zoya_ast::StructFieldDef {
                name: "y".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
        ]),
    })];

    let mod_a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "make_point".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path {
            prefix: PathPrefix::Root,
            segments: vec!["b".to_string(), "Point".to_string()],
            type_args: None,
        })),
        body: Expr::Struct {
            path: Path {
                prefix: PathPrefix::Root,
                segments: vec!["b".to_string(), "Point".to_string()],
                type_args: None,
            },
            fields: vec![
                ("x".to_string(), Expr::Int(1)),
                ("y".to_string(), Expr::Int(2)),
            ],
        },
    })];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("a"), mod_a_items),
        (QualifiedPath::root().child("b"), mod_b_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let make_point = result
        .items
        .get(&QualifiedPath::root().child("a").child("make_point"))
        .expect("make_point function should be checked");
    assert!(
        matches!(&make_point.return_type, Type::Struct { name, .. } if name == "Point"),
        "expected Point struct return type, got {:?}",
        make_point.return_type
    );
}

#[test]
fn test_cross_module_function_param_uses_sibling_type() {
    // Module "a" has: pub fn use_point(p: root::b::Point) -> Int { ... }
    // Module "b" has: pub struct Point { x: Int, y: Int }

    let mod_b_items = vec![Item::Struct(zoya_ast::StructDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Point".to_string(),
        type_params: vec![],
        kind: zoya_ast::StructKind::Named(vec![
            zoya_ast::StructFieldDef {
                name: "x".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
            zoya_ast::StructFieldDef {
                name: "y".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
        ]),
    })];

    let mod_a_items = vec![Item::Function(FunctionDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "use_point".to_string(),
        type_params: vec![],
        params: vec![zoya_ast::Param {
            pattern: Pattern::Path(Path::simple("p".to_string())),
            typ: TypeAnnotation::Named(Path {
                prefix: PathPrefix::Root,
                segments: vec!["b".to_string(), "Point".to_string()],
                type_args: None,
            }),
        }],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::FieldAccess {
            expr: Box::new(Expr::Path(Path::simple("p".to_string()))),
            field: "x".to_string(),
        },
    })];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("a"), mod_a_items),
        (QualifiedPath::root().child("b"), mod_b_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let use_point = result
        .items
        .get(&QualifiedPath::root().child("a").child("use_point"))
        .expect("use_point function should be checked");
    assert_eq!(use_point.return_type, Type::Int);
}

#[test]
fn test_cross_module_struct_field_references_sibling_type() {
    // Module "a" has: pub struct Wrapper { inner: root::b::Point }
    // Module "b" has: pub struct Point { x: Int, y: Int }
    // This tests that struct field type resolution works across sibling modules.

    let mod_b_items = vec![Item::Struct(zoya_ast::StructDef {
        attributes: vec![],
        visibility: Visibility::Public,
        name: "Point".to_string(),
        type_params: vec![],
        kind: zoya_ast::StructKind::Named(vec![
            zoya_ast::StructFieldDef {
                name: "x".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
            zoya_ast::StructFieldDef {
                name: "y".to_string(),
                typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
            },
        ]),
    })];

    let mod_a_items = vec![
        Item::Struct(zoya_ast::StructDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "Wrapper".to_string(),
            type_params: vec![],
            kind: zoya_ast::StructKind::Named(vec![zoya_ast::StructFieldDef {
                name: "inner".to_string(),
                typ: TypeAnnotation::Named(Path {
                    prefix: PathPrefix::Root,
                    segments: vec!["b".to_string(), "Point".to_string()],
                    type_args: None,
                }),
            }]),
        }),
        Item::Function(FunctionDef {
            attributes: vec![],
            visibility: Visibility::Public,
            name: "test_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Expr::FieldAccess {
                expr: Box::new(Expr::FieldAccess {
                    expr: Box::new(Expr::Struct {
                        path: Path::simple("Wrapper".to_string()),
                        fields: vec![(
                            "inner".to_string(),
                            Expr::Struct {
                                path: Path {
                                    prefix: PathPrefix::Root,
                                    segments: vec!["b".to_string(), "Point".to_string()],
                                    type_args: None,
                                },
                                fields: vec![
                                    ("x".to_string(), Expr::Int(10)),
                                    ("y".to_string(), Expr::Int(20)),
                                ],
                            },
                        )],
                    }),
                    field: "inner".to_string(),
                }),
                field: "x".to_string(),
            },
        }),
    ];

    let tree = build_multi_module_package(vec![
        (QualifiedPath::root(), vec![]),
        (QualifiedPath::root().child("a"), mod_a_items),
        (QualifiedPath::root().child("b"), mod_b_items),
    ]);

    let result = check(&tree, &[]).unwrap();
    let test_fn = find_test_function_in(&result, &QualifiedPath::root().child("a")).unwrap();
    assert_eq!(test_fn.return_type, Type::Int);
}
