use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use zoya_ast::Item;

/// Module path: root is `["root"]`, `utils::helpers` is `["root", "utils", "helpers"]`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath(pub Vec<String>);

impl ModulePath {
    pub fn root() -> Self {
        ModulePath(vec!["root".to_string()])
    }

    pub fn child(&self, name: &str) -> Self {
        let mut segments = self.0.clone();
        segments.push(name.to_string());
        ModulePath(segments)
    }

    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            None
        } else {
            let mut segments = self.0.clone();
            segments.pop();
            Some(ModulePath(segments))
        }
    }

    pub fn is_root(&self) -> bool {
        self.0.len() == 1 && self.0[0] == "root"
    }
}

impl std::fmt::Display for ModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("::"))
    }
}

/// A loaded module
#[derive(Debug, Clone)]
pub struct Module {
    pub items: Vec<Item>,
    pub path: ModulePath,
    pub children: HashMap<String, ModulePath>,
}

/// The complete module tree
#[derive(Debug, Clone)]
pub struct ModuleTree {
    pub modules: HashMap<ModulePath, Module>,
}

impl ModuleTree {
    pub fn root(&self) -> Option<&Module> {
        self.modules.get(&ModulePath::root())
    }

    pub fn get(&self, path: &ModulePath) -> Option<&Module> {
        self.modules.get(path)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoaderError {
    ModuleNotFound {
        mod_name: String,
        expected_path: PathBuf,
    },
    DuplicateMod {
        mod_name: String,
    },
    IoError {
        path: PathBuf,
        message: String,
    },
    LexError {
        path: PathBuf,
        message: String,
    },
    ParseError {
        path: PathBuf,
        message: String,
    },
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::ModuleNotFound {
                mod_name,
                expected_path,
            } => {
                write!(
                    f,
                    "module '{}' not found at '{}'",
                    mod_name,
                    expected_path.display()
                )
            }
            LoaderError::DuplicateMod { mod_name } => {
                write!(f, "duplicate module declaration: '{}'", mod_name)
            }
            LoaderError::IoError { path, message } => {
                write!(f, "failed to read '{}': {}", path.display(), message)
            }
            LoaderError::LexError { path, message } => {
                write!(f, "lexer error in '{}': {}", path.display(), message)
            }
            LoaderError::ParseError { path, message } => {
                write!(f, "parse error in '{}': {}", path.display(), message)
            }
        }
    }
}

impl std::error::Error for LoaderError {}

/// Load module tree starting from root file
pub fn load_modules(path: &Path) -> Result<ModuleTree, LoaderError> {
    let mut tree = ModuleTree {
        modules: HashMap::new(),
    };
    let base_dir = path.parent().unwrap_or(Path::new("."));
    load_module_recursive(path, ModulePath::root(), base_dir, &mut tree)?;
    Ok(tree)
}

fn load_module_recursive(
    file_path: &Path,
    module_path: ModulePath,
    base_dir: &Path,
    tree: &mut ModuleTree,
) -> Result<(), LoaderError> {
    // Read file
    let source = std::fs::read_to_string(file_path).map_err(|e| LoaderError::IoError {
        path: file_path.to_path_buf(),
        message: e.to_string(),
    })?;

    // Lex
    let tokens = zoya_lexer::lex(&source).map_err(|e| LoaderError::LexError {
        path: file_path.to_path_buf(),
        message: e.message,
    })?;

    // Parse
    let module_def = zoya_parser::parse_module(tokens).map_err(|e| LoaderError::ParseError {
        path: file_path.to_path_buf(),
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

        let submodule_file = compute_module_file_path(base_dir, &module_path, &mod_decl.name);
        if !submodule_file.exists() {
            return Err(LoaderError::ModuleNotFound {
                mod_name: mod_decl.name.clone(),
                expected_path: submodule_file,
            });
        }
        submodules.push((submodule_file, child_path));
    }

    // Store module
    tree.modules.insert(
        module_path.clone(),
        Module {
            items: module_def.items,
            path: module_path,
            children,
        },
    );

    // Recursively load submodules
    for (submodule_file, child_path) in submodules {
        load_module_recursive(&submodule_file, child_path, base_dir, tree)?;
    }

    Ok(())
}

/// Get path segments for filesystem resolution (strips "root" prefix)
/// Panics if path is not a local module (doesn't start with "root")
fn filesystem_segments(path: &ModulePath) -> &[String] {
    assert!(
        path.0.first().map(|s| s.as_str()) == Some("root"),
        "filesystem_segments called on non-local path: {}",
        path
    );
    &path.0[1..]
}

/// Compute file path for a submodule
/// - `mod foo` in root -> `base_dir/foo.zoya`
/// - `mod bar` in `utils` -> `base_dir/utils/bar.zoya`
fn compute_module_file_path(base_dir: &Path, parent_module: &ModulePath, mod_name: &str) -> PathBuf {
    let mut path = base_dir.to_path_buf();
    for segment in filesystem_segments(parent_module) {
        path.push(segment);
    }
    path.push(format!("{}.zoya", mod_name));
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    // === ModulePath unit tests ===

    #[test]
    fn test_module_path_root() {
        let root = ModulePath::root();
        assert!(root.is_root());
        assert_eq!(root.0, vec!["root"]);
        assert_eq!(root.to_string(), "root");
    }

    #[test]
    fn test_module_path_child() {
        let root = ModulePath::root();
        let utils = root.child("utils");
        assert_eq!(utils.0, vec!["root", "utils"]);
        assert_eq!(utils.to_string(), "root::utils");
        assert!(!utils.is_root());
    }

    #[test]
    fn test_module_path_parent() {
        let root = ModulePath::root();
        assert!(root.parent().is_none());

        let utils = root.child("utils");
        assert_eq!(utils.parent(), Some(ModulePath::root()));

        let helpers = utils.child("helpers");
        assert_eq!(
            helpers.parent(),
            Some(ModulePath(vec!["root".to_string(), "utils".to_string()]))
        );
    }

    #[test]
    fn test_module_path_deeply_nested() {
        let path = ModulePath::root().child("a").child("b").child("c");
        assert_eq!(path.0, vec!["root", "a", "b", "c"]);
        assert_eq!(path.to_string(), "root::a::b::c");
    }

    // === File path computation tests ===

    #[test]
    fn test_compute_module_file_path_root() {
        let base = Path::new("/project");
        let path = compute_module_file_path(base, &ModulePath::root(), "foo");
        assert_eq!(path, PathBuf::from("/project/foo.zoya"));
    }

    #[test]
    fn test_compute_module_file_path_nested() {
        let base = Path::new("/project");
        let utils = ModulePath(vec!["root".to_string(), "utils".to_string()]);
        let path = compute_module_file_path(base, &utils, "bar");
        assert_eq!(path, PathBuf::from("/project/utils/bar.zoya"));
    }

    #[test]
    fn test_compute_module_file_path_deeply_nested() {
        let base = Path::new("/project");
        let helpers = ModulePath(vec![
            "root".to_string(),
            "utils".to_string(),
            "helpers".to_string(),
        ]);
        let path = compute_module_file_path(base, &helpers, "baz");
        assert_eq!(path, PathBuf::from("/project/utils/helpers/baz.zoya"));
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

    // === Basic loading tests ===

    #[test]
    fn test_load_single_file_no_mods() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "fn foo() -> Int 42");

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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

        let result = load_modules(&dir.path().join("main.zoya"));

        assert!(
            matches!(result, Err(LoaderError::ModuleNotFound { mod_name, .. }) if mod_name == "missing")
        );
    }

    #[test]
    fn test_error_duplicate_mod() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils mod utils");
        create_file(dir.path(), "utils.zoya", "");

        let result = load_modules(&dir.path().join("main.zoya"));

        assert!(
            matches!(result, Err(LoaderError::DuplicateMod { mod_name }) if mod_name == "utils")
        );
    }

    #[test]
    fn test_error_io_file_not_found() {
        let dir = TempDir::new().unwrap();
        let result = load_modules(&dir.path().join("nonexistent.zoya"));

        assert!(matches!(result, Err(LoaderError::IoError { .. })));
    }

    #[test]
    fn test_error_lex_error() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "fn foo() # invalid");

        let result = load_modules(&dir.path().join("main.zoya"));

        assert!(matches!(result, Err(LoaderError::LexError { .. })));
    }

    #[test]
    fn test_error_parse_error() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "fn fn fn");

        let result = load_modules(&dir.path().join("main.zoya"));

        assert!(matches!(result, Err(LoaderError::ParseError { .. })));
    }

    // === ModuleTree API tests ===

    #[test]
    fn test_module_tree_get() {
        let dir = TempDir::new().unwrap();
        create_file(dir.path(), "main.zoya", "mod utils");
        create_file(dir.path(), "utils.zoya", "");

        let tree = load_modules(&dir.path().join("main.zoya")).unwrap();

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
}
