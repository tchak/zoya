use std::hash::{Hash, Hasher};

use crate::utils;

/// A content-addressed commit pointing to a tree snapshot.
#[derive(Debug, Clone)]
pub struct Commit {
    commit_id: String,
    parents: Vec<String>,
    change_id: String,
    message: String,
    tree_id: String,
    timestamp: u64,
}

impl Commit {
    pub fn new(change_id: String, parents: &[String], tree_id: String, message: String) -> Self {
        let timestamp = utils::timestamp();
        let commit_id =
            utils::compute_commit_id(parents, &change_id, &message, &tree_id, timestamp);
        Commit {
            commit_id,
            parents: parents.to_vec(),
            change_id,
            message,
            tree_id,
            timestamp,
        }
    }

    pub(crate) fn restore(
        commit_id: String,
        change_id: String,
        parents: Vec<String>,
        tree_id: String,
        message: String,
        timestamp: u64,
    ) -> Self {
        Commit {
            commit_id,
            change_id,
            parents,
            tree_id,
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

    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn tree_id(&self) -> &str {
        &self.tree_id
    }

    pub fn timestamp(&self) -> u64 {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zoya_package::QualifiedPath;

    use crate::{Blob, Tree};

    use super::*;

    fn make_tree_id(content: &str) -> String {
        let mut blobs = HashMap::new();
        blobs.insert(QualifiedPath::root(), Blob::new(content.to_string()));
        Tree::new(blobs).id().to_string()
    }

    const CHANGE_1: &str = "01020304050607080910111213141516";

    #[test]
    fn test_minimal_commit() {
        let tree_id = make_tree_id("hello");
        let commit = Commit::new(CHANGE_1.to_string(), &[], tree_id, String::new());

        assert!(!commit.commit_id().is_empty());
        assert!(commit.parents().is_empty());
        assert_eq!(commit.change_id(), CHANGE_1);
        assert_eq!(commit.message(), "");
    }

    #[test]
    fn test_commit_with_message() {
        let tree_id = make_tree_id("hello");
        let commit = Commit::new(
            CHANGE_1.to_string(),
            &[],
            tree_id,
            "initial commit".to_string(),
        );

        assert_eq!(commit.message(), "initial commit");
    }

    #[test]
    fn test_commit_with_parent() {
        let tree_id = make_tree_id("hello");
        let commit = Commit::new(
            CHANGE_1.to_string(),
            &["parent-abc".to_string()],
            tree_id,
            String::new(),
        );

        assert_eq!(commit.parents(), &["parent-abc".to_string()]);
    }

    #[test]
    fn test_commit_with_all_fields() {
        let tree_id = make_tree_id("hello");
        let commit = Commit::new(
            CHANGE_1.to_string(),
            &["parent-abc".to_string()],
            tree_id,
            "add feature".to_string(),
        );

        assert_eq!(commit.change_id(), CHANGE_1);
        assert_eq!(commit.parents(), &["parent-abc".to_string()]);
        assert_eq!(commit.message(), "add feature");
        assert!(!commit.commit_id().is_empty());
    }

    #[test]
    fn test_parent_affects_id() {
        let tree_id1 = make_tree_id("hello");
        let c1 = Commit::new(CHANGE_1.to_string(), &[], tree_id1, String::new());

        let tree_id2 = make_tree_id("hello");
        let c2 = Commit::new(
            CHANGE_1.to_string(),
            &["parent".to_string()],
            tree_id2,
            String::new(),
        );

        // Different parents should produce different commit IDs
        // (timestamps differ too since they're generated at construction)
        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_tree_id_accessible() {
        let tree_id = make_tree_id("hello");
        let expected_tree_id = tree_id.clone();
        let commit = Commit::new(CHANGE_1.to_string(), &[], tree_id, String::new());

        assert_eq!(commit.tree_id(), expected_tree_id);
    }

    #[test]
    fn test_timestamp_defaults_to_now() {
        let before = utils::timestamp();
        let tree_id = make_tree_id("hello");
        let commit = Commit::new(CHANGE_1.to_string(), &[], tree_id, String::new());
        let after = utils::timestamp();

        assert!(commit.timestamp() >= before);
        assert!(commit.timestamp() <= after);
    }

    #[test]
    fn test_multiple_parents() {
        let tree_id = make_tree_id("hello");
        let parents = vec!["parent-a".to_string(), "parent-b".to_string()];
        let commit = Commit::new(CHANGE_1.to_string(), &parents, tree_id, "merge".to_string());

        assert_eq!(commit.parents().len(), 2);
        assert_eq!(commit.parents()[0], "parent-a");
        assert_eq!(commit.parents()[1], "parent-b");
    }
}
