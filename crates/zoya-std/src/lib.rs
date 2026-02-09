use std::sync::LazyLock;

use zoya_check::check;
use zoya_ir::CheckedPackage;
use zoya_loader::{load_package_with, MemorySource};

static STD_PACKAGE: LazyLock<CheckedPackage> = LazyLock::new(|| {
    build_std().expect("failed to build std package")
});

fn build_std() -> Result<CheckedPackage, String> {
    let source = MemorySource::new()
        .with_module("root", include_str!("std/main.zoya"))
        .with_module("option", include_str!("std/option.zoya"))
        .with_module("result", include_str!("std/result.zoya"));

    let pkg = load_package_with(&source, &"root".to_string())
        .map_err(|e| format!("failed to load std package: {e}"))?;

    check(&pkg).map_err(|e| format!("failed to check std package: {e}"))
}

/// Returns the standard library as a checked package.
pub fn std() -> &'static CheckedPackage {
    &STD_PACKAGE
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ir::CheckedItem;
    use zoya_loader::QualifiedPath;

    #[test]
    fn test_std_has_three_modules() {
        let pkg = std();
        assert_eq!(pkg.modules.len(), 3);
    }

    #[test]
    fn test_std_has_option_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("option");
        let module = pkg.get(&path).expect("option module");
        assert_eq!(module.items.len(), 1);
        assert!(matches!(&module.items[0], CheckedItem::Enum(_)));
    }

    #[test]
    fn test_std_has_result_enum() {
        let pkg = std();
        let path = QualifiedPath::root().child("result");
        let module = pkg.get(&path).expect("result module");
        assert_eq!(module.items.len(), 1);
        assert!(matches!(&module.items[0], CheckedItem::Enum(_)));
    }

    #[test]
    fn test_std_is_cached() {
        let a = std();
        let b = std();
        assert!(std::ptr::eq(a, b));
    }
}
