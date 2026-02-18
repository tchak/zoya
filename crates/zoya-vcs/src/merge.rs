use std::ops::Range;

use imara_diff::intern::InternedInput;
use imara_diff::sources::lines_with_terminator;
use imara_diff::{Algorithm, Sink, diff};

use crate::Blob;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub base: String,
    pub ours: String,
    pub theirs: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    pub blob: Blob,
    pub conflicts: Vec<Conflict>,
}

impl MergeResult {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// Three-way merge of optional blobs.
///
/// Always produces a `MergeResult` containing the merged blob and any conflicts.
/// When conflicts exist, diff3-style markers are materialized inline in the blob content.
pub fn three_way_merge(
    base: Option<&Blob>,
    ours: Option<&Blob>,
    theirs: Option<&Blob>,
) -> MergeResult {
    match (base, ours, theirs) {
        (None, None, None) => clean(Blob::new(String::new())),
        (None, Some(o), None) => clean(o.clone()),
        (None, None, Some(t)) => clean(t.clone()),
        (Some(_), None, None) => clean(Blob::new(String::new())),

        (Some(b), Some(o), None) => {
            if o.id() == b.id() {
                clean(Blob::new(String::new()))
            } else {
                let conflict = Conflict {
                    base: b.content().to_string(),
                    ours: o.content().to_string(),
                    theirs: String::new(),
                };
                let content = format_conflict(&conflict);
                MergeResult {
                    blob: Blob::new(content),
                    conflicts: vec![conflict],
                }
            }
        }

        (Some(b), None, Some(t)) => {
            if t.id() == b.id() {
                clean(Blob::new(String::new()))
            } else {
                let conflict = Conflict {
                    base: b.content().to_string(),
                    ours: String::new(),
                    theirs: t.content().to_string(),
                };
                let content = format_conflict(&conflict);
                MergeResult {
                    blob: Blob::new(content),
                    conflicts: vec![conflict],
                }
            }
        }

        (None, Some(o), Some(t)) => {
            if o.id() == t.id() {
                clean(o.clone())
            } else {
                merge_content("", o.content(), t.content())
            }
        }

        (Some(b), Some(o), Some(t)) => {
            if o.id() == b.id() {
                return clean(t.clone());
            }
            if t.id() == b.id() {
                return clean(o.clone());
            }
            if o.id() == t.id() {
                return clean(o.clone());
            }
            merge_content(b.content(), o.content(), t.content())
        }
    }
}

fn clean(blob: Blob) -> MergeResult {
    MergeResult {
        blob,
        conflicts: Vec::new(),
    }
}

fn format_conflict(conflict: &Conflict) -> String {
    let mut s = String::new();
    s.push_str("<<<<<<<\n");
    s.push_str(&conflict.ours);
    if !conflict.ours.is_empty() && !conflict.ours.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("|||||||\n");
    s.push_str(&conflict.base);
    if !conflict.base.is_empty() && !conflict.base.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("=======\n");
    s.push_str(&conflict.theirs);
    if !conflict.theirs.is_empty() && !conflict.theirs.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(">>>>>>>\n");
    s
}

// --- Change computation ---

#[derive(Debug, Clone)]
struct ChangeRange {
    before: Range<u32>,
    after: Range<u32>,
}

struct RangeCollector {
    changes: Vec<ChangeRange>,
}

impl Sink for RangeCollector {
    type Out = Vec<ChangeRange>;

    fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
        self.changes.push(ChangeRange { before, after });
    }

    fn finish(self) -> Vec<ChangeRange> {
        self.changes
    }
}

fn compute_changes(before: &str, after: &str) -> Vec<ChangeRange> {
    let input = InternedInput::new(lines_with_terminator(before), lines_with_terminator(after));
    let collector = RangeCollector {
        changes: Vec::new(),
    };
    diff(Algorithm::Histogram, &input, collector)
}

// --- Core merge algorithm ---

fn merge_content(base: &str, ours: &str, theirs: &str) -> MergeResult {
    let base_lines: Vec<&str> = lines_with_terminator(base).collect();
    let ours_lines: Vec<&str> = lines_with_terminator(ours).collect();
    let theirs_lines: Vec<&str> = lines_with_terminator(theirs).collect();

    let ours_changes = compute_changes(base, ours);
    let theirs_changes = compute_changes(base, theirs);

    let mut result = String::new();
    let mut conflicts = Vec::new();
    let mut base_pos: u32 = 0;
    let mut oi = 0;
    let mut ti = 0;

    while oi < ours_changes.len() || ti < theirs_changes.len() {
        let oc = ours_changes.get(oi);
        let tc = theirs_changes.get(ti);

        match (oc, tc) {
            (Some(o), None) => {
                emit_base_lines(&base_lines, base_pos, o.before.start, &mut result);
                emit_lines(&ours_lines, &o.after, &mut result);
                base_pos = o.before.end;
                oi += 1;
            }
            (None, Some(t)) => {
                emit_base_lines(&base_lines, base_pos, t.before.start, &mut result);
                emit_lines(&theirs_lines, &t.after, &mut result);
                base_pos = t.before.end;
                ti += 1;
            }
            (Some(o), Some(t)) => {
                if changes_overlap(o, t) {
                    // Overlapping — collect the full overlapping extent
                    let (extent_start, extent_end, oi_end, ti_end) =
                        collect_overlapping_extent(&ours_changes, &theirs_changes, oi, ti);

                    emit_base_lines(&base_lines, base_pos, extent_start, &mut result);

                    let ours_text = reconstruct_side(
                        &base_lines,
                        &ours_lines,
                        &ours_changes[oi..oi_end],
                        extent_start,
                        extent_end,
                    );
                    let theirs_text = reconstruct_side(
                        &base_lines,
                        &theirs_lines,
                        &theirs_changes[ti..ti_end],
                        extent_start,
                        extent_end,
                    );

                    if ours_text == theirs_text {
                        result.push_str(&ours_text);
                    } else {
                        let base_text: String =
                            base_lines[extent_start as usize..extent_end as usize].concat();
                        let conflict = Conflict {
                            base: base_text,
                            ours: ours_text,
                            theirs: theirs_text,
                        };
                        result.push_str(&format_conflict(&conflict));
                        conflicts.push(conflict);
                    }

                    base_pos = extent_end;
                    oi = oi_end;
                    ti = ti_end;
                } else if o.before.start <= t.before.start {
                    // Non-overlapping, ours first
                    emit_base_lines(&base_lines, base_pos, o.before.start, &mut result);
                    emit_lines(&ours_lines, &o.after, &mut result);
                    base_pos = o.before.end;
                    oi += 1;
                } else {
                    // Non-overlapping, theirs first
                    emit_base_lines(&base_lines, base_pos, t.before.start, &mut result);
                    emit_lines(&theirs_lines, &t.after, &mut result);
                    base_pos = t.before.end;
                    ti += 1;
                }
            }
            (None, None) => unreachable!(),
        }
    }

    // Emit remaining base lines
    emit_base_lines(&base_lines, base_pos, base_lines.len() as u32, &mut result);

    MergeResult {
        blob: Blob::new(result),
        conflicts,
    }
}

fn emit_base_lines(base_lines: &[&str], from: u32, to: u32, result: &mut String) {
    for line in &base_lines[from as usize..to as usize] {
        result.push_str(line);
    }
}

fn emit_lines(lines: &[&str], range: &Range<u32>, result: &mut String) {
    for line in &lines[range.start as usize..range.end as usize] {
        result.push_str(line);
    }
}

/// Check whether two changes overlap in the base.
/// Standard range overlap, plus the special case of both being pure insertions
/// at the same position (empty before ranges at the same point).
fn changes_overlap(a: &ChangeRange, b: &ChangeRange) -> bool {
    if a.before.start < b.before.end && b.before.start < a.before.end {
        return true;
    }
    // Both pure insertions at the same position
    a.before.is_empty() && b.before.is_empty() && a.before.start == b.before.start
}

/// Collect the full extent of transitively overlapping changes from both sides.
/// Returns `(extent_start, extent_end, oi_end, ti_end)`.
fn collect_overlapping_extent(
    ours_changes: &[ChangeRange],
    theirs_changes: &[ChangeRange],
    oi_start: usize,
    ti_start: usize,
) -> (u32, u32, usize, usize) {
    let mut extent_end = ours_changes[oi_start]
        .before
        .end
        .max(theirs_changes[ti_start].before.end);
    let extent_start = ours_changes[oi_start]
        .before
        .start
        .min(theirs_changes[ti_start].before.start);
    let mut oi = oi_start + 1;
    let mut ti = ti_start + 1;

    loop {
        let mut extended = false;

        while oi < ours_changes.len() && ours_changes[oi].before.start < extent_end {
            extent_end = extent_end.max(ours_changes[oi].before.end);
            oi += 1;
            extended = true;
        }

        while ti < theirs_changes.len() && theirs_changes[ti].before.start < extent_end {
            extent_end = extent_end.max(theirs_changes[ti].before.end);
            ti += 1;
            extended = true;
        }

        if !extended {
            break;
        }
    }

    (extent_start, extent_end, oi, ti)
}

/// Reconstruct one side's text over a base extent by replaying its changes.
fn reconstruct_side(
    base_lines: &[&str],
    side_lines: &[&str],
    changes: &[ChangeRange],
    extent_start: u32,
    extent_end: u32,
) -> String {
    let mut text = String::new();
    let mut pos = extent_start;

    for change in changes {
        // Emit unchanged base lines before this change
        for line in &base_lines[pos as usize..change.before.start as usize] {
            text.push_str(line);
        }
        // Emit the changed lines from this side
        for line in &side_lines[change.after.start as usize..change.after.end as usize] {
            text.push_str(line);
        }
        pos = change.before.end;
    }

    // Emit remaining base lines in the extent
    for line in &base_lines[pos as usize..extent_end as usize] {
        text.push_str(line);
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(content: &str) -> Blob {
        Blob::new(content.to_string())
    }

    // --- Option combination tests ---

    #[test]
    fn all_none() {
        let result = three_way_merge(None, None, None);
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "");
    }

    #[test]
    fn only_ours() {
        let o = blob("hello\n");
        let result = three_way_merge(None, Some(&o), None);
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "hello\n");
    }

    #[test]
    fn only_theirs() {
        let t = blob("hello\n");
        let result = three_way_merge(None, None, Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "hello\n");
    }

    #[test]
    fn both_deleted() {
        let b = blob("hello\n");
        let result = three_way_merge(Some(&b), None, None);
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "");
    }

    #[test]
    fn ours_unchanged_theirs_deleted() {
        let b = blob("hello\n");
        let o = blob("hello\n");
        let result = three_way_merge(Some(&b), Some(&o), None);
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "");
    }

    #[test]
    fn ours_modified_theirs_deleted() {
        let b = blob("hello\n");
        let o = blob("modified\n");
        let result = three_way_merge(Some(&b), Some(&o), None);
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].ours, "modified\n");
        assert_eq!(result.conflicts[0].base, "hello\n");
        assert_eq!(result.conflicts[0].theirs, "");
    }

    #[test]
    fn theirs_unchanged_ours_deleted() {
        let b = blob("hello\n");
        let t = blob("hello\n");
        let result = three_way_merge(Some(&b), None, Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "");
    }

    #[test]
    fn theirs_modified_ours_deleted() {
        let b = blob("hello\n");
        let t = blob("modified\n");
        let result = three_way_merge(Some(&b), None, Some(&t));
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].ours, "");
        assert_eq!(result.conflicts[0].base, "hello\n");
        assert_eq!(result.conflicts[0].theirs, "modified\n");
    }

    #[test]
    fn add_add_same() {
        let o = blob("hello\n");
        let t = blob("hello\n");
        let result = three_way_merge(None, Some(&o), Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "hello\n");
    }

    #[test]
    fn add_add_different() {
        let o = blob("ours\n");
        let t = blob("theirs\n");
        let result = three_way_merge(None, Some(&o), Some(&t));
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].ours, "ours\n");
        assert_eq!(result.conflicts[0].theirs, "theirs\n");
    }

    // --- Early return tests ---

    #[test]
    fn ours_equals_base() {
        let b = blob("original\n");
        let o = blob("original\n");
        let t = blob("theirs\n");
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "theirs\n");
    }

    #[test]
    fn theirs_equals_base() {
        let b = blob("original\n");
        let o = blob("ours\n");
        let t = blob("original\n");
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "ours\n");
    }

    #[test]
    fn ours_equals_theirs() {
        let b = blob("original\n");
        let o = blob("same\n");
        let t = blob("same\n");
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "same\n");
    }

    // --- Clean merge tests ---

    #[test]
    fn non_overlapping_changes() {
        let b = blob("a\nb\nc\nd\n");
        let o = blob("x\nb\nc\nd\n"); // changed line 1
        let t = blob("a\nb\nc\ny\n"); // changed line 4
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "x\nb\nc\ny\n");
    }

    #[test]
    fn same_overlapping_change() {
        let b = blob("a\nb\nc\n");
        let o = blob("a\nx\nc\n"); // changed line 2 to x
        let t = blob("a\nx\nc\n"); // changed line 2 to x (same)
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(result.is_clean());
        assert_eq!(result.blob.content(), "a\nx\nc\n");
    }

    // --- Conflict tests ---

    #[test]
    fn overlapping_different_changes() {
        let b = blob("a\nb\nc\n");
        let o = blob("a\nx\nc\n"); // changed line 2 to x
        let t = blob("a\ny\nc\n"); // changed line 2 to y
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].base, "b\n");
        assert_eq!(result.conflicts[0].ours, "x\n");
        assert_eq!(result.conflicts[0].theirs, "y\n");
    }

    #[test]
    fn multiple_conflicts() {
        let b = blob("a\nb\nc\nd\ne\n");
        let o = blob("x\nb\nc\nd\nw\n"); // changed lines 1 and 5
        let t = blob("y\nb\nc\nd\nz\n"); // changed lines 1 and 5 differently
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 2);
        assert_eq!(result.conflicts[0].base, "a\n");
        assert_eq!(result.conflicts[0].ours, "x\n");
        assert_eq!(result.conflicts[0].theirs, "y\n");
        assert_eq!(result.conflicts[1].base, "e\n");
        assert_eq!(result.conflicts[1].ours, "w\n");
        assert_eq!(result.conflicts[1].theirs, "z\n");
    }

    #[test]
    fn conflict_marker_format() {
        let b = blob("a\nb\nc\n");
        let o = blob("a\nx\nc\n");
        let t = blob("a\ny\nc\n");
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        let content = result.blob.content();
        assert!(content.contains("<<<<<<<\n"));
        assert!(content.contains("|||||||\n"));
        assert!(content.contains("=======\n"));
        assert!(content.contains(">>>>>>>\n"));
        // Verify the full structure: a\n + conflict block + c\n
        let expected = "a\n<<<<<<<\nx\n|||||||\nb\n=======\ny\n>>>>>>>\nc\n";
        assert_eq!(content, expected);
    }

    // --- Edge case tests ---

    #[test]
    fn all_same_content() {
        let b = blob("hello\n");
        let o = blob("hello\n");
        let t = blob("hello\n");
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(result.is_clean());
        // ours == base → returns theirs (which has same content)
        assert_eq!(result.blob.content(), "hello\n");
    }

    #[test]
    fn no_trailing_newline() {
        let b = blob("a\nb\nc");
        let o = blob("a\nx\nc");
        let t = blob("a\ny\nc");
        let result = three_way_merge(Some(&b), Some(&o), Some(&t));
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].base, "b\n");
        assert_eq!(result.conflicts[0].ours, "x\n");
        assert_eq!(result.conflicts[0].theirs, "y\n");
    }
}
