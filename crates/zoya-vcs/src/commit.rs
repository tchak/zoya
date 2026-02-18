use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::Tree;

/// A content-addressed commit pointing to a tree snapshot.
#[derive(Debug, Clone)]
pub struct Commit {
    commit_id: String,
    parents: Vec<String>,
    change_id: Uuid,
    message: String,
    tree: Tree,
    timestamp: SystemTime,
}

impl Commit {
    pub fn new(change_id: Uuid, parents: &[String], tree: Tree, message: String) -> Self {
        let timestamp = SystemTime::now();
        let commit_id = compute_commit_id(parents, &change_id, &message, tree.id(), timestamp);
        Commit {
            commit_id,
            parents: parents.to_vec(),
            change_id,
            message,
            tree,
            timestamp,
        }
    }

    pub(crate) fn restore(
        commit_id: String,
        change_id: Uuid,
        parents: Vec<String>,
        tree: Tree,
        message: String,
        timestamp: SystemTime,
    ) -> Self {
        Commit {
            commit_id,
            change_id,
            parents,
            tree,
            message,
            timestamp,
        }
    }

    pub fn commit_id(&self) -> &str {
        &self.commit_id
    }

    pub fn parents(&self) -> &[String] {
        &self.parents
    }

    pub fn change_id(&self) -> Uuid {
        self.change_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

impl PartialEq for Commit {
    fn eq(&self, other: &Self) -> bool {
        self.commit_id == other.commit_id
    }
}

impl Eq for Commit {}

impl Hash for Commit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.commit_id.hash(state);
    }
}

pub(crate) fn compute_commit_id(
    parents: &[String],
    change_id: &Uuid,
    message: &str,
    tree_id: &str,
    timestamp: SystemTime,
) -> String {
    let duration = timestamp.duration_since(UNIX_EPOCH).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    for parent in parents {
        hasher.update(parent.as_bytes());
    }
    hasher.update(change_id.as_bytes());
    hasher.update(message.as_bytes());
    hasher.update(tree_id.as_bytes());
    hasher.update(&duration.as_secs().to_le_bytes());
    hasher.update(&duration.subsec_nanos().to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::Blob;

    use super::*;

    fn make_tree(content: &str) -> Tree {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        Tree::new(blobs)
    }

    const CHANGE_1: Uuid = Uuid::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ]);

    #[test]
    fn test_minimal_commit() {
        let tree = make_tree("hello");
        let commit = Commit::new(CHANGE_1, &[], tree, String::new());

        assert!(!commit.commit_id().is_empty());
        assert!(commit.parents().is_empty());
        assert_eq!(commit.change_id(), CHANGE_1);
        assert_eq!(commit.message(), "");
    }

    #[test]
    fn test_commit_with_message() {
        let tree = make_tree("hello");
        let commit = Commit::new(CHANGE_1, &[], tree, "initial commit".to_string());

        assert_eq!(commit.message(), "initial commit");
    }

    #[test]
    fn test_commit_with_parent() {
        let tree = make_tree("hello");
        let commit = Commit::new(CHANGE_1, &["parent-abc".to_string()], tree, String::new());

        assert_eq!(commit.parents(), &["parent-abc".to_string()]);
    }

    #[test]
    fn test_commit_with_all_fields() {
        let tree = make_tree("hello");
        let commit = Commit::new(
            CHANGE_1,
            &["parent-abc".to_string()],
            tree,
            "add feature".to_string(),
        );

        assert_eq!(commit.change_id(), CHANGE_1);
        assert_eq!(commit.parents(), &["parent-abc".to_string()]);
        assert_eq!(commit.message(), "add feature");
        assert!(!commit.commit_id().is_empty());
    }

    #[test]
    fn test_parent_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::new(CHANGE_1, &[], tree1, String::new());

        let tree2 = make_tree("hello");
        let c2 = Commit::new(CHANGE_1, &["parent".to_string()], tree2, String::new());

        // Different parents should produce different commit IDs
        // (timestamps differ too since they're generated at construction)
        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_tree_accessible() {
        let tree = make_tree("hello");
        let expected_tree_id = tree.id().to_string();
        let commit = Commit::new(CHANGE_1, &[], tree, String::new());

        assert_eq!(commit.tree().id(), expected_tree_id);
    }

    #[test]
    fn test_timestamp_defaults_to_now() {
        let before = SystemTime::now();
        let tree = make_tree("hello");
        let commit = Commit::new(CHANGE_1, &[], tree, String::new());
        let after = SystemTime::now();

        assert!(commit.timestamp() >= before);
        assert!(commit.timestamp() <= after);
    }

    #[test]
    fn test_multiple_parents() {
        let tree = make_tree("hello");
        let parents = vec!["parent-a".to_string(), "parent-b".to_string()];
        let commit = Commit::new(CHANGE_1, &parents, tree, "merge".to_string());

        assert_eq!(commit.parents().len(), 2);
        assert_eq!(commit.parents()[0], "parent-a");
        assert_eq!(commit.parents()[1], "parent-b");
    }
}
