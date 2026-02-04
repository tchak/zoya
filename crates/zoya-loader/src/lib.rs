use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};

// Re-export module types from zoya-package
pub use zoya_package::{Module, ModulePath, Package};

// ============================================================================
// Path Wrapper (PathBuf with Display)
// ============================================================================

/// A wrapper around PathBuf that implements Display
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub PathBuf);

impl FilePath {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_path_buf())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, path: impl AsRef<Path>) -> Self {
        Self(self.0.join(path))
    }

    pub fn parent(&self) -> Option<&Path> {
        self.0.parent()
    }

    pub fn file_stem(&self) -> Option<&std::ffi::OsStr> {
        self.0.file_stem()
    }

    pub fn file_name(&self) -> Option<&std::ffi::OsStr> {
        self.0.file_name()
    }

    pub fn exists(&self) -> bool {
        self.0.exists()
    }
}

impl Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<PathBuf> for FilePath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

impl From<&Path> for FilePath {
    fn from(path: &Path) -> Self {
        Self(path.to_path_buf())
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

// ============================================================================
// Source Error Types
// ============================================================================

/// Error kind for source operations
#[derive(Debug, Clone, PartialEq)]
pub enum SourceErrorKind {
    NotFound,
    PermissionDenied,
    IoError,
    Other,
}

/// Error from a module source operation
#[derive(Debug, Clone, PartialEq)]
pub struct SourceError {
    pub kind: SourceErrorKind,
    pub message: String,
}

impl SourceError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: SourceErrorKind::NotFound,
            message: message.into(),
        }
    }

    pub fn io_error(message: impl Into<String>) -> Self {
        Self {
            kind: SourceErrorKind::IoError,
            message: message.into(),
        }
    }
}

impl Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SourceError {}

// ============================================================================
// ModuleSource Trait
// ============================================================================

/// Trait for abstracting module source backends (filesystem, memory, etc.)
pub trait ModuleSource {
    /// The path type used by this source (e.g., PathBuf for filesystem, String for memory)
    type Path: Clone + Debug + Display;

    /// Read the source code at the given path
    fn read(&self, path: &Self::Path) -> Result<String, SourceError>;

    /// Check if a module exists at the given path
    fn exists(&self, path: &Self::Path) -> bool;

    /// Resolve the path for a submodule given the current module's logical path and the submodule name
    fn resolve_submodule(&self, module_path: &ModulePath, mod_name: &str) -> Self::Path;
}

// ============================================================================
// Filesystem Source
// ============================================================================

/// Filesystem-based module source
pub struct FsSource {
    base_dir: PathBuf,
}

impl FsSource {
    /// Create a new FsSource with the given base directory
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a new FsSource from a file path, using its parent directory as base
    pub fn from_file(file_path: &Path) -> Self {
        let base_dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Self { base_dir }
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

impl ModuleSource for FsSource {
    type Path = FilePath;

    fn read(&self, path: &Self::Path) -> Result<String, SourceError> {
        std::fs::read_to_string(&path.0).map_err(|e| {
            let kind = match e.kind() {
                std::io::ErrorKind::NotFound => SourceErrorKind::NotFound,
                std::io::ErrorKind::PermissionDenied => SourceErrorKind::PermissionDenied,
                _ => SourceErrorKind::IoError,
            };
            SourceError {
                kind,
                message: e.to_string(),
            }
        })
    }

    fn exists(&self, path: &Self::Path) -> bool {
        path.exists()
    }

    fn resolve_submodule(&self, module_path: &ModulePath, mod_name: &str) -> Self::Path {
        // Build path from base_dir using module path segments (skipping "root")
        let mut path = self.base_dir.clone();
        for segment in &module_path.0[1..] {
            // Skip "root"
            path.push(segment);
        }
        path.push(format!("{}.zoya", mod_name));
        FilePath::new(path)
    }
}

// ============================================================================
// Memory Source
// ============================================================================

/// In-memory module source for testing
pub struct MemorySource {
    modules: HashMap<String, String>,
}

impl MemorySource {
    /// Create a new empty MemorySource
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Add a module with the given path and source
    pub fn with_module(mut self, path: impl Into<String>, source: impl Into<String>) -> Self {
        self.modules.insert(path.into(), source.into());
        self
    }

    /// Add a module with the given path and source (mutable version)
    pub fn add_module(&mut self, path: impl Into<String>, source: impl Into<String>) {
        self.modules.insert(path.into(), source.into());
    }
}

impl Default for MemorySource {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleSource for MemorySource {
    type Path = String;

    fn read(&self, path: &Self::Path) -> Result<String, SourceError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| SourceError::not_found(format!("module not found: {}", path)))
    }

    fn exists(&self, path: &Self::Path) -> bool {
        self.modules.contains_key(path)
    }

    fn resolve_submodule(&self, module_path: &ModulePath, mod_name: &str) -> Self::Path {
        // Build path from module path segments (skipping "root")
        let segments: Vec<&str> = module_path.0[1..].iter().map(|s| s.as_str()).collect();
        if segments.is_empty() {
            mod_name.to_string()
        } else {
            format!("{}/{}", segments.join("/"), mod_name)
        }
    }
}

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
            uses: module_def.uses,
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
