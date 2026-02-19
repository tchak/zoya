use std::ops::Range;

use imara_diff::sources::lines_with_terminator;
use imara_diff::{Algorithm, Sink, diff, intern::InternedInput};

use zoya_package::QualifiedPath;

use crate::Blob;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffHunk {
    Matching(String),
    Different { before: String, after: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added {
        path: QualifiedPath,
        blob: Blob,
    },
    Removed {
        path: QualifiedPath,
        blob: Blob,
    },
    Updated {
        path: QualifiedPath,
        old: Blob,
        new: Blob,
        diff: Vec<DiffHunk>,
    },
    Renamed {
        old_path: QualifiedPath,
        new_path: QualifiedPath,
        blob: Blob,
    },
}

/// Compute line-level diff hunks between two strings.
pub fn compute_diff(before: &str, after: &str) -> Vec<DiffHunk> {
    let before_src = lines_with_terminator(before);
    let after_src = lines_with_terminator(after);

    // Lines is Copy — collecting consumes a copy, leaving the original intact
    let before_lines: Vec<&str> = before_src.collect();
    let after_lines: Vec<&str> = after_src.collect();

    let input = InternedInput::new(before_src, after_src);

    let collector = HunkCollector {
        before_lines: &before_lines,
        after_lines: &after_lines,
        hunks: Vec::new(),
        before_pos: 0,
        after_pos: 0,
    };

    diff(Algorithm::Histogram, &input, collector)
}

struct HunkCollector<'a> {
    before_lines: &'a [&'a str],
    after_lines: &'a [&'a str],
    hunks: Vec<DiffHunk>,
    before_pos: u32,
    after_pos: u32,
}

impl Sink for HunkCollector<'_> {
    type Out = Vec<DiffHunk>;

    fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
        // Emit matching lines before this change
        if self.before_pos < before.start {
            let text: String =
                self.before_lines[self.before_pos as usize..before.start as usize].concat();
            self.hunks.push(DiffHunk::Matching(text));
        }

        let before_text: String =
            self.before_lines[before.start as usize..before.end as usize].concat();
        let after_text: String =
            self.after_lines[after.start as usize..after.end as usize].concat();

        self.hunks.push(DiffHunk::Different {
            before: before_text,
            after: after_text,
        });

        self.before_pos = before.end;
        self.after_pos = after.end;
    }

    fn finish(mut self) -> Vec<DiffHunk> {
        // Emit any trailing matching lines
        if (self.before_pos as usize) < self.before_lines.len() {
            let text: String = self.before_lines[self.before_pos as usize..].concat();
            self.hunks.push(DiffHunk::Matching(text));
        }
        self.hunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical() {
        let hunks = compute_diff("hello\nworld\n", "hello\nworld\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0], DiffHunk::Matching("hello\nworld\n".to_string()));
    }

    #[test]
    fn test_completely_different() {
        let hunks = compute_diff("aaa\n", "bbb\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0],
            DiffHunk::Different {
                before: "aaa\n".to_string(),
                after: "bbb\n".to_string(),
            }
        );
    }

    #[test]
    fn test_added_line() {
        let hunks = compute_diff("hello\n", "hello\nworld\n");
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0], DiffHunk::Matching("hello\n".to_string()));
        assert_eq!(
            hunks[1],
            DiffHunk::Different {
                before: String::new(),
                after: "world\n".to_string(),
            }
        );
    }

    #[test]
    fn test_removed_line() {
        let hunks = compute_diff("hello\nworld\n", "hello\n");
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0], DiffHunk::Matching("hello\n".to_string()));
        assert_eq!(
            hunks[1],
            DiffHunk::Different {
                before: "world\n".to_string(),
                after: String::new(),
            }
        );
    }

    #[test]
    fn test_empty_inputs() {
        let hunks = compute_diff("", "");
        assert!(hunks.is_empty());
    }

    #[test]
    fn test_empty_to_content() {
        let hunks = compute_diff("", "hello\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0],
            DiffHunk::Different {
                before: String::new(),
                after: "hello\n".to_string(),
            }
        );
    }

    #[test]
    fn test_content_to_empty() {
        let hunks = compute_diff("hello\n", "");
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0],
            DiffHunk::Different {
                before: "hello\n".to_string(),
                after: String::new(),
            }
        );
    }

    #[test]
    fn test_middle_change() {
        let hunks = compute_diff("a\nb\nc\n", "a\nx\nc\n");
        assert_eq!(hunks.len(), 3);
        assert_eq!(hunks[0], DiffHunk::Matching("a\n".to_string()));
        assert_eq!(
            hunks[1],
            DiffHunk::Different {
                before: "b\n".to_string(),
                after: "x\n".to_string(),
            }
        );
        assert_eq!(hunks[2], DiffHunk::Matching("c\n".to_string()));
    }
}
