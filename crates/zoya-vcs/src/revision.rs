use uuid::Uuid;

use crate::Commit;

/// A revision groups one or more commits sharing the same `change_id`.
/// Multiple commits indicate divergence.
#[derive(Debug, Clone)]
pub struct Revision {
    commits: Vec<Commit>,
    is_head: bool,
    is_working_copy: bool,
}

impl Revision {
    pub(crate) fn new(commits: Vec<Commit>, is_head: bool, is_working_copy: bool) -> Self {
        debug_assert!(!commits.is_empty());
        Revision {
            commits,
            is_head,
            is_working_copy,
        }
    }

    pub fn change_id(&self) -> Uuid {
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
