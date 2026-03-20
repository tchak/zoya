use std::sync::LazyLock;

use zoya_check::check;
use zoya_ir::CheckedPackage;
use zoya_loader::{MemorySource, load_memory_package};

/// Error type for standard library building.
#[derive(Debug, thiserror::Error)]
pub enum StdError {
    #[error("failed to load std package: {0}")]
    Load(#[from] zoya_loader::LoaderError<String>),
    #[error("failed to check std package: {0}")]
    Check(#[from] zoya_ir::TypeError),
}

static STD_PACKAGE: LazyLock<CheckedPackage> =
    LazyLock::new(|| build_std().expect("failed to build std package"));

fn build_std() -> Result<CheckedPackage, StdError> {
    let source = MemorySource::new()
        .with_module("root", include_str!("std/main.zy"))
        .with_module("bigint", include_str!("std/bigint.zy"))
        .with_module("bytes", include_str!("std/bytes.zy"))
        .with_module("dict", include_str!("std/dict.zy"))
        .with_module("float", include_str!("std/float.zy"))
        .with_module("http", include_str!("std/http.zy"))
        .with_module("int", include_str!("std/int.zy"))
        .with_module("io", include_str!("std/io.zy"))
        .with_module("json", include_str!("std/json.zy"))
        .with_module("list", include_str!("std/list.zy"))
        .with_module("option", include_str!("std/option.zy"))
        .with_module("prelude", include_str!("std/prelude.zy"))
        .with_module("result", include_str!("std/result.zy"))
        .with_module("set", include_str!("std/set.zy"))
        .with_module("string", include_str!("std/string.zy"))
        .with_module("task", include_str!("std/task.zy"));

    let mut pkg = load_memory_package(&source, zoya_loader::Mode::Release)?;
    pkg.name = "std".to_string();

    Ok(check(&pkg, &[])?)
}

/// Returns the standard library as a checked package.
pub fn std() -> &'static CheckedPackage {
    &STD_PACKAGE
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ir::Definition;
    use zoya_loader::QualifiedPath;

    #[test]
    fn test_std_has_definitions() {
        let pkg = std();
        assert!(
            !pkg.definitions.is_empty(),
            "std package should have definitions"
        );
    }

    #[test]
    fn test_std_has_option_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("option").child("Option");
        let def = pkg.definitions.get(&path).expect("Option definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_has_result_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("result").child("Result");
        let def = pkg.definitions.get(&path).expect("Result definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_reexports_option_variants() {
        let pkg = std();
        let option_path = QualifiedPath::root().child("option");
        let some_path = option_path.child("Some");
        let none_path = option_path.child("None");
        assert!(
            pkg.definitions.contains_key(&some_path),
            "Some should be re-exported in option module"
        );
        assert!(
            pkg.definitions.contains_key(&none_path),
            "None should be re-exported in option module"
        );
    }

    #[test]
    fn test_std_reexports_result_variants() {
        let pkg = std();
        let result_path = QualifiedPath::root().child("result");
        let ok_path = result_path.child("Ok");
        let err_path = result_path.child("Err");
        assert!(
            pkg.definitions.contains_key(&ok_path),
            "Ok should be re-exported in result module"
        );
        assert!(
            pkg.definitions.contains_key(&err_path),
            "Err should be re-exported in result module"
        );
    }

    #[test]
    fn test_std_has_prelude_definitions() {
        let pkg = std();
        let prelude = QualifiedPath::root().child("prelude");
        // Prelude should re-export Option, Some, None, Result, Ok, Err
        for name in &["Option", "Some", "None", "Result", "Ok", "Err"] {
            let path = prelude.child(name);
            assert!(
                pkg.definitions.contains_key(&path),
                "{} should be re-exported in prelude module",
                name
            );
        }
    }

    #[test]
    fn test_std_has_io_module() {
        let pkg = std();
        let io_path = QualifiedPath::root().child("io");
        assert!(
            pkg.definitions.contains_key(&io_path),
            "io module should exist"
        );
    }

    #[test]
    fn test_std_has_println_function() {
        let pkg = std();
        let path = QualifiedPath::root().child("io").child("println");
        let def = pkg.definitions.get(&path).expect("println definition");
        assert!(matches!(def, Definition::Function(_)));
    }

    #[test]
    fn test_std_has_println_in_items() {
        let pkg = std();
        let path = QualifiedPath::root().child("io").child("println");
        let func = pkg
            .items
            .get(&path)
            .and_then(|v| v.first())
            .expect("println in items");
        assert_eq!(func.kind, zoya_ir::FunctionKind::Builtin);
    }

    #[test]
    fn test_std_prelude_has_println() {
        let pkg = std();
        let path = QualifiedPath::root().child("prelude").child("println");
        assert!(
            pkg.definitions.contains_key(&path),
            "println should be re-exported in prelude module"
        );
    }

    #[test]
    fn test_std_has_json_module() {
        let pkg = std();
        let json_path = QualifiedPath::root().child("json");
        assert!(
            pkg.definitions.contains_key(&json_path),
            "json module should exist"
        );
    }

    #[test]
    fn test_std_has_number_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("json").child("Number");
        let def = pkg.definitions.get(&path).expect("Number definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_has_json_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("json").child("JSON");
        let def = pkg.definitions.get(&path).expect("JSON definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_json_variants() {
        let pkg = std();
        let json_path = QualifiedPath::root().child("json").child("JSON");
        for name in &["Null", "Bool", "Number", "String", "Array", "Object"] {
            let path = json_path.child(name);
            assert!(
                pkg.definitions.contains_key(&path),
                "{} variant should exist in JSON enum",
                name
            );
        }
    }

    #[test]
    fn test_std_number_variants() {
        let pkg = std();
        let number_path = QualifiedPath::root().child("json").child("Number");
        for name in &["Int", "Float"] {
            let path = number_path.child(name);
            assert!(
                pkg.definitions.contains_key(&path),
                "{} variant should exist in Number enum",
                name
            );
        }
    }

    #[test]
    fn test_std_has_json_parse_error_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("json").child("ParseError");
        let def = pkg.definitions.get(&path).expect("ParseError definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_has_json_parse_function() {
        let pkg = std();
        let path = QualifiedPath::root().child("json").child("parse");
        let def = pkg.definitions.get(&path).expect("parse definition");
        assert!(matches!(def, Definition::Function(_)));
    }

    #[test]
    fn test_std_has_json_parse_in_items() {
        let pkg = std();
        let path = QualifiedPath::root().child("json").child("parse");
        let func = pkg
            .items
            .get(&path)
            .and_then(|v| v.first())
            .expect("parse in items");
        assert_eq!(func.kind, zoya_ir::FunctionKind::Builtin);
    }

    #[test]
    fn test_std_has_panic_function() {
        let pkg = std();
        let path = QualifiedPath::root().child("panic");
        let def = pkg.definitions.get(&path).expect("panic definition");
        assert!(matches!(def, Definition::Function(_)));
    }

    #[test]
    fn test_std_has_panic_in_items() {
        let pkg = std();
        let path = QualifiedPath::root().child("panic");
        let func = pkg
            .items
            .get(&path)
            .and_then(|v| v.first())
            .expect("panic in items");
        assert_eq!(func.kind, zoya_ir::FunctionKind::Builtin);
    }

    #[test]
    fn test_std_prelude_has_panic() {
        let pkg = std();
        let path = QualifiedPath::root().child("prelude").child("panic");
        assert!(
            pkg.definitions.contains_key(&path),
            "panic should be re-exported in prelude module"
        );
    }

    #[test]
    fn test_std_has_assert_function() {
        let pkg = std();
        let path = QualifiedPath::root().child("assert");
        let def = pkg.definitions.get(&path).expect("assert definition");
        assert!(matches!(def, Definition::Function(_)));
    }

    #[test]
    fn test_std_has_assert_in_items() {
        let pkg = std();
        let path = QualifiedPath::root().child("assert");
        let func = pkg
            .items
            .get(&path)
            .and_then(|v| v.first())
            .expect("assert in items");
        assert_eq!(func.kind, zoya_ir::FunctionKind::Builtin);
    }

    #[test]
    fn test_std_prelude_has_assert() {
        let pkg = std();
        let path = QualifiedPath::root().child("prelude").child("assert");
        assert!(
            pkg.definitions.contains_key(&path),
            "assert should be re-exported in prelude module"
        );
    }

    #[test]
    fn test_std_has_assert_eq_function() {
        let pkg = std();
        let path = QualifiedPath::root().child("assert_eq");
        let def = pkg.definitions.get(&path).expect("assert_eq definition");
        assert!(matches!(def, Definition::Function(_)));
    }

    #[test]
    fn test_std_has_assert_eq_in_items() {
        let pkg = std();
        let path = QualifiedPath::root().child("assert_eq");
        let func = pkg
            .items
            .get(&path)
            .and_then(|v| v.first())
            .expect("assert_eq in items");
        assert_eq!(func.kind, zoya_ir::FunctionKind::Builtin);
    }

    #[test]
    fn test_std_prelude_has_assert_eq() {
        let pkg = std();
        let path = QualifiedPath::root().child("prelude").child("assert_eq");
        assert!(
            pkg.definitions.contains_key(&path),
            "assert_eq should be re-exported in prelude module"
        );
    }

    #[test]
    fn test_std_has_assert_ne_function() {
        let pkg = std();
        let path = QualifiedPath::root().child("assert_ne");
        let def = pkg.definitions.get(&path).expect("assert_ne definition");
        assert!(matches!(def, Definition::Function(_)));
    }

    #[test]
    fn test_std_has_assert_ne_in_items() {
        let pkg = std();
        let path = QualifiedPath::root().child("assert_ne");
        let func = pkg
            .items
            .get(&path)
            .and_then(|v| v.first())
            .expect("assert_ne in items");
        assert_eq!(func.kind, zoya_ir::FunctionKind::Builtin);
    }

    #[test]
    fn test_std_prelude_has_assert_ne() {
        let pkg = std();
        let path = QualifiedPath::root().child("prelude").child("assert_ne");
        assert!(
            pkg.definitions.contains_key(&path),
            "assert_ne should be re-exported in prelude module"
        );
    }

    #[test]
    fn test_std_has_option_map_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("option")
            .child("Option")
            .child("map");
        let def = pkg.definitions.get(&path).expect("Option::map definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_option_and_then_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("option")
            .child("Option")
            .child("and_then");
        let def = pkg
            .definitions
            .get(&path)
            .expect("Option::and_then definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_result_map_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("result")
            .child("Result")
            .child("map");
        let def = pkg.definitions.get(&path).expect("Result::map definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_result_and_then_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("result")
            .child("Result")
            .child("and_then");
        let def = pkg
            .definitions
            .get(&path)
            .expect("Result::and_then definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_int_abs_method() {
        let pkg = std();
        let path = QualifiedPath::root().child("int").child("Int").child("abs");
        let def = pkg.definitions.get(&path).expect("Int::abs definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_float_sqrt_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("float")
            .child("Float")
            .child("sqrt");
        let def = pkg.definitions.get(&path).expect("Float::sqrt definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_string_len_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("string")
            .child("String")
            .child("len");
        let def = pkg.definitions.get(&path).expect("String::len definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_string_is_empty_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("string")
            .child("String")
            .child("is_empty");
        let def = pkg
            .definitions
            .get(&path)
            .expect("String::is_empty definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_list_len_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("list")
            .child("List")
            .child("len");
        let def = pkg.definitions.get(&path).expect("List::len definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_list_push_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("list")
            .child("List")
            .child("push");
        let def = pkg.definitions.get(&path).expect("List::push definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_dict_module() {
        let pkg = std();
        let dict_path = QualifiedPath::root().child("dict");
        assert!(
            pkg.definitions.contains_key(&dict_path),
            "dict module should exist"
        );
    }

    #[test]
    fn test_std_has_dict_new_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("dict")
            .child("Dict")
            .child("new");
        let def = pkg.definitions.get(&path).expect("Dict::new definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_dict_get_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("dict")
            .child("Dict")
            .child("get");
        let def = pkg.definitions.get(&path).expect("Dict::get definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_dict_insert_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("dict")
            .child("Dict")
            .child("insert");
        let def = pkg.definitions.get(&path).expect("Dict::insert definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_dict_len_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("dict")
            .child("Dict")
            .child("len");
        let def = pkg.definitions.get(&path).expect("Dict::len definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_dict_is_empty_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("dict")
            .child("Dict")
            .child("is_empty");
        let def = pkg
            .definitions
            .get(&path)
            .expect("Dict::is_empty definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_set_module() {
        let pkg = std();
        let set_path = QualifiedPath::root().child("set");
        assert!(
            pkg.definitions.contains_key(&set_path),
            "set module should exist"
        );
    }

    #[test]
    fn test_std_has_set_new_method() {
        let pkg = std();
        let path = QualifiedPath::root().child("set").child("Set").child("new");
        let def = pkg.definitions.get(&path).expect("Set::new definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_set_contains_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("set")
            .child("Set")
            .child("contains");
        let def = pkg
            .definitions
            .get(&path)
            .expect("Set::contains definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_set_insert_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("set")
            .child("Set")
            .child("insert");
        let def = pkg.definitions.get(&path).expect("Set::insert definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_set_len_method() {
        let pkg = std();
        let path = QualifiedPath::root().child("set").child("Set").child("len");
        let def = pkg.definitions.get(&path).expect("Set::len definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_set_is_empty_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("set")
            .child("Set")
            .child("is_empty");
        let def = pkg
            .definitions
            .get(&path)
            .expect("Set::is_empty definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_http_module() {
        let pkg = std();
        let http_path = QualifiedPath::root().child("http");
        assert!(
            pkg.definitions.contains_key(&http_path),
            "http module should exist"
        );
    }

    #[test]
    fn test_std_has_method_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("http").child("Method");
        let def = pkg.definitions.get(&path).expect("Method definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_has_method_variants() {
        let pkg = std();
        let method_path = QualifiedPath::root().child("http").child("Method");
        for name in &["Get", "Post", "Put", "Patch", "Delete", "Head", "Options"] {
            let path = method_path.child(name);
            assert!(
                pkg.definitions.contains_key(&path),
                "{} variant should exist in Method enum",
                name
            );
        }
    }

    #[test]
    fn test_std_has_body_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("http").child("Body");
        let def = pkg.definitions.get(&path).expect("Body definition");
        assert!(matches!(def, Definition::Enum(_)));
    }

    #[test]
    fn test_std_has_body_variants() {
        let pkg = std();
        let body_path = QualifiedPath::root().child("http").child("Body");
        for name in &["Text", "Json"] {
            let path = body_path.child(name);
            assert!(
                pkg.definitions.contains_key(&path),
                "{} variant should exist in Body enum",
                name
            );
        }
    }

    #[test]
    fn test_std_has_request_struct() {
        let pkg = std();
        let path = QualifiedPath::root().child("http").child("Request");
        let def = pkg.definitions.get(&path).expect("Request definition");
        assert!(matches!(def, Definition::Struct(_)));
    }

    #[test]
    fn test_std_has_response_struct() {
        let pkg = std();
        let path = QualifiedPath::root().child("http").child("Response");
        let def = pkg.definitions.get(&path).expect("Response definition");
        assert!(matches!(def, Definition::Struct(_)));
    }

    #[test]
    fn test_std_has_response_ok_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("http")
            .child("Response")
            .child("ok");
        let def = pkg.definitions.get(&path).expect("Response::ok definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_has_headers_type_alias() {
        let pkg = std();
        let path = QualifiedPath::root().child("http").child("Headers");
        let def = pkg.definitions.get(&path).expect("Headers definition");
        assert!(matches!(def, Definition::TypeAlias(_)));
    }

    #[test]
    fn test_std_has_bigint_abs_method() {
        let pkg = std();
        let path = QualifiedPath::root()
            .child("bigint")
            .child("BigInt")
            .child("abs");
        let def = pkg.definitions.get(&path).expect("BigInt::abs definition");
        assert!(matches!(def, Definition::ImplMethod(_)));
    }

    #[test]
    fn test_std_is_cached() {
        let a = std();
        let b = std();
        assert!(std::ptr::eq(a, b));
    }
}
