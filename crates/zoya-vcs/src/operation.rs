use std::time::SystemTime;

use crate::Commit;

/// An operation recording a change to the repository state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    operation_id: String,
    operation_type: String,
    view_id: String,
    heads: Vec<Commit>,
    timestamp: SystemTime,
}

impl Operation {
    pub fn new(
        operation_id: String,
        operation_type: String,
        view_id: String,
        heads: Vec<Commit>,
    ) -> Self {
        Operation {
            operation_id,
            operation_type,
            view_id,
            heads,
            timestamp: SystemTime::now(),
        }
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn operation_type(&self) -> &str {
        &self.operation_type
    }

    pub fn view_id(&self) -> &str {
        &self.view_id
    }

    pub fn heads(&self) -> &[Commit] {
        &self.heads
    }

    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} (view: {}, heads: {})",
            self.operation_id,
            self.operation_type,
            self.view_id,
            self.heads.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{Blob, Tree};

    use super::*;

    fn make_commit(content: &str, change_id: &str) -> Commit {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        let tree = Tree::new(blobs);
        Commit::builder(change_id.to_string(), tree).build()
    }

    #[test]
    fn test_construction() {
        let before = SystemTime::now();
        let commit = make_commit("hello", "change-1");
        let op = Operation::new(
            "op-1".to_string(),
            "snapshot".to_string(),
            "view-1".to_string(),
            vec![commit],
        );
        let after = SystemTime::now();

        assert_eq!(op.operation_id(), "op-1");
        assert_eq!(op.operation_type(), "snapshot");
        assert_eq!(op.view_id(), "view-1");
        assert_eq!(op.heads().len(), 1);
        assert!(op.timestamp() >= before);
        assert!(op.timestamp() <= after);
    }

    #[test]
    fn test_empty_heads() {
        let op = Operation::new(
            "op-1".to_string(),
            "init".to_string(),
            "view-1".to_string(),
            vec![],
        );
        assert!(op.heads().is_empty());
    }

    #[test]
    fn test_display() {
        let commit = make_commit("hello", "change-1");
        let op = Operation::new(
            "op-1".to_string(),
            "snapshot".to_string(),
            "view-1".to_string(),
            vec![commit],
        );

        let display = format!("{op}");
        assert_eq!(display, "op-1 snapshot (view: view-1, heads: 1)");
    }

    #[test]
    fn test_multiple_heads() {
        let c1 = make_commit("hello", "change-1");
        let c2 = make_commit("world", "change-2");
        let op = Operation::new(
            "op-1".to_string(),
            "merge".to_string(),
            "view-1".to_string(),
            vec![c1, c2],
        );

        assert_eq!(op.heads().len(), 2);
    }
}
