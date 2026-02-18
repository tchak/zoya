use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::revision::Revision;
use crate::{Blob, Commit, Operation, Tree};

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
        // 1. Check if commit already exists — if so, everything is already saved
        let (commit_exists,): (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM commits WHERE commit_id = ?)")
                .bind(commit.commit_id())
                .fetch_one(&mut *conn)
                .await?;

        if commit_exists {
            return Ok(());
        }

        // 2. Check if tree already exists (blobs and entries must exist too)
        let (tree_exists,): (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM trees WHERE tree_id = ?)")
                .bind(commit.tree().id())
                .fetch_one(&mut *conn)
                .await?;

        if !tree_exists {
            // 3. Insert blobs
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

            // 4. Insert tree
            sqlx::query("INSERT OR IGNORE INTO trees (tree_id) VALUES (?)")
                .bind(commit.tree().id())
                .execute(&mut *conn)
                .await?;

            // 5. Insert tree entries
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

        // 6. Insert change
        sqlx::query("INSERT OR IGNORE INTO changes (change_id) VALUES (?)")
            .bind(commit.change_id().to_string())
            .execute(&mut *conn)
            .await?;

        // 7. Insert commit
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

        // 8. Insert parent links
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

    pub fn log(&self, n: usize) -> Result<Vec<Revision>, sqlx::Error> {
        self.runtime.block_on(async {
            // 1. Find latest view_id, working_copy, and heads
            let (view_id, working_copy_commit_id): (String, String) = sqlx::query_as(
                "SELECT o.view_id, v.working_copy_commit_id \
                 FROM operations o \
                 JOIN views v ON v.view_id = o.view_id \
                 ORDER BY o.operation_id DESC LIMIT 1",
            )
            .fetch_one(&self.pool)
            .await?;

            let head_rows: Vec<(String,)> =
                sqlx::query_as("SELECT commit_id FROM view_heads WHERE view_id = ?")
                    .bind(&view_id)
                    .fetch_all(&self.pool)
                    .await?;
            let head_set: HashSet<String> = head_rows.into_iter().map(|(id,)| id).collect();

            // 2. Recursive CTE to walk ancestor graph
            let limit = (n * 2) as i64;
            let ancestor_rows: Vec<(String, String, i64, String, String)> = sqlx::query_as(
                "WITH RECURSIVE ancestors AS ( \
                     SELECT c.commit_id, c.change_id, c.timestamp, c.tree_id, c.message \
                     FROM commits c \
                     JOIN view_heads vh ON vh.commit_id = c.commit_id \
                     WHERE vh.view_id = ? \
                     UNION \
                     SELECT c.commit_id, c.change_id, c.timestamp, c.tree_id, c.message \
                     FROM ancestors a \
                     JOIN commit_parents cp ON cp.commit_id = a.commit_id \
                     JOIN commits c ON c.commit_id = cp.parent_commit_id \
                 ) \
                 SELECT * FROM ancestors ORDER BY timestamp DESC LIMIT ?",
            )
            .bind(&view_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

            // 3. Group by change_id, take first n revisions
            let mut revision_order: Vec<String> = Vec::new();
            let mut grouped: HashMap<String, Vec<(String, i64, String, String)>> = HashMap::new();
            for (commit_id, change_id, timestamp, tree_id, message) in &ancestor_rows {
                if !grouped.contains_key(change_id) {
                    revision_order.push(change_id.clone());
                }
                grouped.entry(change_id.clone()).or_default().push((
                    commit_id.clone(),
                    *timestamp,
                    tree_id.clone(),
                    message.clone(),
                ));
            }
            revision_order.truncate(n);

            // Collect all commit_ids and tree_ids we need
            let mut needed_commit_ids: Vec<String> = Vec::new();
            let mut needed_tree_ids: HashSet<String> = HashSet::new();
            for change_id in &revision_order {
                if let Some(commits) = grouped.get(change_id) {
                    for (commit_id, _, tree_id, _) in commits {
                        needed_commit_ids.push(commit_id.clone());
                        needed_tree_ids.insert(tree_id.clone());
                    }
                }
            }

            // 4. Batch-load parents for needed commits
            let mut parents_map: HashMap<String, Vec<(String, i32)>> = HashMap::new();
            if !needed_commit_ids.is_empty() {
                let placeholders: String = needed_commit_ids
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(", ");
                let query = format!(
                    "SELECT commit_id, parent_commit_id, parent_order \
                     FROM commit_parents WHERE commit_id IN ({placeholders}) \
                     ORDER BY commit_id, parent_order"
                );
                let mut q = sqlx::query_as::<_, (String, String, i32)>(&query);
                for id in &needed_commit_ids {
                    q = q.bind(id);
                }
                let parent_rows: Vec<(String, String, i32)> = q.fetch_all(&self.pool).await?;
                for (commit_id, parent_commit_id, parent_order) in parent_rows {
                    parents_map
                        .entry(commit_id)
                        .or_default()
                        .push((parent_commit_id, parent_order));
                }
            }

            // 5. Batch-load trees and blobs for needed tree_ids
            let mut tree_blobs: HashMap<String, HashMap<String, Blob>> = HashMap::new();
            if !needed_tree_ids.is_empty() {
                let tree_ids: Vec<&String> = needed_tree_ids.iter().collect();
                let placeholders: String =
                    tree_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                let query = format!(
                    "SELECT te.tree_id, te.path, b.blob_id, b.content, b.size \
                     FROM tree_entries te \
                     JOIN blobs b ON b.blob_id = te.blob_id \
                     WHERE te.tree_id IN ({placeholders})"
                );
                let mut q = sqlx::query_as::<_, (String, String, String, String, i64)>(&query);
                for id in &tree_ids {
                    q = q.bind(*id);
                }
                let blob_rows: Vec<(String, String, String, String, i64)> =
                    q.fetch_all(&self.pool).await?;
                for (tree_id, path, _blob_id, content, _size) in blob_rows {
                    tree_blobs
                        .entry(tree_id)
                        .or_default()
                        .insert(path, Blob::new(content));
                }
            }

            // 6. Reconstruct: Tree → Commit → Revision
            let mut revisions = Vec::new();
            for change_id in &revision_order {
                let Some(commit_entries) = grouped.get(change_id) else {
                    continue;
                };
                let change_uuid =
                    Uuid::parse_str(change_id).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

                let mut commits = Vec::new();
                let mut is_head = false;
                let mut is_working_copy = false;

                for (commit_id, timestamp, tree_id, message) in commit_entries {
                    let blobs = tree_blobs.get(tree_id).cloned().unwrap_or_default();
                    let tree = Tree::new(blobs);

                    let mut parents: Vec<(String, i32)> =
                        parents_map.get(commit_id).cloned().unwrap_or_default();
                    parents.sort_by_key(|(_, order)| *order);
                    let parent_ids: Vec<String> = parents.into_iter().map(|(id, _)| id).collect();

                    let ts = UNIX_EPOCH + Duration::from_secs(*timestamp as u64);
                    let commit = Commit::restore(
                        commit_id.clone(),
                        change_uuid,
                        parent_ids,
                        tree,
                        message.clone(),
                        ts,
                    );

                    if head_set.contains(commit_id) {
                        is_head = true;
                    }
                    if *commit_id == working_copy_commit_id {
                        is_working_copy = true;
                    }

                    commits.push(commit);
                }

                revisions.push(Revision::new(commits, is_head, is_working_copy));
            }

            Ok(revisions)
        })
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

    // --- log() tests ---

    fn get_root_commit_id(store: &Store) -> String {
        let (commit_id,): (String,) = store
            .runtime
            .block_on(
                sqlx::query_as(
                    "SELECT c.commit_id FROM commits c \
                     WHERE NOT EXISTS (SELECT 1 FROM commit_parents cp WHERE cp.commit_id = c.commit_id)",
                )
                .fetch_one(&store.pool),
            )
            .unwrap();
        commit_id
    }

    fn make_commit_with_parent(
        content: &str,
        change_id: Uuid,
        parents: &[String],
        message: &str,
    ) -> Commit {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        let tree = Tree::new(blobs);
        Commit::new(change_id, parents, tree, message.to_string())
    }

    fn make_change_id(byte: u8) -> Uuid {
        Uuid::from_bytes([
            byte, byte, byte, byte, byte, byte, byte, byte, byte, byte, byte, byte, byte, byte,
            byte, byte,
        ])
    }

    #[test]
    fn test_log_empty_store() {
        let store = Store::init_memory().unwrap();
        let revisions = store.log(20).unwrap();

        // Only the root commit (from init)
        assert_eq!(revisions.len(), 1);
        assert!(revisions[0].is_head());
        assert!(revisions[0].is_working_copy());
        assert!(!revisions[0].is_divergent());
        assert_eq!(revisions[0].commit().message(), "");
    }

    #[test]
    fn test_log_linear_chain() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        // A → B → C
        let a = make_commit_with_parent("a", make_change_id(0x21), &[root_id], "commit A");
        store.save_commit(&a).unwrap();

        let b = make_commit_with_parent(
            "b",
            make_change_id(0x22),
            &[a.commit_id().to_string()],
            "commit B",
        );
        store.save_commit(&b).unwrap();

        let c = make_commit_with_parent(
            "c",
            make_change_id(0x23),
            &[b.commit_id().to_string()],
            "commit C",
        );

        // Create operation with C as head and working copy
        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            c.clone(),
            vec![c.clone()],
        );
        store.save_operation(&op).unwrap();

        let revisions = store.log(20).unwrap();

        // root + A + B + C = 4 revisions
        assert_eq!(revisions.len(), 4);

        // First revision should be the most recent (C)
        assert_eq!(revisions[0].commit().message(), "commit C");
        assert!(revisions[0].is_head());
        assert!(revisions[0].is_working_copy());

        // Second should be B
        assert_eq!(revisions[1].commit().message(), "commit B");
        assert!(!revisions[1].is_head());
        assert!(!revisions[1].is_working_copy());

        // Third should be A
        assert_eq!(revisions[2].commit().message(), "commit A");
        assert!(!revisions[2].is_head());
        assert!(!revisions[2].is_working_copy());

        // Fourth is root
        assert_eq!(revisions[3].commit().message(), "");
    }

    #[test]
    fn test_log_limit() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        // Create a chain of 5 commits
        let mut parent_id = root_id;
        let mut last_commit = None;
        for i in 0..5 {
            let c = make_commit_with_parent(
                &format!("content-{i}"),
                make_change_id(0x30 + i),
                &[parent_id],
                &format!("commit {i}"),
            );
            store.save_commit(&c).unwrap();
            parent_id = c.commit_id().to_string();
            last_commit = Some(c);
        }

        let head = last_commit.unwrap();
        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            head.clone(),
            vec![head],
        );
        store.save_operation(&op).unwrap();

        // log(3) should return only 3 revisions
        let revisions = store.log(3).unwrap();
        assert_eq!(revisions.len(), 3);

        // They should be the 3 most recent
        assert_eq!(revisions[0].commit().message(), "commit 4");
        assert_eq!(revisions[1].commit().message(), "commit 3");
        assert_eq!(revisions[2].commit().message(), "commit 2");
    }

    #[test]
    fn test_log_merge_commit() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        // Create two branches from root, then merge
        let a = make_commit_with_parent("a", make_change_id(0x41), &[root_id.clone()], "branch A");
        store.save_commit(&a).unwrap();

        let b = make_commit_with_parent("b", make_change_id(0x42), &[root_id], "branch B");
        store.save_commit(&b).unwrap();

        let merge = make_commit_with_parent(
            "merged",
            make_change_id(0x43),
            &[a.commit_id().to_string(), b.commit_id().to_string()],
            "merge commit",
        );

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            merge.clone(),
            vec![merge],
        );
        store.save_operation(&op).unwrap();

        let revisions = store.log(20).unwrap();

        // root + A + B + merge = 4 revisions
        assert_eq!(revisions.len(), 4);

        // All ancestors should be present
        let messages: Vec<&str> = revisions.iter().map(|r| r.commit().message()).collect();
        assert!(messages.contains(&"merge commit"));
        assert!(messages.contains(&"branch A"));
        assert!(messages.contains(&"branch B"));
        assert!(messages.contains(&""));

        // Merge commit should have 2 parents
        let merge_rev = revisions
            .iter()
            .find(|r| r.commit().message() == "merge commit")
            .unwrap();
        assert_eq!(merge_rev.commit().parents().len(), 2);
    }

    #[test]
    fn test_log_head_and_working_copy_flags() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let a = make_commit_with_parent("a", make_change_id(0x51), &[root_id.clone()], "commit A");
        store.save_commit(&a).unwrap();

        let b = make_commit_with_parent("b", make_change_id(0x52), &[root_id], "commit B");
        store.save_commit(&b).unwrap();

        // Working copy is A, but both A and B are heads
        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone(), b.clone()],
        );
        store.save_operation(&op).unwrap();

        let revisions = store.log(20).unwrap();

        let rev_a = revisions
            .iter()
            .find(|r| r.commit().message() == "commit A")
            .unwrap();
        assert!(rev_a.is_head());
        assert!(rev_a.is_working_copy());

        let rev_b = revisions
            .iter()
            .find(|r| r.commit().message() == "commit B")
            .unwrap();
        assert!(rev_b.is_head());
        assert!(!rev_b.is_working_copy());

        // Root is not a head in this view (it has descendants that are heads)
        let rev_root = revisions
            .iter()
            .find(|r| r.commit().message().is_empty())
            .unwrap();
        assert!(!rev_root.is_head());
        assert!(!rev_root.is_working_copy());
    }
}
