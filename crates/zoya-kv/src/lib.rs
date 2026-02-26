use std::collections::HashMap;
use std::time::SystemTime;

use sqlx::SqlitePool;
use zoya_package::QualifiedPath;
use zoya_value::{Value, ValueData};

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
    pub key: QualifiedPath,
    pub value: Value,
    pub versionstamp: String,
}

fn kv_module() -> QualifiedPath {
    QualifiedPath::from(vec!["std", "kv"])
}

impl From<Entry> for Value {
    fn from(entry: Entry) -> Self {
        let mut fields = HashMap::new();
        fields.insert(
            "key".into(),
            Value::List(
                entry
                    .key
                    .segments()
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            ),
        );
        fields.insert("value".into(), entry.value);
        fields.insert("versionstamp".into(), Value::String(entry.versionstamp));
        Value::Struct {
            name: "Entry".into(),
            module: kv_module(),
            data: ValueData::Struct(fields),
        }
    }
}

/// Key-value repository trait with async methods (RPITIT, edition 2024).
pub trait KvRepository {
    /// Create the `kv_store` table if it doesn't exist.
    fn init(&self) -> impl Future<Output = Result<(), KvError>> + Send;

    /// Fetch a single value by exact key.
    fn get(
        &self,
        key: &QualifiedPath,
    ) -> impl Future<Output = Result<Option<Entry>, KvError>> + Send;

    /// Upsert a value at the given key.
    fn set(
        &self,
        key: &QualifiedPath,
        value: &Value,
    ) -> impl Future<Output = Result<Entry, KvError>> + Send;

    /// Delete a value by exact key.
    fn delete(&self, key: &QualifiedPath) -> impl Future<Output = Result<(), KvError>> + Send;

    /// List all entries whose key starts with `{prefix}::`, ordered by key.
    /// Excludes the exact prefix match — only returns children.
    fn list(
        &self,
        prefix: &QualifiedPath,
    ) -> impl Future<Output = Result<Vec<Entry>, KvError>> + Send;
}

/// Encode a `QualifiedPath` into a binary key that preserves lexicographic ordering.
///
/// Each segment is encoded as: `0x02 + utf8_bytes + 0x00`.
/// Segments are concatenated directly. Follows Deno.KV's encoding format.
fn key_encode(path: &QualifiedPath) -> Vec<u8> {
    let mut result = Vec::new();
    for segment in path.segments() {
        result.push(0x02);
        result.extend_from_slice(segment.as_bytes());
        result.push(0x00);
    }
    result
}

/// Decode a binary key back into a `QualifiedPath`.
///
/// Inverse of `key_encode`: splits on `0x02...0x00` boundaries,
/// extracts UTF-8 segments, and wraps them in a `QualifiedPath`.
fn key_decode(encoded: &[u8]) -> QualifiedPath {
    let mut segments = Vec::new();
    let mut i = 0;
    while i < encoded.len() {
        if encoded[i] == 0x02 {
            i += 1;
            let start = i;
            while i < encoded.len() && encoded[i] != 0x00 {
                i += 1;
            }
            let segment = String::from_utf8(encoded[start..i].to_vec())
                .expect("key_decode: invalid UTF-8 in segment");
            segments.push(segment);
            if i < encoded.len() {
                i += 1; // skip 0x00 terminator
            }
        } else {
            i += 1;
        }
    }
    QualifiedPath::from(segments.iter().map(|s| s.as_str()).collect::<Vec<_>>())
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
            "CREATE TABLE IF NOT EXISTS kv_store (
                key BLOB PRIMARY KEY NOT NULL,
                value BLOB NOT NULL,
                versionstamp TEXT NOT NULL
            )",
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn get(&self, key: &QualifiedPath) -> Result<Option<Entry>, KvError> {
        let encoded = key_encode(key);
        let row: Option<(Vec<u8>, String)> =
            sqlx::query_as("SELECT value, versionstamp FROM kv_store WHERE key = ?")
                .bind(&encoded)
                .fetch_optional(&self.0)
                .await?;
        match row {
            Some((blob, vs)) => {
                let value: Value = serde_json::from_slice(&blob)?;
                Ok(Some(Entry {
                    key: key.clone(),
                    value,
                    versionstamp: vs,
                }))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &QualifiedPath, value: &Value) -> Result<Entry, KvError> {
        let encoded = key_encode(key);
        let blob = serde_json::to_vec(value)?;
        let vs = versionstamp();
        sqlx::query(
            "INSERT INTO kv_store (key, value, versionstamp) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, versionstamp = excluded.versionstamp",
        )
        .bind(&encoded)
        .bind(&blob)
        .bind(&vs)
        .execute(&self.0)
        .await?;
        Ok(Entry {
            key: key.clone(),
            value: value.clone(),
            versionstamp: vs,
        })
    }

    async fn delete(&self, key: &QualifiedPath) -> Result<(), KvError> {
        let encoded = key_encode(key);
        sqlx::query("DELETE FROM kv_store WHERE key = ?")
            .bind(&encoded)
            .execute(&self.0)
            .await?;
        Ok(())
    }

    async fn list(&self, prefix: &QualifiedPath) -> Result<Vec<Entry>, KvError> {
        let encoded_prefix = key_encode(prefix);
        let upper_bound = strinc(&encoded_prefix);
        let rows: Vec<(Vec<u8>, Vec<u8>, String)> = sqlx::query_as(
            "SELECT key, value, versionstamp FROM kv_store WHERE key > ? AND key < ? ORDER BY key",
        )
        .bind(&encoded_prefix)
        .bind(&upper_bound)
        .fetch_all(&self.0)
        .await?;

        rows.into_iter()
            .map(|(key_blob, value_blob, vs)| {
                let key = key_decode(&key_blob);
                let value: Value = serde_json::from_slice(&value_blob)?;
                Ok(Entry {
                    key,
                    value,
                    versionstamp: vs,
                })
            })
            .collect()
    }
}

/// High-level KV facade that wraps a `KvRepository` and swallows errors.
///
/// All methods are infallible — errors are logged with `tracing::warn!`
/// and replaced with sensible defaults (`None`, `()`, empty `Vec`).
pub struct Kv<R: KvRepository> {
    repository: R,
}

impl<R: KvRepository> Kv<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn get(&self, key: &QualifiedPath) -> Option<Entry> {
        match self.repository.get(key).await {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!("kv get failed for {key}: {e}");
                None
            }
        }
    }

    pub async fn set(&self, key: &QualifiedPath, value: &Value) {
        if let Err(e) = self.repository.set(key, value).await {
            tracing::warn!("kv set failed for {key}: {e}");
        }
    }

    pub async fn delete(&self, key: &QualifiedPath) {
        if let Err(e) = self.repository.delete(key).await {
            tracing::warn!("kv delete failed for {key}: {e}");
        }
    }

    pub async fn list(&self, prefix: &QualifiedPath) -> Vec<Entry> {
        match self.repository.list(prefix).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("kv list failed for {prefix}: {e}");
                Vec::new()
            }
        }
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

    // ── key_encode tests ─────────────────────────────────────────────

    #[test]
    fn key_encode_single_segment() {
        let path = QualifiedPath::from("app");
        let encoded = key_encode(&path);
        assert_eq!(encoded, vec![0x02, b'a', b'p', b'p', 0x00]);
    }

    #[test]
    fn key_encode_multiple_segments() {
        let path = QualifiedPath::from(vec!["app", "users"]);
        let encoded = key_encode(&path);
        assert_eq!(
            encoded,
            vec![
                0x02, b'a', b'p', b'p', 0x00, 0x02, b'u', b's', b'e', b'r', b's', 0x00
            ]
        );
    }

    #[test]
    fn key_encode_preserves_ordering() {
        let path_a = QualifiedPath::from(vec!["app", "a"]);
        let path_b = QualifiedPath::from(vec!["app", "b"]);
        let path_ab = QualifiedPath::from(vec!["app", "ab"]);

        let enc_a = key_encode(&path_a);
        let enc_b = key_encode(&path_b);
        let enc_ab = key_encode(&path_ab);

        // a < ab < b (lexicographic)
        assert!(enc_a < enc_ab);
        assert!(enc_ab < enc_b);
    }

    // ── key_decode tests ─────────────────────────────────────────────

    #[test]
    fn key_decode_single_segment() {
        let encoded = vec![0x02, b'a', b'p', b'p', 0x00];
        let path = key_decode(&encoded);
        assert_eq!(path, QualifiedPath::from("app"));
    }

    #[test]
    fn key_decode_multiple_segments() {
        let encoded = vec![
            0x02, b'a', b'p', b'p', 0x00, 0x02, b'u', b's', b'e', b'r', b's', 0x00,
        ];
        let path = key_decode(&encoded);
        assert_eq!(path, QualifiedPath::from(vec!["app", "users"]));
    }

    #[test]
    fn key_decode_roundtrips_with_encode() {
        let original = QualifiedPath::from(vec!["root", "app", "users", "123"]);
        let encoded = key_encode(&original);
        let decoded = key_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn key_decode_roundtrips_single_segment() {
        let original = QualifiedPath::from("hello");
        let encoded = key_encode(&original);
        let decoded = key_decode(&encoded);
        assert_eq!(decoded, original);
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

    // ── Entry into Value tests ──────────────────────────────────────

    #[test]
    fn entry_into_value_produces_struct() {
        let entry = Entry {
            key: QualifiedPath::from(vec!["app", "users", "1"]),
            value: Value::Int(42),
            versionstamp: "12345".into(),
        };
        let value: Value = entry.into();

        match value {
            Value::Struct { name, module, data } => {
                assert_eq!(name, "Entry");
                assert_eq!(module, kv_module());
                match data {
                    ValueData::Struct(fields) => {
                        assert_eq!(
                            fields.get("key"),
                            Some(&Value::List(vec![
                                Value::String("app".into()),
                                Value::String("users".into()),
                                Value::String("1".into()),
                            ]))
                        );
                        assert_eq!(fields.get("value"), Some(&Value::Int(42)));
                        assert_eq!(
                            fields.get("versionstamp"),
                            Some(&Value::String("12345".into()))
                        );
                    }
                    _ => panic!("expected ValueData::Struct"),
                }
            }
            _ => panic!("expected Value::Struct"),
        }
    }

    // ── get tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let repo = setup().await;
        let key = QualifiedPath::from(vec!["root", "missing"]);
        let result = repo.get(&key).await.unwrap();
        assert!(result.is_none());
    }

    // ── set + get round-trip tests ─────────────────────────────────

    #[tokio::test]
    async fn set_then_get_roundtrips() {
        let repo = setup().await;
        let key = QualifiedPath::from(vec!["root", "counter"]);
        let value = Value::Int(42);

        let entry = repo.set(&key, &value).await.unwrap();
        assert_eq!(entry.key, key);
        assert_eq!(entry.value, value);
        assert!(!entry.versionstamp.is_empty());

        let fetched = repo.get(&key).await.unwrap().expect("should exist");
        assert_eq!(fetched.key, key);
        assert_eq!(fetched.value, value);
        assert_eq!(fetched.versionstamp, entry.versionstamp);
    }

    #[tokio::test]
    async fn set_overwrites_existing_key() {
        let repo = setup().await;
        let key = QualifiedPath::from(vec!["root", "counter"]);

        let entry1 = repo.set(&key, &Value::Int(1)).await.unwrap();
        let entry2 = repo.set(&key, &Value::Int(2)).await.unwrap();

        // New versionstamp should differ (or at least the value is updated)
        assert_eq!(entry2.key, key);
        assert_eq!(entry2.value, Value::Int(2));
        assert!(entry2.versionstamp >= entry1.versionstamp);

        let fetched = repo.get(&key).await.unwrap().expect("should exist");
        assert_eq!(fetched.value, Value::Int(2));
    }

    // ── delete tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn delete_removes_key() {
        let repo = setup().await;
        let key = QualifiedPath::from(vec!["root", "key"]);
        repo.set(&key, &Value::String("hello".into()))
            .await
            .unwrap();

        repo.delete(&key).await.unwrap();
        let result = repo.get(&key).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_missing_key_is_noop() {
        let repo = setup().await;
        let key = QualifiedPath::from(vec!["root", "nonexistent"]);
        // Should not error
        repo.delete(&key).await.unwrap();
    }

    // ── list tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn list_returns_children_sorted() {
        let repo = setup().await;

        // Insert children under root::app
        let key_a = QualifiedPath::from(vec!["root", "app", "alpha"]);
        let key_b = QualifiedPath::from(vec!["root", "app", "beta"]);
        let key_c = QualifiedPath::from(vec!["root", "app", "gamma"]);

        repo.set(&key_b, &Value::Int(2)).await.unwrap();
        repo.set(&key_a, &Value::Int(1)).await.unwrap();
        repo.set(&key_c, &Value::Int(3)).await.unwrap();

        let prefix = QualifiedPath::from(vec!["root", "app"]);
        let entries = repo.list(&prefix).await.unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, key_a);
        assert_eq!(entries[0].value, Value::Int(1)); // alpha
        assert_eq!(entries[1].key, key_b);
        assert_eq!(entries[1].value, Value::Int(2)); // beta
        assert_eq!(entries[2].key, key_c);
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
        assert_eq!(entries[0].key, child);
        assert_eq!(entries[0].value, Value::String("child".into()));
    }

    #[tokio::test]
    async fn list_empty_prefix_returns_empty() {
        let repo = setup().await;

        // Insert some data under a different prefix
        let key = QualifiedPath::from(vec!["root", "other", "key"]);
        repo.set(&key, &Value::Int(1)).await.unwrap();

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
        assert_eq!(entries[0].key, app_key);
        assert_eq!(entries[0].value, Value::Int(1));
    }

    // ── get returns entry with correct key ─────────────────────────

    #[tokio::test]
    async fn get_returns_entry_with_correct_key() {
        let repo = setup().await;
        let key = QualifiedPath::from(vec!["root", "app", "config"]);
        repo.set(&key, &Value::String("data".into())).await.unwrap();

        let entry = repo.get(&key).await.unwrap().expect("should exist");
        assert_eq!(entry.key, key);
    }

    // ── Kv facade tests ─────────────────────────────────────────────

    async fn setup_kv() -> Kv<SqliteKvRepository> {
        let repo = setup().await;
        Kv::new(repo)
    }

    #[tokio::test]
    async fn kv_get_missing_returns_none() {
        let kv = setup_kv().await;
        let key = QualifiedPath::from(vec!["root", "missing"]);
        assert!(kv.get(&key).await.is_none());
    }

    #[tokio::test]
    async fn kv_set_then_get() {
        let kv = setup_kv().await;
        let key = QualifiedPath::from(vec!["root", "counter"]);
        let value = Value::Int(42);

        kv.set(&key, &value).await;

        let entry = kv.get(&key).await.expect("should exist");
        assert_eq!(entry.key, key);
        assert_eq!(entry.value, value);
    }

    #[tokio::test]
    async fn kv_delete_is_silent() {
        let kv = setup_kv().await;
        let key = QualifiedPath::from(vec!["root", "key"]);

        // Delete a missing key — should not panic
        kv.delete(&key).await;

        // Set then delete
        kv.set(&key, &Value::Int(1)).await;
        kv.delete(&key).await;
        assert!(kv.get(&key).await.is_none());
    }

    #[tokio::test]
    async fn kv_list_returns_entries() {
        let kv = setup_kv().await;

        let key_a = QualifiedPath::from(vec!["root", "app", "alpha"]);
        let key_b = QualifiedPath::from(vec!["root", "app", "beta"]);

        kv.set(&key_b, &Value::Int(2)).await;
        kv.set(&key_a, &Value::Int(1)).await;

        let prefix = QualifiedPath::from(vec!["root", "app"]);
        let entries = kv.list(&prefix).await;

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, key_a);
        assert_eq!(entries[0].value, Value::Int(1));
        assert_eq!(entries[1].key, key_b);
        assert_eq!(entries[1].value, Value::Int(2));
    }
}
