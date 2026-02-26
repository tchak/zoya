use std::time::SystemTime;

use sqlx::SqlitePool;
use zoya_package::QualifiedPath;
use zoya_value::Value;

/// Errors that can occur during KV operations.
#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// A stored key-value entry.
pub struct Entry {
    pub value: Value,
    pub versionstamp: String,
}

/// Key-value repository trait with async methods (RPITIT, edition 2024).
pub trait KvRepository {
    /// Create the `kv` table if it doesn't exist.
    fn init(&self) -> impl Future<Output = Result<(), KvError>> + Send;

    /// Fetch a single value by exact path.
    fn get(
        &self,
        path: &QualifiedPath,
    ) -> impl Future<Output = Result<Option<Entry>, KvError>> + Send;

    /// Upsert a value at the given path.
    fn set(
        &self,
        path: &QualifiedPath,
        value: &Value,
    ) -> impl Future<Output = Result<Entry, KvError>> + Send;

    /// Delete a value by exact path.
    fn delete(&self, path: &QualifiedPath) -> impl Future<Output = Result<(), KvError>> + Send;

    /// List all entries whose path starts with `{prefix}::`, ordered by path.
    /// Excludes the exact prefix match — only returns children.
    fn list(
        &self,
        prefix: &QualifiedPath,
    ) -> impl Future<Output = Result<Vec<Entry>, KvError>> + Send;
}

/// Encode a `QualifiedPath` into a binary key that preserves lexicographic ordering.
///
/// Each segment is encoded as: `0x02 + utf8_bytes + 0x00`.
/// Segments are concatenated directly. This follows Deno.KV's encoding format.
fn key_hash(path: &QualifiedPath) -> Vec<u8> {
    let mut result = Vec::new();
    for segment in path.segments() {
        result.push(0x02);
        result.extend_from_slice(segment.as_bytes());
        result.push(0x00);
    }
    result
}

/// Increment the last non-0xFF byte of a key, truncating trailing 0xFF bytes.
/// Used to compute the exclusive upper bound for range queries.
fn strinc(key: &[u8]) -> Vec<u8> {
    let mut result = key.to_vec();
    // Remove trailing 0xFF bytes
    while result.last() == Some(&0xFF) {
        result.pop();
    }
    // Increment the last byte
    if let Some(last) = result.last_mut() {
        *last += 1;
    }
    result
}

/// Generate a versionstamp from the current system time (epoch nanos as string).
fn versionstamp() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_nanos()
        .to_string()
}

/// SQLite-backed KV repository.
pub struct SqliteKvRepository(SqlitePool);

impl SqliteKvRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self(pool)
    }
}

impl KvRepository for SqliteKvRepository {
    async fn init(&self) -> Result<(), KvError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS kv (
                key_hash BLOB PRIMARY KEY NOT NULL,
                value BLOB NOT NULL,
                versionstamp TEXT NOT NULL
            )",
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn get(&self, path: &QualifiedPath) -> Result<Option<Entry>, KvError> {
        let key = key_hash(path);
        let row: Option<(Vec<u8>, String)> =
            sqlx::query_as("SELECT value, versionstamp FROM kv WHERE key_hash = ?")
                .bind(&key)
                .fetch_optional(&self.0)
                .await?;
        match row {
            Some((blob, vs)) => {
                let value: Value = serde_json::from_slice(&blob)?;
                Ok(Some(Entry {
                    value,
                    versionstamp: vs,
                }))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, path: &QualifiedPath, value: &Value) -> Result<Entry, KvError> {
        let key = key_hash(path);
        let blob = serde_json::to_vec(value)?;
        let vs = versionstamp();
        sqlx::query(
            "INSERT INTO kv (key_hash, value, versionstamp) VALUES (?, ?, ?)
             ON CONFLICT(key_hash) DO UPDATE SET value = excluded.value, versionstamp = excluded.versionstamp",
        )
        .bind(&key)
        .bind(&blob)
        .bind(&vs)
        .execute(&self.0)
        .await?;
        Ok(Entry {
            value: value.clone(),
            versionstamp: vs,
        })
    }

    async fn delete(&self, path: &QualifiedPath) -> Result<(), KvError> {
        let key = key_hash(path);
        sqlx::query("DELETE FROM kv WHERE key_hash = ?")
            .bind(&key)
            .execute(&self.0)
            .await?;
        Ok(())
    }

    async fn list(&self, prefix: &QualifiedPath) -> Result<Vec<Entry>, KvError> {
        let encoded_prefix = key_hash(prefix);
        let upper_bound = strinc(&encoded_prefix);
        let rows: Vec<(Vec<u8>, String)> = sqlx::query_as(
            "SELECT value, versionstamp FROM kv WHERE key_hash > ? AND key_hash < ? ORDER BY key_hash",
        )
        .bind(&encoded_prefix)
        .bind(&upper_bound)
        .fetch_all(&self.0)
        .await?;

        rows.into_iter()
            .map(|(blob, vs)| {
                let value: Value = serde_json::from_slice(&blob)?;
                Ok(Entry {
                    value,
                    versionstamp: vs,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> SqliteKvRepository {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteKvRepository::new(pool);
        repo.init().await.unwrap();
        repo
    }

    // ── key_hash encoding tests ────────────────────────────────────

    #[test]
    fn key_hash_single_segment() {
        let path = QualifiedPath::from("app");
        let encoded = key_hash(&path);
        assert_eq!(encoded, vec![0x02, b'a', b'p', b'p', 0x00]);
    }

    #[test]
    fn key_hash_multiple_segments() {
        let path = QualifiedPath::from(vec!["app", "users"]);
        let encoded = key_hash(&path);
        assert_eq!(
            encoded,
            vec![
                0x02, b'a', b'p', b'p', 0x00, 0x02, b'u', b's', b'e', b'r', b's', 0x00
            ]
        );
    }

    #[test]
    fn key_hash_preserves_ordering() {
        let path_a = QualifiedPath::from(vec!["app", "a"]);
        let path_b = QualifiedPath::from(vec!["app", "b"]);
        let path_ab = QualifiedPath::from(vec!["app", "ab"]);

        let enc_a = key_hash(&path_a);
        let enc_b = key_hash(&path_b);
        let enc_ab = key_hash(&path_ab);

        // a < ab < b (lexicographic)
        assert!(enc_a < enc_ab);
        assert!(enc_ab < enc_b);
    }

    // ── strinc tests ───────────────────────────────────────────────

    #[test]
    fn strinc_basic() {
        let key = vec![0x02, b'a', b'p', b'p', 0x00];
        let inc = strinc(&key);
        assert_eq!(inc, vec![0x02, b'a', b'p', b'p', 0x01]);
    }

    #[test]
    fn strinc_trailing_ff() {
        let key = vec![0x02, 0xFF, 0xFF];
        let inc = strinc(&key);
        assert_eq!(inc, vec![0x03]);
    }

    // ── get tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let repo = setup().await;
        let path = QualifiedPath::from(vec!["root", "missing"]);
        let result = repo.get(&path).await.unwrap();
        assert!(result.is_none());
    }

    // ── set + get round-trip tests ─────────────────────────────────

    #[tokio::test]
    async fn set_then_get_roundtrips() {
        let repo = setup().await;
        let path = QualifiedPath::from(vec!["root", "counter"]);
        let value = Value::Int(42);

        let entry = repo.set(&path, &value).await.unwrap();
        assert_eq!(entry.value, value);
        assert!(!entry.versionstamp.is_empty());

        let fetched = repo.get(&path).await.unwrap().expect("should exist");
        assert_eq!(fetched.value, value);
        assert_eq!(fetched.versionstamp, entry.versionstamp);
    }

    #[tokio::test]
    async fn set_overwrites_existing_key() {
        let repo = setup().await;
        let path = QualifiedPath::from(vec!["root", "counter"]);

        let entry1 = repo.set(&path, &Value::Int(1)).await.unwrap();
        let entry2 = repo.set(&path, &Value::Int(2)).await.unwrap();

        // New versionstamp should differ (or at least the value is updated)
        assert_eq!(entry2.value, Value::Int(2));
        assert!(entry2.versionstamp >= entry1.versionstamp);

        let fetched = repo.get(&path).await.unwrap().expect("should exist");
        assert_eq!(fetched.value, Value::Int(2));
    }

    // ── delete tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn delete_removes_key() {
        let repo = setup().await;
        let path = QualifiedPath::from(vec!["root", "key"]);
        repo.set(&path, &Value::String("hello".into()))
            .await
            .unwrap();

        repo.delete(&path).await.unwrap();
        let result = repo.get(&path).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_missing_key_is_noop() {
        let repo = setup().await;
        let path = QualifiedPath::from(vec!["root", "nonexistent"]);
        // Should not error
        repo.delete(&path).await.unwrap();
    }

    // ── list tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn list_returns_children_sorted() {
        let repo = setup().await;

        // Insert children under root::app
        let path_a = QualifiedPath::from(vec!["root", "app", "alpha"]);
        let path_b = QualifiedPath::from(vec!["root", "app", "beta"]);
        let path_c = QualifiedPath::from(vec!["root", "app", "gamma"]);

        repo.set(&path_b, &Value::Int(2)).await.unwrap();
        repo.set(&path_a, &Value::Int(1)).await.unwrap();
        repo.set(&path_c, &Value::Int(3)).await.unwrap();

        let prefix = QualifiedPath::from(vec!["root", "app"]);
        let entries = repo.list(&prefix).await.unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].value, Value::Int(1)); // alpha
        assert_eq!(entries[1].value, Value::Int(2)); // beta
        assert_eq!(entries[2].value, Value::Int(3)); // gamma
    }

    #[tokio::test]
    async fn list_excludes_exact_prefix_match() {
        let repo = setup().await;

        let prefix = QualifiedPath::from(vec!["root", "app"]);
        let child = QualifiedPath::from(vec!["root", "app", "child"]);

        repo.set(&prefix, &Value::String("parent".into()))
            .await
            .unwrap();
        repo.set(&child, &Value::String("child".into()))
            .await
            .unwrap();

        let entries = repo.list(&prefix).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, Value::String("child".into()));
    }

    #[tokio::test]
    async fn list_empty_prefix_returns_empty() {
        let repo = setup().await;

        // Insert some data under a different prefix
        let path = QualifiedPath::from(vec!["root", "other", "key"]);
        repo.set(&path, &Value::Int(1)).await.unwrap();

        let prefix = QualifiedPath::from(vec!["root", "empty"]);
        let entries = repo.list(&prefix).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn list_does_not_include_sibling_prefixes() {
        let repo = setup().await;

        let app_key = QualifiedPath::from(vec!["root", "app", "key"]);
        let other_key = QualifiedPath::from(vec!["root", "other", "key"]);

        repo.set(&app_key, &Value::Int(1)).await.unwrap();
        repo.set(&other_key, &Value::Int(2)).await.unwrap();

        let prefix = QualifiedPath::from(vec!["root", "app"]);
        let entries = repo.list(&prefix).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, Value::Int(1));
    }
}
