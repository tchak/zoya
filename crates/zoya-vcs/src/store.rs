use std::collections::HashMap;
use std::path::Path;
use std::time::UNIX_EPOCH;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::{Commit, Operation, Tree};

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

        let store = Store { runtime, pool };
        store.initialize_if_empty()?;
        Ok(store)
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

        let store = Store { runtime, pool };
        store.initialize_if_empty()?;
        Ok(store)
    }

    fn initialize_if_empty(&self) -> Result<(), sqlx::Error> {
        self.runtime.block_on(async {
            let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM commits")
                .fetch_one(&self.pool)
                .await?;

            if count > 0 {
                return Ok(());
            }

            let empty_tree = Tree::new(HashMap::new());
            let change_id = Uuid::now_v7();
            let root_commit = Commit::new(change_id, &[], empty_tree, String::new());

            let operation_id = Uuid::now_v7();
            let init_op = Operation::new(operation_id, "init".to_string(), root_commit, vec![]);

            let mut tx = self.pool.begin().await?;
            for head in init_op.heads() {
                Self::save_commit_with_tx(&mut tx, head).await?;
            }
            Self::save_operation_with_tx(&mut tx, &init_op).await?;
            tx.commit().await?;

            Ok(())
        })
    }

    pub fn save_commit(&self, commit: &Commit) -> Result<(), sqlx::Error> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await?;
            Self::save_commit_with_tx(&mut tx, commit).await?;
            tx.commit().await?;
            Ok(())
        })
    }

    pub fn save_operation(&self, operation: &Operation) -> Result<(), sqlx::Error> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await?;
            for head in operation.heads() {
                Self::save_commit_with_tx(&mut tx, head).await?;
            }
            Self::save_operation_with_tx(&mut tx, operation).await?;
            tx.commit().await?;
            Ok(())
        })
    }

    async fn save_commit_with_tx(
        conn: &mut SqliteConnection,
        commit: &Commit,
    ) -> Result<(), sqlx::Error> {
        // 1. Check if tree already exists (blobs and entries must exist too)
        let (tree_exists,): (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM trees WHERE tree_id = ?)")
                .bind(commit.tree().id())
                .fetch_one(&mut *conn)
                .await?;

        if !tree_exists {
            // 2. Insert blobs
            for blob in commit.tree().blobs().values() {
                sqlx::query(
                    "INSERT OR IGNORE INTO blobs (blob_id, content, size) VALUES (?, ?, ?)",
                )
                .bind(blob.id())
                .bind(blob.content())
                .bind(blob.size() as i64)
                .execute(&mut *conn)
                .await?;
            }

            // 3. Insert tree
            sqlx::query("INSERT OR IGNORE INTO trees (tree_id) VALUES (?)")
                .bind(commit.tree().id())
                .execute(&mut *conn)
                .await?;

            // 4. Insert tree entries
            for (path, blob) in commit.tree().blobs() {
                sqlx::query(
                    "INSERT OR IGNORE INTO tree_entries (tree_id, path, blob_id) VALUES (?, ?, ?)",
                )
                .bind(commit.tree().id())
                .bind(path)
                .bind(blob.id())
                .execute(&mut *conn)
                .await?;
            }
        }

        // 5. Insert change
        sqlx::query("INSERT OR IGNORE INTO changes (change_id) VALUES (?)")
            .bind(commit.change_id().to_string())
            .execute(&mut *conn)
            .await?;

        // 6. Insert commit
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
        .execute(&mut *conn)
        .await?;

        // 7. Insert parent links
        for (i, parent_id) in commit.parents().iter().enumerate() {
            sqlx::query(
                "INSERT OR IGNORE INTO commit_parents (commit_id, parent_commit_id, parent_order) VALUES (?, ?, ?)",
            )
            .bind(commit.commit_id())
            .bind(parent_id)
            .bind(i as i32)
            .execute(&mut *conn)
            .await?;
        }

        Ok(())
    }

    async fn save_operation_with_tx(
        conn: &mut SqliteConnection,
        operation: &Operation,
    ) -> Result<(), sqlx::Error> {
        // 1. Insert view
        sqlx::query("INSERT OR IGNORE INTO views (view_id, working_copy_commit_id) VALUES (?, ?)")
            .bind(operation.view_id())
            .bind(operation.working_copy().commit_id())
            .execute(&mut *conn)
            .await?;

        // 2. Insert view heads
        for head in operation.heads() {
            sqlx::query("INSERT OR IGNORE INTO view_heads (view_id, commit_id) VALUES (?, ?)")
                .bind(operation.view_id())
                .bind(head.commit_id())
                .execute(&mut *conn)
                .await?;
        }

        // 3. Insert operation
        let timestamp = operation
            .timestamp()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        sqlx::query(
            "INSERT OR IGNORE INTO operations (operation_id, operation_type, view_id, timestamp) VALUES (?, ?, ?, ?)",
        )
        .bind(operation.operation_id().to_string())
        .bind(operation.operation_type())
        .bind(operation.view_id())
        .bind(timestamp)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

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
        Commit::new(change_id, &[], tree, "test commit".to_string())
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
    fn test_init_creates_root_commit() {
        let store = Store::init_memory().unwrap();

        let (count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM commits").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(count, 1);

        let (message, tree_id): (String, String) = store
            .runtime
            .block_on(sqlx::query_as("SELECT message, tree_id FROM commits").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(message, "");

        // Root commit has empty tree
        let empty_tree = Tree::new(HashMap::new());
        assert_eq!(tree_id, empty_tree.id());

        // Root commit has no parents
        let (parent_count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM commit_parents").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(parent_count, 0);
    }

    #[test]
    fn test_init_creates_init_operation() {
        let store = Store::init_memory().unwrap();

        let (count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM operations").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(count, 1);

        let (op_type, timestamp): (String, i64) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT operation_type, timestamp FROM operations")
                    .fetch_one(&store.pool),
            )
            .unwrap();
        assert_eq!(op_type, "init");
        assert!(timestamp > 0);
    }

    #[test]
    fn test_init_creates_view() {
        let store = Store::init_memory().unwrap();

        let (count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM views").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(count, 1);

        // View's working_copy_commit_id matches the root commit
        let (wc_commit_id,): (String,) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT working_copy_commit_id FROM views").fetch_one(&store.pool),
            )
            .unwrap();

        let (root_commit_id,): (String,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT commit_id FROM commits").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(wc_commit_id, root_commit_id);

        // View has 1 head
        let (head_count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM view_heads").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(head_count, 1);
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
        assert!(timestamp > 0);
    }

    #[test]
    fn test_save_commit_idempotent() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();
        store.save_commit(&commit).unwrap();

        // 1 root commit + 1 user commit = 2
        let (count,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM commits").fetch_one(&store.pool))
            .unwrap();

        assert_eq!(count, 2);
    }

    #[test]
    fn test_save_commit_stores_blobs() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();

        let (blob_id, content, size): (String, String, i64) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT blob_id, content, size FROM blobs WHERE content = ?")
                    .bind("hello world")
                    .fetch_one(&store.pool),
            )
            .unwrap();

        let expected_blob = commit.tree().get("root").unwrap();
        assert_eq!(blob_id, expected_blob.id());
        assert_eq!(content, "hello world");
        assert_eq!(size, 11);
    }

    #[test]
    fn test_save_commit_stores_tree_entries() {
        let store = Store::init_memory().unwrap();
        let commit = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit).unwrap();

        let (tree_id, path, blob_id): (String, String, String) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT tree_id, path, blob_id FROM tree_entries WHERE tree_id = ?")
                    .bind(commit.tree().id())
                    .fetch_one(&store.pool),
            )
            .unwrap();

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
        let child = Commit::new(
            CHANGE_2,
            &[parent.commit_id().to_string()],
            tree,
            "child commit".to_string(),
        );
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

    #[test]
    fn test_save_operation() {
        let store = Store::init_memory().unwrap();

        let commit = make_commit("hello world", CHANGE_1);
        let op_id = Uuid::from_bytes([
            0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae,
            0xaf, 0xb0,
        ]);
        let operation = Operation::new(op_id, "snapshot".to_string(), commit.clone(), vec![commit]);

        store.save_operation(&operation).unwrap();

        // Verify operation row
        let (op_type, view_id, timestamp): (String, String, i64) = store
            .runtime
            .block_on(
                sqlx::query_as(
                    "SELECT operation_type, view_id, timestamp FROM operations WHERE operation_id = ?",
                )
                .bind(op_id.to_string())
                .fetch_one(&store.pool),
            )
            .unwrap();
        assert_eq!(op_type, "snapshot");
        assert_eq!(view_id, operation.view_id());
        assert!(timestamp > 0);

        // Verify view row
        let (wc_commit_id,): (String,) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT working_copy_commit_id FROM views WHERE view_id = ?")
                    .bind(operation.view_id())
                    .fetch_one(&store.pool),
            )
            .unwrap();
        assert_eq!(wc_commit_id, operation.working_copy().commit_id());

        // Verify view_heads row
        let heads: Vec<(String,)> = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT commit_id FROM view_heads WHERE view_id = ?")
                    .bind(operation.view_id())
                    .fetch_all(&store.pool),
            )
            .unwrap();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].0, operation.heads()[0].commit_id());
    }
}
