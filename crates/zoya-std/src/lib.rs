use std::sync::LazyLock;

use zoya_check::check;
use zoya_ir::CheckedPackage;
use zoya_loader::{MemorySource, load_memory_package};

static STD_PACKAGE: LazyLock<CheckedPackage> =
    LazyLock::new(|| build_std().expect("failed to build std package"));

fn build_std() -> Result<CheckedPackage, String> {
    let source = MemorySource::new()
        .with_module("root", include_str!("std/main.zy"))
        .with_module("io", include_str!("std/io.zy"))
        .with_module("json", include_str!("std/json.zy"))
        .with_module("option", include_str!("std/option.zy"))
        .with_module("prelude", include_str!("std/prelude.zy"))
        .with_module("result", include_str!("std/result.zy"));

    let mut pkg =
        load_memory_package(&source).map_err(|e| format!("failed to load std package: {e}"))?;
    pkg.name = "std".to_string();

    check(&pkg, &[]).map_err(|e| format!("failed to check std package: {e}"))
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
        let func = pkg.items.get(&path).expect("println in items");
        assert!(func.is_builtin);
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
        let func = pkg.items.get(&path).expect("parse in items");
        assert!(func.is_builtin);
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
        let func = pkg.items.get(&path).expect("panic in items");
        assert!(func.is_builtin);
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
        let func = pkg.items.get(&path).expect("assert in items");
        assert!(func.is_builtin);
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
        let func = pkg.items.get(&path).expect("assert_eq in items");
        assert!(func.is_builtin);
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
        let func = pkg.items.get(&path).expect("assert_ne in items");
        assert!(func.is_builtin);
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
    fn test_std_is_cached() {
        let a = std();
        let b = std();
        assert!(std::ptr::eq(a, b));
    }
}
