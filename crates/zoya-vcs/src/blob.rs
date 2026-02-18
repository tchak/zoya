use crate::diff::{DiffHunk, compute_diff};
use crate::merge::{self, MergeResult};

/// A content-addressed blob storing source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    id: String,
    content: String,
    size: usize,
}

impl Blob {
    pub fn new(content: String) -> Self {
        let id = blake3::hash(content.as_bytes()).to_hex().to_string();
        let size = content.len();
        Blob { id, content, size }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn diff(&self, other: &Blob) -> Vec<DiffHunk> {
        compute_diff(self.content(), other.content())
    }

    pub fn three_way_merge(
        base: Option<&Blob>,
        ours: Option<&Blob>,
        theirs: Option<&Blob>,
    ) -> MergeResult {
        merge::three_way_merge(base, ours, theirs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_id() {
        let a = Blob::new("hello".to_string());
        let b = Blob::new("hello".to_string());
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn test_different_content_different_id() {
        let a = Blob::new("hello".to_string());
        let b = Blob::new("world".to_string());
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn test_empty_content() {
        let blob = Blob::new(String::new());
        assert_eq!(blob.size(), 0);
        assert_eq!(blob.content(), "");
        assert!(!blob.id().is_empty());
    }

    #[test]
    fn test_hex_format() {
        let blob = Blob::new("test".to_string());
        assert_eq!(blob.id().len(), 64);
        assert!(blob.id().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_diff_identical() {
        let a = Blob::new("hello\nworld\n".to_string());
        let b = Blob::new("hello\nworld\n".to_string());
        let hunks = a.diff(&b);
        assert_eq!(hunks.len(), 1);
        assert!(matches!(&hunks[0], DiffHunk::Matching(_)));
    }

    #[test]
    fn test_diff_added_lines() {
        let a = Blob::new("hello\n".to_string());
        let b = Blob::new("hello\nworld\n".to_string());
        let hunks = a.diff(&b);
        let has_insert = hunks
            .iter()
            .any(|h| matches!(h, DiffHunk::Different { after, .. } if after.contains("world")));
        assert!(has_insert);
    }

    #[test]
    fn test_diff_removed_lines() {
        let a = Blob::new("hello\nworld\n".to_string());
        let b = Blob::new("hello\n".to_string());
        let hunks = a.diff(&b);
        let has_delete = hunks
            .iter()
            .any(|h| matches!(h, DiffHunk::Different { before, .. } if before.contains("world")));
        assert!(has_delete);
    }

    #[test]
    fn test_size_matches_content() {
        let content = "hello world".to_string();
        let blob = Blob::new(content.clone());
        assert_eq!(blob.size(), content.len());
        assert_eq!(blob.content(), content);
    }
}
