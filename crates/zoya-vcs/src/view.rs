use crate::Commit;

/// The current repository view: working copy and head commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    view_id: String,
    working_copy: Commit,
    heads: Vec<Commit>,
}

impl View {
    pub fn new(view_id: Option<String>, working_copy: Commit, heads: Vec<Commit>) -> Self {
        assert!(!heads.is_empty(), "heads must not be empty");
        let view_id = view_id.unwrap_or_else(|| compute_view_id(&working_copy, &heads));
        View {
            view_id,
            working_copy,
            heads,
        }
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
