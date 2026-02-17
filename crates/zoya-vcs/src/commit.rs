use std::time::{SystemTime, UNIX_EPOCH};

use crate::Tree;

/// A content-addressed commit pointing to a tree snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    commit_id: String,
    parent_id: Option<String>,
    change_id: String,
    message: String,
    tree: Tree,
    timestamp: SystemTime,
}

impl Commit {
    pub fn builder(change_id: String, tree: Tree) -> CommitBuilder {
        CommitBuilder {
            change_id,
            tree,
            parent_id: None,
            message: String::new(),
            timestamp: SystemTime::now(),
        }
    }

    pub fn commit_id(&self) -> &str {
        &self.commit_id
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    pub fn change_id(&self) -> &str {
        &self.change_id
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

/// Builder for constructing commits.
pub struct CommitBuilder {
    change_id: String,
    tree: Tree,
    parent_id: Option<String>,
    message: String,
    timestamp: SystemTime,
}

impl CommitBuilder {
    pub fn parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    pub fn message(mut self, message: String) -> Self {
        self.message = message;
        self
    }

    pub fn timestamp(mut self, timestamp: SystemTime) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn build(self) -> Commit {
        let commit_id = compute_commit_id(
            self.parent_id.as_deref(),
            &self.change_id,
            &self.message,
            self.tree.id(),
            self.timestamp,
        );
        Commit {
            commit_id,
            parent_id: self.parent_id,
            change_id: self.change_id,
            message: self.message,
            tree: self.tree,
            timestamp: self.timestamp,
        }
    }
}

fn compute_commit_id(
    parent_id: Option<&str>,
    change_id: &str,
    message: &str,
    tree_id: &str,
    timestamp: SystemTime,
) -> String {
    let duration = timestamp.duration_since(UNIX_EPOCH).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    hasher.update(parent_id.unwrap_or("").as_bytes());
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
    use std::time::Duration;

    use crate::Blob;

    use super::*;

    const FIXED_TIMESTAMP: SystemTime = UNIX_EPOCH;

    fn make_tree(content: &str) -> Tree {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        Tree::new(blobs)
    }

    #[test]
    fn test_minimal_commit() {
        let tree = make_tree("hello");
        let commit = Commit::builder("change-1".to_string(), tree)
            .timestamp(FIXED_TIMESTAMP)
            .build();

        assert!(!commit.commit_id().is_empty());
        assert!(commit.parent_id().is_none());
        assert_eq!(commit.change_id(), "change-1");
        assert_eq!(commit.message(), "");
    }

    #[test]
    fn test_commit_with_message() {
        let tree = make_tree("hello");
        let commit = Commit::builder("change-1".to_string(), tree)
            .message("initial commit".to_string())
            .build();

        assert_eq!(commit.message(), "initial commit");
    }

    #[test]
    fn test_commit_with_parent() {
        let tree = make_tree("hello");
        let commit = Commit::builder("change-1".to_string(), tree)
            .parent_id("parent-abc".to_string())
            .build();

        assert_eq!(commit.parent_id(), Some("parent-abc"));
    }

    #[test]
    fn test_commit_with_all_fields() {
        let tree = make_tree("hello");
        let commit = Commit::builder("change-1".to_string(), tree)
            .parent_id("parent-abc".to_string())
            .message("add feature".to_string())
            .build();

        assert_eq!(commit.change_id(), "change-1");
        assert_eq!(commit.parent_id(), Some("parent-abc"));
        assert_eq!(commit.message(), "add feature");
        assert!(!commit.commit_id().is_empty());
    }

    #[test]
    fn test_deterministic_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .message("msg".to_string())
            .timestamp(FIXED_TIMESTAMP)
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .message("msg".to_string())
            .timestamp(FIXED_TIMESTAMP)
            .build();

        assert_eq!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_message_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .message("msg1".to_string())
            .timestamp(FIXED_TIMESTAMP)
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .message("msg2".to_string())
            .timestamp(FIXED_TIMESTAMP)
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_parent_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .timestamp(FIXED_TIMESTAMP)
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .parent_id("parent".to_string())
            .timestamp(FIXED_TIMESTAMP)
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_tree_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .timestamp(FIXED_TIMESTAMP)
            .build();

        let tree2 = make_tree("world");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .timestamp(FIXED_TIMESTAMP)
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_change_id_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .timestamp(FIXED_TIMESTAMP)
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-2".to_string(), tree2)
            .timestamp(FIXED_TIMESTAMP)
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_tree_accessible() {
        let tree = make_tree("hello");
        let expected_tree_id = tree.id().to_string();
        let commit = Commit::builder("change-1".to_string(), tree).build();

        assert_eq!(commit.tree().id(), expected_tree_id);
    }

    #[test]
    fn test_timestamp_affects_id() {
        let ts1 = UNIX_EPOCH + Duration::from_secs(1000);
        let ts2 = UNIX_EPOCH + Duration::from_secs(2000);

        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .timestamp(ts1)
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .timestamp(ts2)
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_timestamp_defaults_to_now() {
        let before = SystemTime::now();
        let tree = make_tree("hello");
        let commit = Commit::builder("change-1".to_string(), tree).build();
        let after = SystemTime::now();

        assert!(commit.timestamp() >= before);
        assert!(commit.timestamp() <= after);
    }
}
