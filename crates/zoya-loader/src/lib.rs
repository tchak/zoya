use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};

use zoya_ast::{Attribute, AttributeArg};

// Re-export module types from zoya-package
pub use zoya_package::{ConfigError, PackageConfig};
pub use zoya_package::{Module, Package, QualifiedPath};

mod source;
mod sources;

pub use source::{ModuleSource, SourceError, SourceErrorKind};
pub use sources::{FilePath, FsSource, MemorySource};

/// Compilation mode controlling which items are included
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Development mode (default) — test items excluded
    #[default]
    Dev,
    /// Test mode — all items included
    Test,
    /// Release mode — test items excluded
    Release,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Dev => write!(f, "dev"),
            Mode::Test => write!(f, "test"),
            Mode::Release => write!(f, "release"),
        }
    }
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dev" => Ok(Mode::Dev),
            "test" => Ok(Mode::Test),
            "release" => Ok(Mode::Release),
            _ => Err(format!(
                "invalid mode '{}': expected 'dev', 'test', or 'release'",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoaderError<P: Clone + Debug + Display = FilePath> {
    #[error("module '{mod_name}' not found at '{expected_path}'")]
    ModuleNotFound { mod_name: String, expected_path: P },
    #[error("duplicate module declaration: '{mod_name}'")]
    DuplicateMod { mod_name: String },
    #[error("failed to read '{path}': {error}")]
    SourceError { path: P, error: SourceError },
    #[error("lexer error in '{path}': {source}")]
    LexError {
        path: P,
        source: zoya_lexer::LexError,
    },
    #[error("parse error in '{path}': {source}")]
    ParseError {
        path: P,
        source: zoya_parser::ParseError,
    },
    #[error(
        "invalid module name '{mod_name}': module names must be snake_case (try '{suggestion}')"
    )]
    InvalidModName {
        mod_name: String,
        suggestion: String,
    },
    #[error(
        "invalid module name '{mod_name}': this name is reserved (root, self, super, std, zoya)"
    )]
    ReservedModName { mod_name: String },
    #[error("no package.toml found in '{}'\nhint: provide a .zy file path or create a package with `zoya init`", dir.display())]
    NoPackageToml { dir: PathBuf },
    #[error("main file '{}' not found in package at '{}'", main.display(), package_dir.display())]
    MainNotFound { main: PathBuf, package_dir: PathBuf },
    #[error("{0}")]
    ConfigError(String),
    #[error("missing root module")]
    MissingRoot,
    #[error("{message}")]
    InvalidAttribute { message: String },
}

/// Load a package from a filesystem path.
///
/// Handles three cases:
/// - **Directory**: looks for `package.toml`, loads config, resolves main file
/// - **`package.toml` file**: loads config from it
/// - **`.zy` file**: treats as standalone file, derives name from filename
pub fn load_package(path: &Path, mode: Mode) -> Result<Package, LoaderError<FilePath>> {
    if path.is_dir() {
        return load_from_directory(path, mode);
    }

    if path.file_name().is_some_and(|f| f == "package.toml") {
        let dir = path.parent().unwrap_or(Path::new("."));
        return load_from_directory(dir, mode);
    }

    // Standalone .zy file
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| zoya_naming::package_name_to_module_name(&zoya_naming::sanitize_package_name(s)))
        .unwrap_or_else(|| "root".to_string());

    let source = FsSource::from_file(path);
    load_with_source(&source, &FilePath::new(path), name, mode)
}

fn load_from_directory(dir: &Path, mode: Mode) -> Result<Package, LoaderError<FilePath>> {
    let config_path = dir.join("package.toml");
    if !config_path.exists() {
        return Err(LoaderError::NoPackageToml {
            dir: dir.to_path_buf(),
        });
    }

    let config = PackageConfig::load(dir).map_err(|e| LoaderError::ConfigError(e.to_string()))?;
    let name = config.module_name();
    let main = config.main_path();
    let main_path = dir.join(&main);

    if !main_path.exists() {
        return Err(LoaderError::MainNotFound {
            main,
            package_dir: dir.to_path_buf(),
        });
    }

    let source = FsSource::from_file(&main_path);
    load_with_source(&source, &FilePath::new(&main_path), name, mode)
}

/// Load a package from an in-memory source.
///
/// Expects a "root" module to exist in the source.
pub fn load_memory_package(
    source: &MemorySource,
    mode: Mode,
) -> Result<Package, LoaderError<String>> {
    let root_path = "root".to_string();
    if !source.exists(&root_path) {
        return Err(LoaderError::MissingRoot);
    }
    load_with_source(source, &root_path, "root".to_string(), mode)
}

fn load_with_source<S: ModuleSource>(
    source: &S,
    file_path: &S::Path,
    name: String,
    mode: Mode,
) -> Result<Package, LoaderError<S::Path>> {
    let mut pkg = Package {
        name,
        modules: HashMap::new(),
    };
    load_module_recursive(source, file_path, QualifiedPath::root(), &mut pkg, mode)?;
    Ok(pkg)
}

/// Check if attributes mark an item as test-only
fn is_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.name == "test"
            || (a.name == "mode"
                && a.args.as_ref().is_some_and(|args| {
                    args.iter()
                        .any(|arg| matches!(arg, AttributeArg::Identifier(s) if s == "test"))
                }))
    })
}

/// Get attributes for an item
fn item_attributes(item: &zoya_ast::Item) -> &[Attribute] {
    match item {
        zoya_ast::Item::Function(f) => &f.attributes,
        zoya_ast::Item::Struct(s) => &s.attributes,
        zoya_ast::Item::Enum(e) => &e.attributes,
        zoya_ast::Item::TypeAlias(t) => &t.attributes,
        zoya_ast::Item::Use(u) => &u.attributes,
        zoya_ast::Item::Impl(i) => &i.attributes,
        zoya_ast::Item::ModDecl(m) => &m.attributes,
    }
}

fn load_module_recursive<S: ModuleSource>(
    source: &S,
    file_path: &S::Path,
    module_path: QualifiedPath,
    pkg: &mut Package,
    mode: Mode,
) -> Result<(), LoaderError<S::Path>> {
    // Read file
    let content = source
        .read(file_path)
        .map_err(|e| LoaderError::SourceError {
            path: file_path.clone(),
            error: e,
        })?;

    // Lex
    let tokens = zoya_lexer::lex(&content).map_err(|e| LoaderError::LexError {
        path: file_path.clone(),
        source: e,
    })?;

    // Parse
    let all_items = zoya_parser::parse_module(tokens).map_err(|e| LoaderError::ParseError {
        path: file_path.clone(),
        source: e,
    })?;

    // Partition items: extract ModDecl items, keep the rest
    let mut mod_decls = Vec::new();
    let mut items = Vec::new();
    for item in all_items {
        match item {
            zoya_ast::Item::ModDecl(m) => mod_decls.push(m),
            other => items.push(other),
        }
    }

    // Validate: #[test] is not allowed on mod declarations (in any mode)
    for mod_decl in &mod_decls {
        if mod_decl.attributes.iter().any(|a| a.name == "test") {
            return Err(LoaderError::InvalidAttribute {
                message: format!(
                    "#[test] is not valid on module '{}'; use #[mode(test)] instead",
                    mod_decl.name
                ),
            });
        }
    }

    // Filter items and mod_decls in non-test modes
    let items = if mode == Mode::Test {
        items
    } else {
        items
            .into_iter()
            .filter(|item| !is_test_only(item_attributes(item)))
            .collect()
    };

    let mod_decls = if mode == Mode::Test {
        mod_decls
    } else {
        mod_decls
            .into_iter()
            .filter(|m| !is_test_only(&m.attributes))
            .collect()
    };

    // Validate, check duplicates, build children map, and resolve submodules in one pass
    let mut seen_mods = HashSet::new();
    let mut children = HashMap::new();
    let mut submodules = Vec::new();

    for mod_decl in &mod_decls {
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
        if !seen_mods.insert(&mod_decl.name) {
            return Err(LoaderError::DuplicateMod {
                mod_name: mod_decl.name.clone(),
            });
        }

        let child_path = module_path.child(&mod_decl.name);
        children.insert(
            mod_decl.name.clone(),
            (child_path.clone(), mod_decl.visibility),
        );

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
            items,
            path: module_path,
            children,
        },
    );

    // Recursively load submodules
    for (submodule_file, child_path) in submodules {
        load_module_recursive(source, &submodule_file, child_path, pkg, mode)?;
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
        assert_eq!(submodule, FilePath::new("/project/foo.zy"));
    }

    #[test]
    fn test_fs_source_resolve_submodule_nested() {
        let source = FsSource::new("/project");
        let utils_path = QualifiedPath::root().child("utils");
        let submodule = source.resolve_submodule(&utils_path, "bar");
        assert_eq!(submodule, FilePath::new("/project/utils/bar.zy"));
    }

    #[test]
    fn test_fs_source_resolve_submodule_deeply_nested() {
        let source = FsSource::new("/project");
        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
        let submodule = source.resolve_submodule(&helpers_path, "baz");
        assert_eq!(submodule, FilePath::new("/project/utils/helpers/baz.zy"));
    }

    // === MemorySource tests ===

    #[test]
    fn test_memory_source_read() {
        let source = MemorySource::new().with_module("root", "fn main() -> Int { 42 }");

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
        assert_eq!(
            source.resolve_submodule(&utils_path, "helpers"),
            "utils/helpers"
        );

        // Deeply nested
        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
        assert_eq!(
            source.resolve_submodule(&helpers_path, "deep"),
            "utils/helpers/deep"
        );
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
        create_file(dir.path(), "main.zy", "fn foo() -> Int 42");

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

        assert_eq!(tree.modules.len(), 1);
        let root = tree.root().unwrap();
        assert_eq!(root.path, QualifiedPath::root());
        assert!(root.children.is_empty());
        assert_eq!(root.items.len(), 1);
    }

    #[test]
    fn test_load_empty_file() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "");

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

        let root = tree.root().unwrap();
        assert!(root.items.is_empty());
        assert!(root.children.is_empty());
    }

    #[test]
    fn test_load_with_one_submodule() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "mod utils");
        create_file(
            dir.path(),
            "utils.zy",
            "fn add(x: Int, y: Int) -> Int x + y",
        );

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

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
        create_file(dir.path(), "main.zy", "mod utils mod helpers mod types");
        create_file(dir.path(), "utils.zy", "fn util_fn() -> Int 1");
        create_file(dir.path(), "helpers.zy", "fn helper_fn() -> Int 2");
        create_file(dir.path(), "types.zy", "struct Point { x: Int, y: Int }");

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

        assert_eq!(tree.modules.len(), 4);

        let root = tree.root().unwrap();
        assert_eq!(root.children.len(), 3);
    }

    #[test]
    fn test_load_nested_modules() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "mod utils");
        create_file(dir.path(), "utils.zy", "mod helpers");
        create_file(dir.path(), "utils/helpers.zy", "fn deep_fn() -> Int 42");

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

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
        create_file(dir.path(), "main.zy", "mod a");
        create_file(dir.path(), "a.zy", "mod b");
        create_file(dir.path(), "a/b.zy", "mod c");
        create_file(dir.path(), "a/b/c.zy", "fn deep() -> Int 1");

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

        assert_eq!(tree.modules.len(), 4);

        let c_path = QualifiedPath::root().child("a").child("b").child("c");
        let c_module = tree.get(&c_path).unwrap();
        assert_eq!(c_module.items.len(), 1);
    }

    // === Error case tests ===

    #[test]
    fn test_error_module_not_found() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "mod missing");

        let result = load_package(&dir.path().join("main.zy"), Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::ModuleNotFound { mod_name, .. }) if mod_name == "missing")
        );
    }

    #[test]
    fn test_error_duplicate_mod() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "mod utils mod utils");
        create_file(dir.path(), "utils.zy", "");

        let result = load_package(&dir.path().join("main.zy"), Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::DuplicateMod { mod_name }) if mod_name == "utils")
        );
    }

    #[test]
    fn test_error_io_file_not_found() {
        let dir = TempDir::new().unwrap();
        let result = load_package(&dir.path().join("nonexistent.zy"), Mode::Dev);

        assert!(matches!(result, Err(LoaderError::SourceError { .. })));
    }

    #[test]
    fn test_error_lex_error() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "fn foo() $ invalid");

        let result = load_package(&dir.path().join("main.zy"), Mode::Dev);

        assert!(matches!(result, Err(LoaderError::LexError { .. })));
    }

    #[test]
    fn test_error_parse_error() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "fn fn fn");

        let result = load_package(&dir.path().join("main.zy"), Mode::Dev);

        assert!(matches!(result, Err(LoaderError::ParseError { .. })));
    }

    // === Package API tests ===

    #[test]
    fn test_package_get() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zy", "mod utils");
        create_file(dir.path(), "utils.zy", "");

        let tree = load_package(&dir.path().join("main.zy"), Mode::Dev).unwrap();

        assert!(tree.get(&QualifiedPath::root()).is_some());
        assert!(tree.get(&QualifiedPath::root().child("utils")).is_some());
        assert!(
            tree.get(&QualifiedPath::root().child("nonexistent"))
                .is_none()
        );
    }

    // === MemorySource integration tests ===

    #[test]
    fn test_memory_source_load_single_module() {
        let source = MemorySource::new().with_module("root", "fn foo() -> Int 42");

        let tree = load_memory_package(&source, Mode::Dev).unwrap();

        assert_eq!(tree.name, "root");
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

        let tree = load_memory_package(&source, Mode::Dev).unwrap();

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

        let tree = load_memory_package(&source, Mode::Dev).unwrap();

        assert_eq!(tree.modules.len(), 3);

        let helpers_path = QualifiedPath::root().child("utils").child("helpers");
        let helpers = tree.get(&helpers_path).unwrap();
        assert_eq!(helpers.items.len(), 1);
    }

    #[test]
    fn test_memory_source_error_module_not_found() {
        let source = MemorySource::new().with_module("root", "mod missing");

        let result = load_memory_package(&source, Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::ModuleNotFound { mod_name, .. }) if mod_name == "missing")
        );
    }

    #[test]
    fn test_memory_source_error_invalid_mod_name_pascal_case() {
        let source = MemorySource::new()
            .with_module("root", "mod MyModule")
            .with_module("my_module", "");

        let result = load_memory_package(&source, Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::InvalidModName { mod_name, suggestion }) if mod_name == "MyModule" && suggestion == "my_module")
        );
    }

    #[test]
    fn test_memory_source_error_invalid_mod_name_leading_underscore() {
        let source = MemorySource::new()
            .with_module("root", "mod _private")
            .with_module("_private", "");

        let result = load_memory_package(&source, Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::InvalidModName { mod_name, .. }) if mod_name == "_private")
        );
    }

    #[test]
    fn test_memory_source_error_reserved_mod_name_std() {
        let source = MemorySource::new()
            .with_module("root", "mod std")
            .with_module("std", "");

        let result = load_memory_package(&source, Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::ReservedModName { mod_name }) if mod_name == "std")
        );
    }

    #[test]
    fn test_memory_source_error_reserved_mod_name_zoya() {
        let source = MemorySource::new()
            .with_module("root", "mod zoya")
            .with_module("zoya", "");

        let result = load_memory_package(&source, Mode::Dev);

        assert!(
            matches!(result, Err(LoaderError::ReservedModName { mod_name }) if mod_name == "zoya")
        );
    }

    #[test]
    fn test_memory_source_error_missing_root() {
        let source = MemorySource::new();

        let result = load_memory_package(&source, Mode::Dev);

        assert!(matches!(result, Err(LoaderError::MissingRoot)));
    }

    // === Mode filtering tests ===

    #[test]
    fn test_mode_dev_strips_test_fn() {
        let source = MemorySource::new().with_module(
            "root",
            r#"
            fn keep() -> Int { 1 }
            #[test]
            fn test_thing() -> Int { 2 }
            "#,
        );

        let tree = load_memory_package(&source, Mode::Dev).unwrap();
        let root = tree.root().unwrap();
        assert_eq!(root.items.len(), 1); // only keep()
    }

    #[test]
    fn test_mode_test_retains_test_fn() {
        let source = MemorySource::new().with_module(
            "root",
            r#"
            fn keep() -> Int { 1 }
            #[test]
            fn test_thing() -> Int { 2 }
            "#,
        );

        let tree = load_memory_package(&source, Mode::Test).unwrap();
        let root = tree.root().unwrap();
        assert_eq!(root.items.len(), 2); // both keep() and test_thing()
    }

    #[test]
    fn test_mode_dev_strips_mode_test_fn() {
        let source = MemorySource::new().with_module(
            "root",
            r#"
            fn keep() -> Int { 1 }
            #[mode(test)]
            fn test_helper() -> Int { 2 }
            "#,
        );

        let tree = load_memory_package(&source, Mode::Dev).unwrap();
        let root = tree.root().unwrap();
        assert_eq!(root.items.len(), 1);
    }

    #[test]
    fn test_mode_dev_strips_mode_test_mod() {
        let source = MemorySource::new()
            .with_module(
                "root",
                r#"
                #[mode(test)] mod tests
                fn keep() -> Int { 1 }
                "#,
            )
            .with_module("tests", "fn test_fn() -> Int { 42 }");

        let tree = load_memory_package(&source, Mode::Dev).unwrap();
        assert_eq!(tree.modules.len(), 1); // only root, tests module not loaded
        let root = tree.root().unwrap();
        assert!(root.children.is_empty());
    }

    #[test]
    fn test_mode_test_loads_mode_test_mod() {
        let source = MemorySource::new()
            .with_module(
                "root",
                r#"
                #[mode(test)] mod tests
                fn keep() -> Int { 1 }
                "#,
            )
            .with_module("tests", "fn test_fn() -> Int { 42 }");

        let tree = load_memory_package(&source, Mode::Test).unwrap();
        assert_eq!(tree.modules.len(), 2); // root + tests
        let root = tree.root().unwrap();
        assert!(root.children.contains_key("tests"));
    }

    #[test]
    fn test_mode_release_strips_test_fn() {
        let source = MemorySource::new().with_module(
            "root",
            r#"
            fn keep() -> Int { 1 }
            #[test]
            fn test_thing() -> Int { 2 }
            "#,
        );

        let tree = load_memory_package(&source, Mode::Release).unwrap();
        let root = tree.root().unwrap();
        assert_eq!(root.items.len(), 1);
    }

    #[test]
    fn test_test_on_mod_is_error_in_any_mode() {
        let source = MemorySource::new()
            .with_module(
                "root",
                r#"
                #[test] mod tests
                "#,
            )
            .with_module("tests", "");

        let result = load_memory_package(&source, Mode::Dev);
        assert!(matches!(result, Err(LoaderError::InvalidAttribute { .. })));

        let result = load_memory_package(&source, Mode::Test);
        assert!(matches!(result, Err(LoaderError::InvalidAttribute { .. })));
    }
}
