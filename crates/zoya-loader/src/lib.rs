use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};

// Re-export module types from zoya-package
pub use zoya_package::{Module, QualifiedPath, Package};
pub use zoya_package::{ConfigError, PackageConfig};

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
    InvalidModName {
        mod_name: String,
        suggestion: String,
    },
    ReservedModName {
        mod_name: String,
    },
    NoPackageToml {
        dir: PathBuf,
    },
    MainNotFound {
        main: PathBuf,
        package_dir: PathBuf,
    },
    ConfigError(String),
    MissingRoot,
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
            LoaderError::InvalidModName {
                mod_name,
                suggestion,
            } => {
                write!(
                    f,
                    "invalid module name '{}': module names must be snake_case (try '{}')",
                    mod_name, suggestion
                )
            }
            LoaderError::ReservedModName { mod_name } => {
                write!(
                    f,
                    "invalid module name '{}': this name is reserved (root, self, super, std, zoya)",
                    mod_name
                )
            }
            LoaderError::NoPackageToml { dir } => {
                if dir == Path::new(".") {
                    write!(
                        f,
                        "no package.toml found in current directory\nhint: provide a file path or create a package with `zoya new`"
                    )
                } else {
                    write!(
                        f,
                        "no package.toml found in '{}'\nhint: provide a .zoya file path or create a package with `zoya new {}`",
                        dir.display(),
                        dir.display()
                    )
                }
            }
            LoaderError::MainNotFound { main, package_dir } => {
                write!(
                    f,
                    "main file '{}' not found in package at '{}'",
                    main.display(),
                    package_dir.display()
                )
            }
            LoaderError::ConfigError(msg) => write!(f, "{}", msg),
            LoaderError::MissingRoot => write!(f, "missing root module"),
        }
    }
}

impl<P: Clone + Debug + Display> std::error::Error for LoaderError<P> {}

/// Load a package from a filesystem path.
///
/// Handles three cases:
/// - **Directory**: looks for `package.toml`, loads config, resolves main file
/// - **`package.toml` file**: loads config from it
/// - **`.zoya` file**: treats as standalone file, derives name from filename
pub fn load_package(path: &Path) -> Result<Package, LoaderError<FilePath>> {
    if path.is_dir() {
        return load_from_directory(path);
    }

    if path.file_name().is_some_and(|f| f == "package.toml") {
        let dir = path.parent().unwrap_or(Path::new("."));
        return load_from_directory(dir);
    }

    // Standalone .zoya file
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| zoya_naming::package_name_to_module_name(&zoya_naming::sanitize_package_name(s)))
        .unwrap_or_else(|| "root".to_string());

    let source = FsSource::from_file(path);
    let mut pkg = Package {
        name,
        output: None,
        modules: HashMap::new(),
    };
    load_module_recursive(&source, &FilePath::new(path), QualifiedPath::root(), &mut pkg)?;
    Ok(pkg)
}

fn load_from_directory(dir: &Path) -> Result<Package, LoaderError<FilePath>> {
    let config_path = dir.join("package.toml");
    if !config_path.exists() {
        return Err(LoaderError::NoPackageToml {
            dir: dir.to_path_buf(),
        });
    }

    let config =
        PackageConfig::load(dir).map_err(|e| LoaderError::ConfigError(e.to_string()))?;
    let name = config.module_name();
    let main = config.main_path();
    let output = config.output.map(|o| dir.join(o));
    let main_path = dir.join(&main);

    if !main_path.exists() {
        return Err(LoaderError::MainNotFound {
            main,
            package_dir: dir.to_path_buf(),
        });
    }

    let source = FsSource::from_file(&main_path);
    let mut pkg = Package {
        name,
        output,
        modules: HashMap::new(),
    };
    load_module_recursive(&source, &FilePath::new(&main_path), QualifiedPath::root(), &mut pkg)?;
    Ok(pkg)
}

/// Load a package from an in-memory source.
///
/// Expects a "root" module to exist in the source.
pub fn load_memory_package(source: &MemorySource) -> Result<Package, LoaderError<String>> {
    if !source.exists(&"root".to_string()) {
        return Err(LoaderError::MissingRoot);
    }
    let mut pkg = Package {
        name: "root".to_string(),
        output: None,
        modules: HashMap::new(),
    };
    load_module_recursive(source, &"root".to_string(), QualifiedPath::root(), &mut pkg)?;
    Ok(pkg)
}

fn load_module_recursive<S: ModuleSource>(
    source: &S,
    file_path: &S::Path,
    module_path: QualifiedPath,
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

    // Validate module names
    for mod_decl in &module_def.mods {
        if zoya_naming::is_reserved_name(&mod_decl.name) {
            return Err(LoaderError::ReservedModName {
                mod_name: mod_decl.name.clone(),
            });
        }
        if !zoya_naming::is_valid_module_name(&mod_decl.name) {
            return Err(LoaderError::InvalidModName {
                mod_name: mod_decl.name.clone(),
                suggestion: zoya_naming::to_snake_case(&mod_decl.name),
            });
        }
    }

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
        children.insert(mod_decl.name.clone(), (child_path.clone(), mod_decl.visibility));

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
        let root_path = QualifiedPath::root();
        let submodule = source.resolve_submodule(&root_path, "foo");
        assert_eq!(submodule, FilePath::new("/project/foo.zoya"));
    }

    #[test]
    fn test_fs_source_resolve_submodule_nested() {
        let source = FsSource::new("/project");
        let utils_path = QualifiedPath::root().child("utils");
        let submodule = source.resolve_submodule(&utils_path, "bar");
        assert_eq!(submodule, FilePath::new("/project/utils/bar.zoya"));
    }

    #[test]
    fn test_fs_source_resolve_submodule_deeply_nested() {
        let source = FsSource::new("/project");
        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
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
        let root_path = QualifiedPath::root();
        assert_eq!(source.resolve_submodule(&root_path, "utils"), "utils");

        // Nested (utils module looking for helpers)
        let utils_path = QualifiedPath::root().child("utils");
        assert_eq!(source.resolve_submodule(&utils_path, "helpers"), "utils/helpers");

        // Deeply nested
        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
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
        assert_eq!(root.path, QualifiedPath::root());
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
        let utils_path = QualifiedPath::root().child("utils");
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
        let utils_path = QualifiedPath::root().child("utils");
        let utils = tree.get(&utils_path).unwrap();
        assert!(utils.children.contains_key("helpers"));

        // Check helpers module exists
        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
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

        let c_path = QualifiedPath::root().child("a").child("b").child("c");
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

        assert!(tree.get(&QualifiedPath::root()).is_some());
        assert!(tree
            .get(&QualifiedPath::root().child("utils"))
            .is_some());
        assert!(tree
            .get(&QualifiedPath::root().child("nonexistent"))
            .is_none());
    }

    // === MemorySource integration tests ===

    #[test]
    fn test_memory_source_load_single_module() {
        let source = MemorySource::new()
            .with_module("root", "fn foo() -> Int 42");

        let tree = load_memory_package(&source).unwrap();

        assert_eq!(tree.name, "root");
        assert_eq!(tree.output, None);
        assert_eq!(tree.modules.len(), 1);
        let root = tree.root().unwrap();
        assert_eq!(root.path, QualifiedPath::root());
        assert!(root.children.is_empty());
        assert_eq!(root.items.len(), 1);
    }

    #[test]
    fn test_memory_source_load_with_submodule() {
        let source = MemorySource::new()
            .with_module("root", "mod utils\nfn main() -> Int 42")
            .with_module("utils", "fn helper() -> Int 10");

        let tree = load_memory_package(&source).unwrap();

        assert_eq!(tree.modules.len(), 2);

        let root = tree.root().unwrap();
        assert_eq!(root.children.len(), 1);
        assert!(root.children.contains_key("utils"));
        assert_eq!(root.items.len(), 1);

        let utils_path = QualifiedPath::root().child("utils");
        let utils = tree.get(&utils_path).unwrap();
        assert_eq!(utils.items.len(), 1);
    }

    #[test]
    fn test_memory_source_load_nested_modules() {
        let source = MemorySource::new()
            .with_module("root", "mod utils")
            .with_module("utils", "mod helpers")
            .with_module("utils/helpers", "fn deep_fn() -> Int 42");

        let tree = load_memory_package(&source).unwrap();

        assert_eq!(tree.modules.len(), 3);

        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
        let helpers = tree.get(&helpers_path).unwrap();
        assert_eq!(helpers.items.len(), 1);
    }

    #[test]
    fn test_memory_source_error_module_not_found() {
        let source = MemorySource::new()
            .with_module("root", "mod missing");

        let result = load_memory_package(&source);

        assert!(
            matches!(result, Err(LoaderError::ModuleNotFound { mod_name, .. }) if mod_name == "missing")
        );
    }

    #[test]
    fn test_memory_source_error_invalid_mod_name_pascal_case() {
        let source = MemorySource::new()
            .with_module("root", "mod MyModule")
            .with_module("my_module", "");

        let result = load_memory_package(&source);

        assert!(
            matches!(result, Err(LoaderError::InvalidModName { mod_name, suggestion }) if mod_name == "MyModule" && suggestion == "my_module")
        );
    }

    #[test]
    fn test_memory_source_error_invalid_mod_name_leading_underscore() {
        let source = MemorySource::new()
            .with_module("root", "mod _private")
            .with_module("_private", "");

        let result = load_memory_package(&source);

        assert!(
            matches!(result, Err(LoaderError::InvalidModName { mod_name, .. }) if mod_name == "_private")
        );
    }

    #[test]
    fn test_memory_source_error_reserved_mod_name_std() {
        let source = MemorySource::new()
            .with_module("root", "mod std")
            .with_module("std", "");

        let result = load_memory_package(&source);

        assert!(
            matches!(result, Err(LoaderError::ReservedModName { mod_name }) if mod_name == "std")
        );
    }

    #[test]
    fn test_memory_source_error_reserved_mod_name_zoya() {
        let source = MemorySource::new()
            .with_module("root", "mod zoya")
            .with_module("zoya", "");

        let result = load_memory_package(&source);

        assert!(
            matches!(result, Err(LoaderError::ReservedModName { mod_name }) if mod_name == "zoya")
        );
    }

    #[test]
    fn test_memory_source_error_missing_root() {
        let source = MemorySource::new();

        let result = load_memory_package(&source);

        assert!(matches!(result, Err(LoaderError::MissingRoot)));
    }
}
