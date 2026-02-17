use std::path::Path;
use std::time::UNIX_EPOCH;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use tokio::runtime::Runtime;

use crate::Commit;

const SCHEMA_SQL: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS blobs (
    blob_id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    size INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS trees (
    tree_id TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS tree_entries (
    tree_id TEXT NOT NULL REFERENCES trees(tree_id),
    path TEXT NOT NULL,
    blob_id TEXT NOT NULL REFERENCES blobs(blob_id),
    PRIMARY KEY (tree_id, path)
);
CREATE INDEX IF NOT EXISTS idx_tree_entries_blob_id ON tree_entries(blob_id);

CREATE TABLE IF NOT EXISTS changes (
    change_id TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS commits (
    commit_id TEXT PRIMARY KEY,
    change_id TEXT NOT NULL REFERENCES changes(change_id),
    timestamp INTEGER NOT NULL,
    tree_id TEXT NOT NULL REFERENCES trees(tree_id),
    message TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_commits_change_id ON commits(change_id);
CREATE INDEX IF NOT EXISTS idx_commits_tree_id ON commits(tree_id);

CREATE TABLE IF NOT EXISTS commit_parents (
    commit_id TEXT NOT NULL REFERENCES commits(commit_id),
    parent_commit_id TEXT NOT NULL REFERENCES commits(commit_id),
    parent_order INTEGER NOT NULL,
    PRIMARY KEY (commit_id, parent_commit_id)
);

CREATE TABLE IF NOT EXISTS views (
    view_id TEXT PRIMARY KEY,
    working_copy_commit_id TEXT NOT NULL REFERENCES commits(commit_id)
);

CREATE TABLE IF NOT EXISTS view_heads (
    view_id TEXT NOT NULL REFERENCES views(view_id),
    commit_id TEXT NOT NULL REFERENCES commits(commit_id),
    PRIMARY KEY (view_id, commit_id)
);

CREATE TABLE IF NOT EXISTS operations (
    operation_id TEXT PRIMARY KEY,
    operation_type TEXT NOT NULL,
    view_id TEXT NOT NULL REFERENCES views(view_id),
    timestamp INTEGER NOT NULL
);
"#;

pub struct Store {
    runtime: Runtime,
    pool: SqlitePool,
}

impl Store {
    pub fn init(path: &Path) -> Result<Self, sqlx::Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);

        let pool = runtime.block_on(SqlitePool::connect_with(options))?;
        runtime.block_on(sqlx::raw_sql(SCHEMA_SQL).execute(&pool))?;

        Ok(Store { runtime, pool })
    }

    #[cfg(test)]
    fn init_memory() -> Result<Self, sqlx::Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true);

        let pool = runtime.block_on(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options),
        )?;
        runtime.block_on(sqlx::raw_sql(SCHEMA_SQL).execute(&pool))?;

        Ok(Store { runtime, pool })
    }

    pub fn save_commit(&self, commit: &Commit) -> Result<(), sqlx::Error> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await?;

            // 1. Insert blobs
            for blob in commit.tree().blobs().values() {
                sqlx::query("INSERT OR IGNORE INTO blobs (blob_id, content, size) VALUES (?, ?, ?)")
                    .bind(blob.id())
                    .bind(blob.content())
                    .bind(blob.size() as i64)
                    .execute(&mut *tx)
                    .await?;
            }

            // 2. Insert tree
            sqlx::query("INSERT OR IGNORE INTO trees (tree_id) VALUES (?)")
                .bind(commit.tree().id())
                .execute(&mut *tx)
                .await?;

            // 3. Insert tree entries
            for (path, blob) in commit.tree().blobs() {
                sqlx::query(
                    "INSERT OR IGNORE INTO tree_entries (tree_id, path, blob_id) VALUES (?, ?, ?)",
                )
                .bind(commit.tree().id())
                .bind(path)
                .bind(blob.id())
                .execute(&mut *tx)
                .await?;
            }

            // 4. Insert change
            sqlx::query("INSERT OR IGNORE INTO changes (change_id) VALUES (?)")
                .bind(commit.change_id().to_string())
                .execute(&mut *tx)
                .await?;

            // 5. Insert commit
            let timestamp = commit
                .timestamp()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            sqlx::query(
                "INSERT OR IGNORE INTO commits (commit_id, change_id, timestamp, tree_id, message) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(commit.commit_id())
            .bind(commit.change_id().to_string())
            .bind(timestamp)
            .bind(commit.tree().id())
            .bind(commit.message())
            .execute(&mut *tx)
            .await?;

            // 6. Insert parent link
            if let Some(parent_id) = commit.parent_id() {
                sqlx::query(
                    "INSERT OR IGNORE INTO commit_parents (commit_id, parent_commit_id, parent_order) VALUES (?, ?, ?)",
                )
                .bind(commit.commit_id())
                .bind(parent_id)
                .bind(0i32)
                .execute(&mut *tx)
                .await?;
            }

            tx.commit().await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::UNIX_EPOCH;

    use uuid::Uuid;

    use crate::Blob;
    use crate::Tree;

    use super::*;

    const CHANGE_1: Uuid = Uuid::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ]);
    const CHANGE_2: Uuid = Uuid::from_bytes([
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        0x20,
    ]);

    fn make_commit(content: &str, change_id: Uuid) -> Commit {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        let tree = Tree::new(blobs);
        Commit::builder(change_id, tree)
            .message("test commit".to_string())
            .timestamp(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000))
            .build()
    }

    #[test]
    fn test_init_creates_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        assert!(!db_path.exists());

        let _store = Store::init(&db_path).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn test_init_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let _store1 = Store::init(&db_path).unwrap();
        drop(_store1);
        let _store2 = Store::init(&db_path).unwrap();
    }

    #[test]
    fn test_tables_exist() {
        let store = Store::init_memory().unwrap();

        let expected_tables = [
            "blobs",
            "trees",
            "tree_entries",
            "changes",
            "commits",
            "commit_parents",
            "views",
            "view_heads",
            "operations",
        ];

        let tables: Vec<(String,)> = store
            .runtime
            .block_on(
                sqlx::query_as(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )
                .fetch_all(&store.pool),
            )
            .unwrap();

        let table_names: Vec<&str> = tables.iter().map(|(name,)| name.as_str()).collect();

        for expected in &expected_tables {
            assert!(
                table_names.contains(expected),
                "table '{expected}' not found in {table_names:?}"
            );
        }
    }

    #[test]
    fn test_foreign_keys_enabled() {
        let store = Store::init_memory().unwrap();

        let (fk_status,): (i32,) = store
            .runtime
            .block_on(sqlx::query_as("PRAGMA foreign_keys").fetch_one(&store.pool))
            .unwrap();

        assert_eq!(fk_status, 1);
    }

    #[test]
    fn test_save_commit() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();

        let (id, change_id, message, timestamp): (String, String, String, i64) = store
            .runtime
            .block_on(
                sqlx::query_as(
                    "SELECT commit_id, change_id, message, timestamp FROM commits WHERE commit_id = ?",
                )
                .bind(commit.commit_id())
                .fetch_one(&store.pool),
            )
            .unwrap();

        assert_eq!(id, commit.commit_id());
        assert_eq!(change_id, CHANGE_1.to_string());
        assert_eq!(message, "test commit");
        assert_eq!(timestamp, 1_700_000_000);
    }

    #[test]
    fn test_save_commit_idempotent() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();
        store.save_commit(&commit).unwrap();

        let (count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM commits").fetch_one(&store.pool))
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn test_save_commit_stores_blobs() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();

        let rows: Vec<(String, String, i64)> = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT blob_id, content, size FROM blobs").fetch_all(&store.pool),
            )
            .unwrap();

        assert_eq!(rows.len(), 1);
        let (blob_id, content, size) = &rows[0];
        let expected_blob = commit.tree().get("root").unwrap();
        assert_eq!(blob_id, expected_blob.id());
        assert_eq!(content, "hello world");
        assert_eq!(*size, 11);
    }

    #[test]
    fn test_save_commit_stores_tree_entries() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();

        let rows: Vec<(String, String, String)> = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT tree_id, path, blob_id FROM tree_entries")
                    .fetch_all(&store.pool),
            )
            .unwrap();

        assert_eq!(rows.len(), 1);
        let (tree_id, path, blob_id) = &rows[0];
        assert_eq!(tree_id, commit.tree().id());
        assert_eq!(path, "root");
        assert_eq!(blob_id, commit.tree().get("root").unwrap().id());
    }

    #[test]
    fn test_save_commit_with_parent() {
        let store = Store::init_memory().unwrap();

        let parent = make_commit("v1", CHANGE_1);
        store.save_commit(&parent).unwrap();

        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new("v2".to_string()));
        let tree = Tree::new(blobs);
        let child = Commit::builder(CHANGE_2, tree)
            .parent_id(parent.commit_id().to_string())
            .message("child commit".to_string())
            .timestamp(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_001))
            .build();
        store.save_commit(&child).unwrap();

        let (parent_commit_id, parent_order): (String, i32) = store
            .runtime
            .block_on(
                sqlx::query_as(
                    "SELECT parent_commit_id, parent_order FROM commit_parents WHERE commit_id = ?",
                )
                .bind(child.commit_id())
                .fetch_one(&store.pool),
            )
            .unwrap();

        assert_eq!(parent_commit_id, parent.commit_id());
        assert_eq!(parent_order, 0);
    }
}
