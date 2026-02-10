use std::sync::LazyLock;

use zoya_check::check;
use zoya_ir::CheckedPackage;
use zoya_loader::{load_memory_package, MemorySource};

static STD_PACKAGE: LazyLock<CheckedPackage> = LazyLock::new(|| {
    build_std().expect("failed to build std package")
});

fn build_std() -> Result<CheckedPackage, String> {
    let source = MemorySource::new()
        .with_module("root", include_str!("std/main.zoya"))
        .with_module("option", include_str!("std/option.zoya"))
        .with_module("result", include_str!("std/result.zoya"));

    let mut pkg = load_memory_package(&source)
        .map_err(|e| format!("failed to load std package: {e}"))?;
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
        assert!(!pkg.definitions.is_empty(), "std package should have definitions");
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
    fn test_std_is_cached() {
        let a = std();
        let b = std();
        assert!(std::ptr::eq(a, b));
    }
}
