use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::path::Path;

// Re-export module types from zoya-package
pub use zoya_package::{Module, ModulePath, Package};

mod source;
mod sources;

pub use source::{ModuleSource, SourceError, SourceErrorKind};
pub use sources::{FilePath, FsSource, MemorySource};

#[derive(Debug, Clone, PartialEq)]
pub enum LoaderError<P: Clone + Debug + Display = FilePath> {
    ModuleNotFound {
        mod_name: String,
        expected_path: P,
    },
    DuplicateMod {
        mod_name: String,
    },
    SourceError {
        path: P,
        error: SourceError,
    },
    LexError {
        path: P,
        message: String,
    },
    ParseError {
        path: P,
        message: String,
    },
}

impl<P: Clone + Debug + Display> std::fmt::Display for LoaderError<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::ModuleNotFound {
                mod_name,
                expected_path,
            } => {
                write!(f, "module '{}' not found at '{}'", mod_name, expected_path)
            }
            LoaderError::DuplicateMod { mod_name } => {
                write!(f, "duplicate module declaration: '{}'", mod_name)
            }
            LoaderError::SourceError { path, error } => {
                write!(f, "failed to read '{}': {}", path, error)
            }
            LoaderError::LexError { path, message } => {
                write!(f, "lexer error in '{}': {}", path, message)
            }
            LoaderError::ParseError { path, message } => {
                write!(f, "parse error in '{}': {}", path, message)
            }
        }
    }
}

impl<P: Clone + Debug + Display> std::error::Error for LoaderError<P> {}

/// Load package starting from root file (convenience wrapper for filesystem)
pub fn load_package(path: &Path) -> Result<Package, LoaderError<FilePath>> {
    let source = FsSource::from_file(path);
    load_package_with(&source, &FilePath::new(path))
}

/// Load package using a generic module source
pub fn load_package_with<S: ModuleSource>(
    source: &S,
    root_path: &S::Path,
) -> Result<Package, LoaderError<S::Path>> {
    let mut pkg = Package {
        modules: HashMap::new(),
    };
    load_module_recursive(source, root_path, ModulePath::root(), &mut pkg)?;
    Ok(pkg)
}

fn load_module_recursive<S: ModuleSource>(
    source: &S,
    file_path: &S::Path,
    module_path: ModulePath,
    pkg: &mut Package,
) -> Result<(), LoaderError<S::Path>> {
    // Read file
    let content = source.read(file_path).map_err(|e| LoaderError::SourceError {
        path: file_path.clone(),
        error: e,
    })?;

    // Lex
    let tokens = zoya_lexer::lex(&content).map_err(|e| LoaderError::LexError {
        path: file_path.clone(),
        message: e.message,
    })?;

    // Parse
    let module_def = zoya_parser::parse_module(tokens).map_err(|e| LoaderError::ParseError {
        path: file_path.clone(),
        message: e.message,
    })?;

    // Check for duplicate mods
    let mut seen_mods = HashSet::new();
    for mod_decl in &module_def.mods {
        if !seen_mods.insert(&mod_decl.name) {
            return Err(LoaderError::DuplicateMod {
                mod_name: mod_decl.name.clone(),
            });
        }
    }

    // Build children map and collect submodules to load
    let mut children = HashMap::new();
    let mut submodules = Vec::new();

    for mod_decl in &module_def.mods {
        let child_path = module_path.child(&mod_decl.name);
        children.insert(mod_decl.name.clone(), child_path.clone());

        let submodule_file = source.resolve_submodule(&module_path, &mod_decl.name);
        if !source.exists(&submodule_file) {
            return Err(LoaderError::ModuleNotFound {
                mod_name: mod_decl.name.clone(),
                expected_path: submodule_file,
            });
        }
        submodules.push((submodule_file, child_path));
    }

    // Store module
    pkg.modules.insert(
        module_path.clone(),
        Module {
            items: module_def.items,
            path: module_path,
            children,
        },
    );

    // Recursively load submodules
    for (submodule_file, child_path) in submodules {
        load_module_recursive(source, &submodule_file, child_path, pkg)?;
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    // === FsSource tests ===

    #[test]
    fn test_fs_source_resolve_submodule_root() {
        let source = FsSource::new("/project");
        let root_path = ModulePath::root();
        let submodule = source.resolve_submodule(&root_path, "foo");
        assert_eq!(submodule, FilePath::new("/project/foo.zoya"));
    }

    #[test]
    fn test_fs_source_resolve_submodule_nested() {
        let source = FsSource::new("/project");
        let utils_path = ModulePath::root().child("utils");
        let submodule = source.resolve_submodule(&utils_path, "bar");
        assert_eq!(submodule, FilePath::new("/project/utils/bar.zoya"));
    }

    #[test]
    fn test_fs_source_resolve_submodule_deeply_nested() {
        let source = FsSource::new("/project");
        let helpers_path = ModulePath::root().child("utils").child("helpers");
        let submodule = source.resolve_submodule(&helpers_path, "baz");
        assert_eq!(submodule, FilePath::new("/project/utils/helpers/baz.zoya"));
    }

    // === MemorySource tests ===

    #[test]
    fn test_memory_source_read() {
        let source = MemorySource::new()
            .with_module("root", "fn main() -> Int { 42 }");

        assert!(source.exists(&"root".to_string()));
        assert!(!source.exists(&"missing".to_string()));

        let content = source.read(&"root".to_string()).unwrap();
        assert_eq!(content, "fn main() -> Int { 42 }");

        let err = source.read(&"missing".to_string()).unwrap_err();
        assert_eq!(err.kind, SourceErrorKind::NotFound);
    }

    #[test]
    fn test_memory_source_resolve_submodule() {
        let source = MemorySource::new();

        // Root level
        let root_path = ModulePath::root();
        assert_eq!(source.resolve_submodule(&root_path, "utils"), "utils");

        // Nested (utils module looking for helpers)
        let utils_path = ModulePath::root().child("utils");
        assert_eq!(source.resolve_submodule(&utils_path, "helpers"), "utils/helpers");

        // Deeply nested
        let helpers_path = ModulePath::root().child("utils").child("helpers");
        assert_eq!(source.resolve_submodule(&helpers_path, "deep"), "utils/helpers/deep");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    // === Basic loading tests (filesystem) ===

    #[test]
    fn test_load_single_file_no_mods() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "fn foo() -> Int 42");

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        assert_eq!(tree.modules.len(), 1);
        let root = tree.root().unwrap();
        assert!(root.path.is_root());
        assert!(root.children.is_empty());
        assert_eq!(root.items.len(), 1);
    }

    #[test]
    fn test_load_empty_file() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "");

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        let root = tree.root().unwrap();
        assert!(root.items.is_empty());
        assert!(root.children.is_empty());
    }

    #[test]
    fn test_load_with_one_submodule() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils");
        create_file(
            dir.path(),
            "utils.zoya",
            "fn add(x: Int, y: Int) -> Int x + y",
        );

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        assert_eq!(tree.modules.len(), 2);

        // Check root
        let root = tree.root().unwrap();
        assert!(root.items.is_empty());
        assert_eq!(root.children.len(), 1);
        assert!(root.children.contains_key("utils"));

        // Check utils module
        let utils_path = ModulePath(vec!["root".to_string(), "utils".to_string()]);
        let utils = tree.get(&utils_path).unwrap();
        assert_eq!(utils.items.len(), 1);
        assert!(utils.children.is_empty());
    }

    #[test]
    fn test_load_with_multiple_submodules() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils mod helpers mod types");
        create_file(dir.path(), "utils.zoya", "fn util_fn() -> Int 1");
        create_file(dir.path(), "helpers.zoya", "fn helper_fn() -> Int 2");
        create_file(
            dir.path(),
            "types.zoya",
            "struct Point { x: Int, y: Int }",
        );

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        assert_eq!(tree.modules.len(), 4);

        let root = tree.root().unwrap();
        assert_eq!(root.children.len(), 3);
    }

    #[test]
    fn test_load_nested_modules() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils");
        create_file(dir.path(), "utils.zoya", "mod helpers");
        create_file(dir.path(), "utils/helpers.zoya", "fn deep_fn() -> Int 42");

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        assert_eq!(tree.modules.len(), 3);

        // Check utils has helpers as child
        let utils_path = ModulePath(vec!["root".to_string(), "utils".to_string()]);
        let utils = tree.get(&utils_path).unwrap();
        assert!(utils.children.contains_key("helpers"));

        // Check helpers module exists
        let helpers_path = ModulePath(vec![
            "root".to_string(),
            "utils".to_string(),
            "helpers".to_string(),
        ]);
        let helpers = tree.get(&helpers_path).unwrap();
        assert_eq!(helpers.items.len(), 1);
    }

    #[test]
    fn test_load_deeply_nested() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod a");
        create_file(dir.path(), "a.zoya", "mod b");
        create_file(dir.path(), "a/b.zoya", "mod c");
        create_file(dir.path(), "a/b/c.zoya", "fn deep() -> Int 1");

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        assert_eq!(tree.modules.len(), 4);

        let c_path = ModulePath(vec![
            "root".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
        ]);
        let c_module = tree.get(&c_path).unwrap();
        assert_eq!(c_module.items.len(), 1);
    }

    // === Error case tests ===

    #[test]
    fn test_error_module_not_found() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod missing");

        let result = load_package(&dir.path().join("main.zoya"));

        assert!(
            matches!(result, Err(LoaderError::ModuleNotFound { mod_name, .. }) if mod_name == "missing")
        );
    }

    #[test]
    fn test_error_duplicate_mod() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils mod utils");
        create_file(dir.path(), "utils.zoya", "");

        let result = load_package(&dir.path().join("main.zoya"));

        assert!(
            matches!(result, Err(LoaderError::DuplicateMod { mod_name }) if mod_name == "utils")
        );
    }

    #[test]
    fn test_error_io_file_not_found() {
        let dir = TempDir::new().unwrap();
        let result = load_package(&dir.path().join("nonexistent.zoya"));

        assert!(matches!(result, Err(LoaderError::SourceError { .. })));
    }

    #[test]
    fn test_error_lex_error() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "fn foo() # invalid");

        let result = load_package(&dir.path().join("main.zoya"));

        assert!(matches!(result, Err(LoaderError::LexError { .. })));
    }

    #[test]
    fn test_error_parse_error() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "fn fn fn");

        let result = load_package(&dir.path().join("main.zoya"));

        assert!(matches!(result, Err(LoaderError::ParseError { .. })));
    }

    // === Package API tests ===

    #[test]
    fn test_package_get() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils");
        create_file(dir.path(), "utils.zoya", "");

        let tree = load_package(&dir.path().join("main.zoya")).unwrap();

        assert!(tree.get(&ModulePath::root()).is_some());
        assert!(tree
            .get(&ModulePath(vec!["root".to_string(), "utils".to_string()]))
            .is_some());
        assert!(tree
            .get(&ModulePath(vec![
                "root".to_string(),
                "nonexistent".to_string()
            ]))
            .is_none());
    }

    // === MemorySource integration tests ===

    #[test]
    fn test_memory_source_load_single_module() {
        let source = MemorySource::new()
            .with_module("root", "fn foo() -> Int 42");

        let tree = load_package_with(&source, &"root".to_string()).unwrap();

        assert_eq!(tree.modules.len(), 1);
        let root = tree.root().unwrap();
        assert!(root.path.is_root());
        assert!(root.children.is_empty());
        assert_eq!(root.items.len(), 1);
    }

    #[test]
    fn test_memory_source_load_with_submodule() {
        let source = MemorySource::new()
            .with_module("root", "mod utils\nfn main() -> Int 42")
            .with_module("utils", "fn helper() -> Int 10");

        let tree = load_package_with(&source, &"root".to_string()).unwrap();

        assert_eq!(tree.modules.len(), 2);

        let root = tree.root().unwrap();
        assert_eq!(root.children.len(), 1);
        assert!(root.children.contains_key("utils"));
        assert_eq!(root.items.len(), 1);

        let utils_path = ModulePath(vec!["root".to_string(), "utils".to_string()]);
        let utils = tree.get(&utils_path).unwrap();
        assert_eq!(utils.items.len(), 1);
    }

    #[test]
    fn test_memory_source_load_nested_modules() {
        let source = MemorySource::new()
            .with_module("root", "mod utils")
            .with_module("utils", "mod helpers")
            .with_module("utils/helpers", "fn deep_fn() -> Int 42");

        let tree = load_package_with(&source, &"root".to_string()).unwrap();

        assert_eq!(tree.modules.len(), 3);

        let helpers_path = ModulePath(vec![
            "root".to_string(),
            "utils".to_string(),
            "helpers".to_string(),
        ]);
        let helpers = tree.get(&helpers_path).unwrap();
        assert_eq!(helpers.items.len(), 1);
    }

    #[test]
    fn test_memory_source_error_module_not_found() {
        let source = MemorySource::new()
            .with_module("root", "mod missing");

        let result = load_package_with(&source, &"root".to_string());

        assert!(
            matches!(result, Err(LoaderError::ModuleNotFound { mod_name, .. }) if mod_name == "missing")
        );
    }

    #[test]
    fn test_memory_source_error_source_not_found() {
        let source = MemorySource::new();

        let result = load_package_with(&source, &"nonexistent".to_string());

        assert!(matches!(result, Err(LoaderError::SourceError { .. })));
    }
}
