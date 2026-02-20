use zoya_ast::{
    Attribute, AttributeArg, EnumDef, Expr, FunctionDef, Item, Param, Path, Pattern, StructDef,
    StructKind, TypeAnnotation, Visibility,
};
use zoya_loader::{MemorySource, load_memory_package};
use zoya_package::QualifiedPath;

use crate::check::check;

use super::build_test_package;

fn check_source(source: &str) -> Result<zoya_ir::CheckedPackage, String> {
    let mem = MemorySource::new().with_module("root", source);
    let pkg = load_memory_package(&mem, zoya_loader::Mode::Dev).map_err(|e| format!("{}", e))?;
    let std = zoya_std::std();
    check(&pkg, &[std]).map_err(|e| e.to_string())
}

#[test]
fn test_test_attr_on_fn_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_something".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_test_attr_on_struct_is_error() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[test]"));
}

#[test]
fn test_test_attr_on_struct_error_message() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[test]"));
    assert!(err.to_string().contains("struct"));
}

#[test]
fn test_test_attr_on_enum_is_error() {
    let items = vec![Item::Enum(zoya_ast::EnumDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Bar".to_string(),
        type_params: vec![],
        variants: vec![],
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[test]"));
    assert!(err.to_string().contains("enum"));
}

#[test]
fn test_unknown_attr_on_fn_is_silently_discarded() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "unknown".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "foo".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_test_attr_with_params_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_bad".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have parameters"));
}

#[test]
fn test_test_attr_wrong_return_type_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_bad".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("must return () or Result"));
}

#[test]
fn test_builtin_and_test_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "builtin".to_string(),
                args: None,
            },
            Attribute {
                name: "test".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "test_bad".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have both"));
}

#[test]
fn test_private_test_fn_appears_in_definitions() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Private,
        name: "test_something".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("test_something");
    assert!(
        checked.definitions.contains_key(&path),
        "private #[test] function should appear in definitions"
    );
}

#[test]
fn test_test_fn_has_is_test_flag() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "test".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "test_something".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("test_something");
    let func = checked.items.get(&path).unwrap();
    assert_eq!(func.kind, zoya_ir::FunctionKind::Test);
}

#[test]
fn test_task_attr_on_fn_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_task_attr_on_struct_is_error() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[task]"));
    assert!(err.to_string().contains("struct"));
}

#[test]
fn test_task_attr_on_enum_is_error() {
    let items = vec![Item::Enum(EnumDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "Bar".to_string(),
        type_params: vec![],
        variants: vec![],
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[task]"));
    assert!(err.to_string().contains("enum"));
}

#[test]
fn test_builtin_and_task_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "builtin".to_string(),
                args: None,
            },
            Attribute {
                name: "task".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have both"));
    assert!(err.to_string().contains("#[builtin]"));
    assert!(err.to_string().contains("#[task]"));
}

#[test]
fn test_test_and_task_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "test".to_string(),
                args: None,
            },
            Attribute {
                name: "task".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have both"));
    assert!(err.to_string().contains("#[test]"));
    assert!(err.to_string().contains("#[task]"));
}

#[test]
fn test_private_task_fn_appears_in_definitions() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Private,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("my_task");
    assert!(
        checked.definitions.contains_key(&path),
        "private #[task] function should appear in definitions"
    );
}

#[test]
fn test_task_fn_has_is_task_flag() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let checked = check(&pkg, &[]).unwrap();
    let path = QualifiedPath::root().child("my_task");
    let func = checked.items.get(&path).unwrap();
    assert_eq!(func.kind, zoya_ir::FunctionKind::Task);
}

#[test]
fn test_task_fn_with_params_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![Param {
            pattern: Pattern::Path(Path::simple("x".to_string())),
            typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
        }],
        return_type: None,
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_task_fn_with_any_return_type_is_valid() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "task".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "my_task".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
        body: Expr::Int(42),
    })];
    let pkg = build_test_package(items);
    let result = check(&pkg, &[]);
    assert!(result.is_ok());
}

// ============================================================================
// HTTP route attribute tests
// ============================================================================

#[test]
fn test_get_attr_valid_no_params() {
    let result = check_source(
        r#"
use std::http::Response
use std::http::Body

#[get("/test")]
pub fn handler() -> Response {
  Response::ok(Option::None)
}
"#,
    );
    assert!(result.is_ok(), "expected ok, got: {:?}", result);
}

#[test]
fn test_post_attr_with_request_param() {
    let result = check_source(
        r#"
use std::http::Request
use std::http::Response
use std::http::Body

#[post("/items")]
pub fn handler(req: Request) -> Response {
  Response::ok(Option::None)
}
"#,
    );
    assert!(result.is_ok(), "expected ok, got: {:?}", result);
}

#[test]
fn test_get_attr_on_struct_is_error() {
    let items = vec![Item::Struct(StructDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "get".to_string(),
            args: Some(vec![AttributeArg::String("/test".to_string())]),
        }],
        visibility: Visibility::Public,
        name: "Foo".to_string(),
        type_params: vec![],
        kind: StructKind::Unit,
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[get]"));
    assert!(err.to_string().contains("struct"));
}

#[test]
fn test_get_attr_on_enum_is_error() {
    let items = vec![Item::Enum(EnumDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "get".to_string(),
            args: Some(vec![AttributeArg::String("/test".to_string())]),
        }],
        visibility: Visibility::Public,
        name: "Bar".to_string(),
        type_params: vec![],
        variants: vec![],
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("#[get]"));
    assert!(err.to_string().contains("enum"));
}

#[test]
fn test_get_attr_missing_path_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "get".to_string(),
            args: None,
        }],
        visibility: Visibility::Public,
        name: "handler".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("requires a path argument"));
}

#[test]
fn test_get_attr_non_string_path_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "get".to_string(),
            args: Some(vec![AttributeArg::Identifier("foo".to_string())]),
        }],
        visibility: Visibility::Public,
        name: "handler".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("requires a string path argument"));
}

#[test]
fn test_get_attr_invalid_pathname_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![Attribute {
            name: "get".to_string(),
            args: Some(vec![AttributeArg::String("no-slash".to_string())]),
        }],
        visibility: Visibility::Public,
        name: "handler".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("must start with '/'"));
}

#[test]
fn test_http_fn_too_many_params_is_error() {
    let err = check_source(
        r#"
use std::http::Request
use std::http::Response

#[get("/test")]
pub fn handler(a: Request, b: Request) -> Response {
  Response::ok(Option::None)
}
"#,
    )
    .unwrap_err();
    assert!(err.contains("at most 1 parameter"));
}

#[test]
fn test_http_fn_wrong_param_type_is_error() {
    let err = check_source(
        r#"
use std::http::Response

#[get("/test")]
pub fn handler(x: Int) -> Response {
  Response::ok(Option::None)
}
"#,
    )
    .unwrap_err();
    assert!(err.contains("must be of type Request"));
}

#[test]
fn test_http_fn_wrong_return_type_is_error() {
    let err = check_source(
        r#"
#[get("/test")]
pub fn handler() -> Int {
  42
}
"#,
    )
    .unwrap_err();
    assert!(err.contains("must return Response"));
}

#[test]
fn test_get_and_test_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "get".to_string(),
                args: Some(vec![AttributeArg::String("/test".to_string())]),
            },
            Attribute {
                name: "test".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "handler".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have both"));
}

#[test]
fn test_get_and_builtin_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "get".to_string(),
                args: Some(vec![AttributeArg::String("/test".to_string())]),
            },
            Attribute {
                name: "builtin".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "handler".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have both"));
}

#[test]
fn test_get_and_task_conflict_is_error() {
    let items = vec![Item::Function(FunctionDef {
        leading_comments: vec![],
        attributes: vec![
            Attribute {
                name: "get".to_string(),
                args: Some(vec![AttributeArg::String("/test".to_string())]),
            },
            Attribute {
                name: "task".to_string(),
                args: None,
            },
        ],
        visibility: Visibility::Public,
        name: "handler".to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None,
        body: Expr::Tuple(vec![]),
    })];
    let pkg = build_test_package(items);
    let err = check(&pkg, &[]).unwrap_err();
    assert!(err.to_string().contains("cannot have both"));
}

#[test]
fn test_http_fn_has_correct_kind() {
    let checked = check_source(
        r#"
use std::http::Response

#[get("/users")]
pub fn handler() -> Response {
  Response::ok(Option::None)
}
"#,
    )
    .unwrap();
    let path = QualifiedPath::root().child("handler");
    let func = checked.items.get(&path).unwrap();
    match &func.kind {
        zoya_ir::FunctionKind::Http(method, pathname) => {
            assert_eq!(*method, zoya_ir::HttpMethod::Get);
            assert_eq!(pathname.as_str(), "/users");
        }
        other => panic!("expected Http kind, got {:?}", other),
    }
}

#[test]
fn test_http_fn_appears_in_definitions() {
    let checked = check_source(
        r#"
use std::http::Response

#[get("/test")]
fn handler() -> Response {
  Response::ok(Option::None)
}
"#,
    )
    .unwrap();
    let path = QualifiedPath::root().child("handler");
    assert!(
        checked.definitions.contains_key(&path),
        "private HTTP function should appear in definitions"
    );
}

#[test]
fn test_routes_returns_http_functions() {
    let checked = check_source(
        r#"
use std::http::Response

#[get("/users")]
pub fn get_users() -> Response {
  Response::ok(Option::None)
}

#[post("/users")]
pub fn create_user() -> Response {
  Response::ok(Option::None)
}

pub fn regular_fn() -> Int {
  42
}
"#,
    )
    .unwrap();
    let routes = checked.routes();
    assert_eq!(routes.len(), 2);
}

#[test]
fn test_all_http_methods() {
    for (method, attr) in &[
        ("get", "Get"),
        ("post", "Post"),
        ("put", "Put"),
        ("patch", "Patch"),
        ("delete", "Delete"),
    ] {
        let source = format!(
            r#"
use std::http::Response

#[{}("/test")]
pub fn handler() -> Response {{
  Response::ok(Option::None)
}}
"#,
            method
        );
        let result = check_source(&source);
        assert!(
            result.is_ok(),
            "expected #[{}] to be valid, got: {:?}",
            method,
            result
        );
        let checked = result.unwrap();
        let path = QualifiedPath::root().child("handler");
        let func = checked.items.get(&path).unwrap();
        match &func.kind {
            zoya_ir::FunctionKind::Http(m, _) => {
                assert_eq!(
                    m.attr_name(),
                    *method,
                    "expected method {}, got {}",
                    method,
                    m.attr_name()
                );
            }
            other => panic!("expected Http kind for #{}, got {:?}", attr, other),
        }
    }
}
