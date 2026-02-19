use crate::Commit;
use crate::view::View;

/// A revision groups one or more commits sharing the same `change_id`.
/// Multiple commits indicate divergence.
#[derive(Debug, Clone)]
pub struct Revision {
    commits: Vec<Commit>,
    is_head: bool,
    is_working_copy: bool,
}

impl Revision {
    pub(crate) fn new(mut commits: Vec<Commit>, view: &View) -> Self {
        debug_assert!(!commits.is_empty());
        let is_head = commits
            .iter()
            .any(|c| view.heads().iter().any(|h| h.commit_id() == c.commit_id()));
        let is_working_copy = commits
            .iter()
            .any(|c| c.commit_id() == view.working_copy().commit_id());
        commits.sort_by_key(|c| !view.heads().iter().any(|h| h.commit_id() == c.commit_id()));
        Revision {
            commits,
            is_head,
            is_working_copy,
        }
    }

    pub fn change_id(&self) -> &str {
        self.commits[0].change_id()
    }

    pub fn commit(&self) -> &Commit {
        &self.commits[0]
    }

    pub fn is_divergent(&self) -> bool {
        self.commits.len() > 1
    }

    pub fn is_head(&self) -> bool {
        self.is_head
    }

    pub fn is_working_copy(&self) -> bool {
        self.is_working_copy
    }

    pub fn commits(&self) -> &[Commit] {
        &self.commits
    }
}
