//! Module data structures for Zoya.
//!
//! This crate provides the core module-related types used across the Zoya compiler:
//! - `ModulePath`: Logical path to a module in the module tree
//! - `Module`: A loaded module containing parsed items
//! - `ModuleTree`: The complete tree of loaded modules

use std::collections::HashMap;

use zoya_ast::{Item, UseDecl};

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

    /// Get the depth of this module path (number of segments)
    pub fn depth(&self) -> usize {
        self.0.len()
    }

    /// Get the segments of this module path
    pub fn segments(&self) -> &[String] {
        &self.0
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
    pub uses: Vec<UseDecl>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
