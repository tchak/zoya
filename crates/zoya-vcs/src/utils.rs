use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

/// Returns the current time as seconds since UNIX epoch.
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Generates a new change ID as a dashless UUIDv7 hex string (32 chars).
pub fn generate_change_id() -> String {
    Uuid::now_v7().simple().to_string()
}

/// Computes a content-addressed commit ID from its components using blake3.
pub fn compute_commit_id(
    parents: &[String],
    change_id: &str,
    message: &str,
    tree_id: &str,
    timestamp: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for parent in parents {
        hasher.update(parent.as_bytes());
    }
    hasher.update(change_id.as_bytes());
    hasher.update(message.as_bytes());
    hasher.update(tree_id.as_bytes());
    hasher.update(&timestamp.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Computes a view ID from the working copy commit and head commit IDs using blake3.
pub fn compute_view_id(working_copy_commit_id: &str, head_commit_ids: &[&str]) -> String {
    let mut sorted_ids: Vec<&str> = head_commit_ids.to_vec();
    sorted_ids.sort();
    let mut hasher = blake3::Hasher::new();
    hasher.update(working_copy_commit_id.as_bytes());
    for id in &sorted_ids {
        hasher.update(id.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}
