use crate::Commit;

/// The current repository view: working copy and head commits.
#[derive(Debug, Clone)]
pub struct View {
    view_id: String,
    working_copy: Commit,
    heads: Vec<Commit>,
}

impl View {
    pub(crate) fn new(view_id: String, working_copy: Commit, heads: Vec<Commit>) -> Self {
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
