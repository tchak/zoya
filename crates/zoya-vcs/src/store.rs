use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use tokio::runtime::Runtime;

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
    view_id TEXT PRIMARY KEY
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
    #[allow(dead_code)]
    runtime: Runtime,
    #[allow(dead_code)]
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
