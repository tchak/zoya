use crate::Tree;

/// A content-addressed commit pointing to a tree snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    commit_id: String,
    parent_id: Option<String>,
    change_id: String,
    message: String,
    tree: Tree,
}

impl Commit {
    pub fn builder(change_id: String, tree: Tree) -> CommitBuilder {
        CommitBuilder {
            change_id,
            tree,
            parent_id: None,
            message: String::new(),
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
}

/// Builder for constructing commits.
pub struct CommitBuilder {
    change_id: String,
    tree: Tree,
    parent_id: Option<String>,
    message: String,
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

    pub fn build(self) -> Commit {
        let commit_id = compute_commit_id(
            self.parent_id.as_deref(),
            &self.change_id,
            &self.message,
            self.tree.id(),
        );
        Commit {
            commit_id,
            parent_id: self.parent_id,
            change_id: self.change_id,
            message: self.message,
            tree: self.tree,
        }
    }
}

fn compute_commit_id(
    parent_id: Option<&str>,
    change_id: &str,
    message: &str,
    tree_id: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(parent_id.unwrap_or("").as_bytes());
    hasher.update(change_id.as_bytes());
    hasher.update(message.as_bytes());
    hasher.update(tree_id.as_bytes());
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

    #[test]
    fn test_minimal_commit() {
        let tree = make_tree("hello");
        let commit = Commit::builder("change-1".to_string(), tree).build();

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
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .message("msg".to_string())
            .build();

        assert_eq!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_message_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1)
            .message("msg1".to_string())
            .build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .message("msg2".to_string())
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_parent_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1).build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-1".to_string(), tree2)
            .parent_id("parent".to_string())
            .build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_tree_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1).build();

        let tree2 = make_tree("world");
        let c2 = Commit::builder("change-1".to_string(), tree2).build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_change_id_affects_id() {
        let tree1 = make_tree("hello");
        let c1 = Commit::builder("change-1".to_string(), tree1).build();

        let tree2 = make_tree("hello");
        let c2 = Commit::builder("change-2".to_string(), tree2).build();

        assert_ne!(c1.commit_id(), c2.commit_id());
    }

    #[test]
    fn test_tree_accessible() {
        let tree = make_tree("hello");
        let expected_tree_id = tree.id().to_string();
        let commit = Commit::builder("change-1".to_string(), tree).build();

        assert_eq!(commit.tree().id(), expected_tree_id);
    }
}
