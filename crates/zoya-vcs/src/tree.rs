use std::collections::{HashMap, HashSet};

use zoya_package::Package;

use crate::Blob;
use crate::diff::Change;
use crate::merge::{self, TreeMergeResult};

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

    pub fn three_way_merge(
        base: Option<&Tree>,
        ours: Option<&Tree>,
        theirs: Option<&Tree>,
    ) -> TreeMergeResult {
        merge::three_way_merge_trees(base, ours, theirs)
    }

    pub fn diff(&self, other: &Tree) -> Vec<Change> {
        let self_paths: HashSet<&String> = self.blobs.keys().collect();
        let other_paths: HashSet<&String> = other.blobs.keys().collect();

        let mut added: Vec<(String, Blob)> = Vec::new();
        let mut removed: Vec<(String, Blob)> = Vec::new();
        let mut changes: Vec<Change> = Vec::new();

        // Paths in other but not self → Added candidates
        for path in other_paths.difference(&self_paths) {
            let blob = other.blobs[*path].clone();
            added.push(((*path).clone(), blob));
        }

        // Paths in self but not other → Removed candidates
        for path in self_paths.difference(&other_paths) {
            let blob = self.blobs[*path].clone();
            removed.push(((*path).clone(), blob));
        }

        // Paths in both with different blob IDs → Updated
        for path in self_paths.intersection(&other_paths) {
            let old_blob = &self.blobs[*path];
            let new_blob = &other.blobs[*path];
            if old_blob.id() != new_blob.id() {
                let diff = old_blob.diff(new_blob);
                changes.push(Change::Updated {
                    path: (*path).clone(),
                    old: old_blob.clone(),
                    new: new_blob.clone(),
                    diff,
                });
            }
        }

        // Rename detection: match removed → added by blob_id
        let mut matched_added: HashSet<usize> = HashSet::new();
        let mut matched_removed: HashSet<usize> = HashSet::new();

        for (ri, (old_path, removed_blob)) in removed.iter().enumerate() {
            for (ai, (new_path, added_blob)) in added.iter().enumerate() {
                if !matched_added.contains(&ai) && removed_blob.id() == added_blob.id() {
                    changes.push(Change::Renamed {
                        old_path: old_path.clone(),
                        new_path: new_path.clone(),
                        blob: removed_blob.clone(),
                    });
                    matched_added.insert(ai);
                    matched_removed.insert(ri);
                    break;
                }
            }
        }

        // Remaining added/removed
        for (i, (path, blob)) in added.into_iter().enumerate() {
            if !matched_added.contains(&i) {
                changes.push(Change::Added { path, blob });
            }
        }
        for (i, (path, blob)) in removed.into_iter().enumerate() {
            if !matched_removed.contains(&i) {
                changes.push(Change::Removed { path, blob });
            }
        }

        // Sort: Added/Removed/Updated by path, Renamed by old_path
        changes.sort_by(|a, b| {
            let key = |c: &Change| match c {
                Change::Added { path, .. } => path.clone(),
                Change::Removed { path, .. } => path.clone(),
                Change::Updated { path, .. } => path.clone(),
                Change::Renamed { old_path, .. } => old_path.clone(),
            };
            key(a).cmp(&key(b))
        });

        changes
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
    use crate::diff::DiffHunk;

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
    fn test_diff_no_changes() {
        let mut blobs = HashMap::new();
        blobs.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree1 = Tree::new(blobs.clone());
        let tree2 = Tree::new(blobs);
        assert!(tree1.diff(&tree2).is_empty());
    }

    #[test]
    fn test_diff_added_file() {
        let tree1 = Tree::new(HashMap::new());
        let mut blobs = HashMap::new();
        blobs.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree2 = Tree::new(blobs);

        let changes = tree1.diff(&tree2);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            Change::Added { path, blob } => {
                assert_eq!(path, "a");
                assert_eq!(blob.content(), "hello");
            }
            _ => panic!("expected Added"),
        }
    }

    #[test]
    fn test_diff_removed_file() {
        let mut blobs = HashMap::new();
        blobs.insert("a".to_string(), Blob::new("hello".to_string()));
        let tree1 = Tree::new(blobs);
        let tree2 = Tree::new(HashMap::new());

        let changes = tree1.diff(&tree2);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            Change::Removed { path, blob } => {
                assert_eq!(path, "a");
                assert_eq!(blob.content(), "hello");
            }
            _ => panic!("expected Removed"),
        }
    }

    #[test]
    fn test_diff_updated_file() {
        let mut blobs1 = HashMap::new();
        blobs1.insert("a".to_string(), Blob::new("hello\n".to_string()));
        let tree1 = Tree::new(blobs1);

        let mut blobs2 = HashMap::new();
        blobs2.insert("a".to_string(), Blob::new("hello\nworld\n".to_string()));
        let tree2 = Tree::new(blobs2);

        let changes = tree1.diff(&tree2);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            Change::Updated {
                path,
                old,
                new,
                diff,
            } => {
                assert_eq!(path, "a");
                assert_eq!(old.content(), "hello\n");
                assert_eq!(new.content(), "hello\nworld\n");
                let has_insert = diff.iter().any(
                    |h| matches!(h, DiffHunk::Different { after, .. } if after.contains("world")),
                );
                assert!(has_insert);
            }
            _ => panic!("expected Updated"),
        }
    }

    #[test]
    fn test_diff_renamed_file() {
        let mut blobs1 = HashMap::new();
        blobs1.insert("old.zy".to_string(), Blob::new("content".to_string()));
        let tree1 = Tree::new(blobs1);

        let mut blobs2 = HashMap::new();
        blobs2.insert("new.zy".to_string(), Blob::new("content".to_string()));
        let tree2 = Tree::new(blobs2);

        let changes = tree1.diff(&tree2);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            Change::Renamed {
                old_path,
                new_path,
                blob,
            } => {
                assert_eq!(old_path, "old.zy");
                assert_eq!(new_path, "new.zy");
                assert_eq!(blob.content(), "content");
            }
            _ => panic!("expected Renamed"),
        }
    }

    #[test]
    fn test_diff_mixed_changes() {
        let mut blobs1 = HashMap::new();
        blobs1.insert("keep".to_string(), Blob::new("same\n".to_string()));
        blobs1.insert("modify".to_string(), Blob::new("old\n".to_string()));
        blobs1.insert("remove".to_string(), Blob::new("gone".to_string()));
        blobs1.insert("rename_old".to_string(), Blob::new("moved".to_string()));
        let tree1 = Tree::new(blobs1);

        let mut blobs2 = HashMap::new();
        blobs2.insert("keep".to_string(), Blob::new("same\n".to_string()));
        blobs2.insert("modify".to_string(), Blob::new("new\n".to_string()));
        blobs2.insert("add".to_string(), Blob::new("fresh".to_string()));
        blobs2.insert("rename_new".to_string(), Blob::new("moved".to_string()));
        let tree2 = Tree::new(blobs2);

        let changes = tree1.diff(&tree2);
        assert_eq!(changes.len(), 4);

        let has_added = changes
            .iter()
            .any(|c| matches!(c, Change::Added { path, .. } if path == "add"));
        let has_removed = changes
            .iter()
            .any(|c| matches!(c, Change::Removed { path, .. } if path == "remove"));
        let has_updated = changes
            .iter()
            .any(|c| matches!(c, Change::Updated { path, .. } if path == "modify"));
        let has_renamed = changes.iter().any(|c| matches!(c, Change::Renamed { old_path, new_path, .. } if old_path == "rename_old" && new_path == "rename_new"));

        assert!(has_added, "missing Added");
        assert!(has_removed, "missing Removed");
        assert!(has_updated, "missing Updated");
        assert!(has_renamed, "missing Renamed");
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
