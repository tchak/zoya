use std::collections::HashMap;

use zoya_package::Package;

use crate::Blob;

/// A content-addressed tree mapping paths to blobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    id: String,
    blobs: HashMap<String, Blob>,
}

impl Tree {
    pub fn new(blobs: HashMap<String, Blob>) -> Self {
        let id = compute_tree_id(&blobs);
        Tree { id, blobs }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn blobs(&self) -> &HashMap<String, Blob> {
        &self.blobs
    }

    pub fn get(&self, path: &str) -> Option<&Blob> {
        self.blobs.get(path)
    }

    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.blobs.len()
    }
}

fn compute_tree_id(blobs: &HashMap<String, Blob>) -> String {
    let mut entries: Vec<_> = blobs.iter().collect();
    entries.sort_by_key(|(path, _)| path.as_str());

    let mut hasher = blake3::Hasher::new();
    for (path, blob) in entries {
        hasher.update(path.as_bytes());
        hasher.update(blob.id().as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

impl From<Package> for Tree {
    fn from(package: Package) -> Self {
        let blobs = package
            .modules
            .into_iter()
            .map(|(path, module)| {
                let content = zoya_fmt::fmt(module.items);
                (path.to_string(), Blob::new(content))
            })
            .collect();
        Tree::new(blobs)
    }
}

impl From<&Package> for Tree {
    fn from(package: &Package) -> Self {
        let blobs = package
            .modules
            .iter()
            .map(|(path, module)| {
                let content = zoya_fmt::fmt(module.items.clone());
                (path.to_string(), Blob::new(content))
            })
            .collect();
        Tree::new(blobs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zoya_package::{Module, Package, QualifiedPath};

    use super::*;

    fn parse_items(source: &str) -> Vec<zoya_ast::Item> {
        let tokens = zoya_lexer::lex(source).expect("lex failed");
        zoya_parser::parse_module(tokens).expect("parse failed")
    }

    #[test]
    fn test_deterministic_id() {
        let mut blobs = HashMap::new();
        blobs.insert("a".to_string(), Blob::new("hello".to_string()));
        blobs.insert("b".to_string(), Blob::new("world".to_string()));
        let tree1 = Tree::new(blobs);

        let mut blobs = HashMap::new();
        blobs.insert("a".to_string(), Blob::new("hello".to_string()));
        blobs.insert("b".to_string(), Blob::new("world".to_string()));
        let tree2 = Tree::new(blobs);

        assert_eq!(tree1.id(), tree2.id());
    }

    #[test]
    fn test_insertion_order_independence() {
        let mut blobs1 = HashMap::new();
        blobs1.insert("a".to_string(), Blob::new("hello".to_string()));
        blobs1.insert("b".to_string(), Blob::new("world".to_string()));
        let tree1 = Tree::new(blobs1);

        let mut blobs2 = HashMap::new();
        blobs2.insert("b".to_string(), Blob::new("world".to_string()));
        blobs2.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree2 = Tree::new(blobs2);

        assert_eq!(tree1.id(), tree2.id());
    }

    #[test]
    fn test_content_change_affects_id() {
        let mut blobs1 = HashMap::new();
        blobs1.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree1 = Tree::new(blobs1);

        let mut blobs2 = HashMap::new();
        blobs2.insert("a".to_string(), Blob::new("world".to_string()));
        let tree2 = Tree::new(blobs2);

        assert_ne!(tree1.id(), tree2.id());
    }

    #[test]
    fn test_path_change_affects_id() {
        let mut blobs1 = HashMap::new();
        blobs1.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree1 = Tree::new(blobs1);

        let mut blobs2 = HashMap::new();
        blobs2.insert("b".to_string(), Blob::new("hello".to_string()));
        let tree2 = Tree::new(blobs2);

        assert_ne!(tree1.id(), tree2.id());
    }

    #[test]
    fn test_empty_tree() {
        let tree = Tree::new(HashMap::new());
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(!tree.id().is_empty());
    }

    #[test]
    fn test_get() {
        let mut blobs = HashMap::new();
        blobs.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree = Tree::new(blobs);

        assert!(tree.get("a").is_some());
        assert!(tree.get("b").is_none());
    }

    #[test]
    fn test_from_package_single_module() {
        let items = parse_items("pub fn main() -> Int 42");
        let mut modules = HashMap::new();
        let root = QualifiedPath::root();
        modules.insert(
            root.clone(),
            Module {
                items,
                path: root,
                children: HashMap::new(),
            },
        );
        let package = Package {
            name: "test".to_string(),
            modules,
        };

        let tree = Tree::from(package);
        assert_eq!(tree.len(), 1);

        let blob = tree.get("root").expect("root blob missing");
        assert_eq!(blob.content(), "pub fn main() -> Int 42\n");
    }

    #[test]
    fn test_from_package_multiple_modules() {
        let root_items = parse_items("pub fn main() -> Int 42");
        let utils_items = parse_items("fn helper() -> Int 1");

        let root = QualifiedPath::root();
        let utils = root.child("utils");

        let mut modules = HashMap::new();
        modules.insert(
            root.clone(),
            Module {
                items: root_items,
                path: root,
                children: HashMap::new(),
            },
        );
        modules.insert(
            utils.clone(),
            Module {
                items: utils_items,
                path: utils,
                children: HashMap::new(),
            },
        );

        let package = Package {
            name: "test".to_string(),
            modules,
        };

        let tree = Tree::from(package);
        assert_eq!(tree.len(), 2);
        assert!(tree.get("root").is_some());
        assert!(tree.get("root::utils").is_some());
    }

    #[test]
    fn test_from_package_formats_content() {
        let items = parse_items("fn  bar()  ->  Int  {  1  }  \npub  fn  foo()  ->  Int  {  2  }");
        let mut modules = HashMap::new();
        let root = QualifiedPath::root();
        modules.insert(
            root.clone(),
            Module {
                items,
                path: root,
                children: HashMap::new(),
            },
        );
        let package = Package {
            name: "test".to_string(),
            modules,
        };

        let tree = Tree::from(package);
        let blob = tree.get("root").expect("root blob missing");
        // Formatter produces canonical output
        assert_eq!(
            blob.content(),
            "fn bar() -> Int 1\n\npub fn foo() -> Int 2\n"
        );
    }

    #[test]
    fn test_from_ref_package() {
        let items = parse_items("pub fn main() -> Int 42");
        let mut modules = HashMap::new();
        let root = QualifiedPath::root();
        modules.insert(
            root.clone(),
            Module {
                items,
                path: root,
                children: HashMap::new(),
            },
        );
        let package = Package {
            name: "test".to_string(),
            modules,
        };

        let tree = Tree::from(&package);
        assert_eq!(tree.len(), 1);
        // Package is still usable after borrowing
        assert!(package.root().is_some());
    }
}
