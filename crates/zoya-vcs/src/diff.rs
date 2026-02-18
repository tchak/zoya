use crate::Blob;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added {
        path: String,
        blob: Blob,
    },
    Removed {
        path: String,
        blob: Blob,
    },
    Updated {
        path: String,
        old: Blob,
        new: Blob,
        diff: String,
    },
    Renamed {
        old_path: String,
        new_path: String,
        blob: Blob,
    },
}
