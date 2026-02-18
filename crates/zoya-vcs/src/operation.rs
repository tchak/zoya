use std::collections::HashSet;
use std::time::SystemTime;

use uuid::Uuid;

use crate::Commit;

/// An operation recording a change to the repository state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    operation_id: Uuid,
    operation_type: String,
    view_id: String,
    working_copy: Commit,
    heads: Vec<Commit>,
    timestamp: SystemTime,
}

impl Operation {
    pub fn new(
        operation_id: Uuid,
        operation_type: String,
        working_copy: Commit,
        heads: Vec<Commit>,
    ) -> Self {
        let mut seen = HashSet::new();
        seen.insert(working_copy.clone());
        let mut deduped = vec![working_copy.clone()];
        for head in heads {
            if seen.insert(head.clone()) {
                deduped.push(head);
            }
        }
        let view_id = compute_view_id(&working_copy, &deduped);
        Operation {
            operation_id,
            operation_type,
            view_id,
            working_copy,
            heads: deduped,
            timestamp: SystemTime::now(),
        }
    }

    pub fn operation_id(&self) -> Uuid {
        self.operation_id
    }

    pub fn operation_type(&self) -> &str {
        &self.operation_type
    }

    pub fn view_id(&self) -> &str {
        &self.view_id
    }

    pub fn working_copy(&self) -> &Commit {
        &self.working_copy
    }

    pub fn heads(&self) -> &[Commit] {
        &self.heads
    }

    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

fn compute_view_id(working_copy: &Commit, heads: &[Commit]) -> String {
    let mut commit_ids: Vec<&str> = heads.iter().map(|c| c.commit_id()).collect();
    commit_ids.sort();
    let mut hasher = blake3::Hasher::new();
    hasher.update(working_copy.commit_id().as_bytes());
    for id in &commit_ids {
        hasher.update(id.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
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

    const OP_1: Uuid = Uuid::from_bytes([
        0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf,
        0xb0,
    ]);

    const CHANGE_1: Uuid = Uuid::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ]);
    const CHANGE_2: Uuid = Uuid::from_bytes([
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        0x20,
    ]);

    fn make_commit(content: &str, change_id: Uuid) -> Commit {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        let tree = Tree::new(blobs);
        Commit::new(change_id, &[], tree, String::new())
    }

    #[test]
    fn test_construction() {
        let before = SystemTime::now();
        let commit = make_commit("hello", CHANGE_1);
        let op = Operation::new(OP_1, "snapshot".to_string(), commit.clone(), vec![commit]);
        let after = SystemTime::now();

        assert_eq!(op.operation_id(), OP_1);
        assert_eq!(op.operation_type(), "snapshot");
        assert!(!op.view_id().is_empty());
        assert_eq!(op.heads().len(), 1);
        assert!(op.timestamp() >= before);
        assert!(op.timestamp() <= after);
    }

    #[test]
    fn test_empty_heads_includes_working_copy() {
        let wc = make_commit("wc", CHANGE_1);
        let op = Operation::new(OP_1, "init".to_string(), wc.clone(), vec![]);
        assert_eq!(op.heads().len(), 1);
        assert_eq!(op.heads()[0], wc);
        assert!(!op.view_id().is_empty());
    }

    #[test]
    fn test_display() {
        let commit = make_commit("hello", CHANGE_1);
        let op = Operation::new(
            OP_1,
            "snapshot".to_string(),
            commit.clone(),
            vec![commit.clone()],
        );

        let display = format!("{op}");
        let expected_view_id = op.view_id().to_string();
        assert_eq!(
            display,
            format!("{OP_1} snapshot (view: {expected_view_id}, heads: 1)")
        );
    }

    #[test]
    fn test_multiple_heads() {
        let c1 = make_commit("hello", CHANGE_1);
        let c2 = make_commit("world", CHANGE_2);
        let op = Operation::new(OP_1, "merge".to_string(), c1.clone(), vec![c2]);

        assert_eq!(op.heads().len(), 2);
    }

    #[test]
    fn test_view_id_deterministic() {
        let c1 = make_commit("hello", CHANGE_1);
        let c2 = make_commit("world", CHANGE_2);

        let op_a = Operation::new(
            OP_1,
            "snapshot".to_string(),
            c1.clone(),
            vec![c1.clone(), c2.clone()],
        );
        let op_b = Operation::new(OP_1, "snapshot".to_string(), c1.clone(), vec![c2, c1]);

        assert_eq!(op_a.view_id(), op_b.view_id());
    }

    #[test]
    fn test_view_id_differs_with_different_heads() {
        let c1 = make_commit("hello", CHANGE_1);
        let c2 = make_commit("world", CHANGE_2);

        let op_a = Operation::new(OP_1, "snapshot".to_string(), c1.clone(), vec![c1]);
        let op_b = Operation::new(OP_1, "snapshot".to_string(), c2.clone(), vec![c2]);

        assert_ne!(op_a.view_id(), op_b.view_id());
    }

    #[test]
    fn test_working_copy_accessor() {
        let wc = make_commit("working", CHANGE_1);
        let head = make_commit("head", CHANGE_2);
        let op = Operation::new(OP_1, "snapshot".to_string(), wc.clone(), vec![head]);

        assert_eq!(op.working_copy(), &wc);
    }

    #[test]
    fn test_working_copy_affects_view_id() {
        let c1 = make_commit("hello", CHANGE_1);
        let c2 = make_commit("world", CHANGE_2);

        let op_a = Operation::new(
            OP_1,
            "snapshot".to_string(),
            c1.clone(),
            vec![c1.clone(), c2.clone()],
        );
        let op_b = Operation::new(OP_1, "snapshot".to_string(), c2.clone(), vec![c1, c2]);

        assert_ne!(op_a.view_id(), op_b.view_id());
    }
}
