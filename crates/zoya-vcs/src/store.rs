use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::revision::Revision;
use crate::view::View;
use crate::{Commit, Operation, Tree};

pub enum RevisionQuery<'a> {
    WorkingCopy,
    ChangeId(&'a str),
    CommitId(&'a str),
}

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
            let root_commit =
                Commit::new(change_id, &[], empty_tree.id().to_string(), String::new());

            let operation_id = Uuid::now_v7();
            let init_op = Operation::new(
                operation_id,
                "init".to_string(),
                root_commit.clone(),
                vec![root_commit],
            );

            let mut tx = self.pool.begin().await?;
            Self::save_commit_with_tx(&mut tx, init_op.working_copy(), Some(&empty_tree)).await?;
            Self::save_operation_with_tx(&mut tx, &init_op).await?;
            tx.commit().await?;

            Ok(())
        })
    }

    pub fn save_commit(&self, commit: &Commit, tree: Option<&Tree>) -> Result<(), sqlx::Error> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await?;
            Self::save_commit_with_tx(&mut tx, commit, tree).await?;
            tx.commit().await?;
            Ok(())
        })
    }

    pub fn save_operation(&self, operation: &Operation) -> Result<(), sqlx::Error> {
        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await?;
            for head in operation.heads() {
                Self::save_commit_with_tx(&mut tx, head, None).await?;
            }
            Self::save_operation_with_tx(&mut tx, operation).await?;
            tx.commit().await?;
            Ok(())
        })
    }

    pub fn view(&self) -> Result<View, sqlx::Error> {
        self.runtime.block_on(async {
            // 1. Load latest view_id and working_copy_commit_id
            let (view_id, working_copy_commit_id): (String, String) = sqlx::query_as(
                "SELECT o.view_id, v.working_copy_commit_id \
                 FROM operations o \
                 JOIN views v ON v.view_id = o.view_id \
                 ORDER BY o.operation_id DESC LIMIT 1",
            )
            .fetch_one(&self.pool)
            .await?;

            // 2. Load head commit_ids
            let head_rows: Vec<(String,)> =
                sqlx::query_as("SELECT commit_id FROM view_heads WHERE view_id = ?")
                    .bind(&view_id)
                    .fetch_all(&self.pool)
                    .await?;
            let head_ids: Vec<String> = head_rows.into_iter().map(|(id,)| id).collect();

            // 3. Collect all unique commit_ids
            let mut all_ids: Vec<String> = vec![working_copy_commit_id.clone()];
            for id in &head_ids {
                if *id != working_copy_commit_id {
                    all_ids.push(id.clone());
                }
            }

            // 4. Batch-load commit metadata
            let placeholders: String = all_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let query = format!(
                "SELECT commit_id, change_id, timestamp, tree_id, message \
                 FROM commits WHERE commit_id IN ({placeholders})"
            );
            let mut q = sqlx::query_as::<_, (String, String, i64, String, String)>(&query);
            for id in &all_ids {
                q = q.bind(id);
            }
            let commit_rows: Vec<(String, String, i64, String, String)> =
                q.fetch_all(&self.pool).await?;

            // 5. Batch-load parents
            let mut parents_map: HashMap<String, Vec<(String, i32)>> = HashMap::new();
            {
                let query = format!(
                    "SELECT commit_id, parent_commit_id, parent_order \
                     FROM commit_parents WHERE commit_id IN ({placeholders}) \
                     ORDER BY commit_id, parent_order"
                );
                let mut q = sqlx::query_as::<_, (String, String, i32)>(&query);
                for id in &all_ids {
                    q = q.bind(id);
                }
                let rows: Vec<(String, String, i32)> = q.fetch_all(&self.pool).await?;
                for (commit_id, parent_commit_id, parent_order) in rows {
                    parents_map
                        .entry(commit_id)
                        .or_default()
                        .push((parent_commit_id, parent_order));
                }
            }

            // 6. Reconstruct Commit objects
            let mut commit_map: HashMap<String, Commit> = HashMap::new();
            for (commit_id, change_id, timestamp, tree_id, message) in &commit_rows {
                let change_uuid =
                    Uuid::parse_str(change_id).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

                let mut parents: Vec<(String, i32)> =
                    parents_map.get(commit_id).cloned().unwrap_or_default();
                parents.sort_by_key(|(_, order)| *order);
                let parent_ids: Vec<String> = parents.into_iter().map(|(id, _)| id).collect();

                let ts = UNIX_EPOCH + Duration::from_secs(*timestamp as u64);
                let commit = Commit::restore(
                    commit_id.clone(),
                    change_uuid,
                    parent_ids,
                    tree_id.clone(),
                    message.clone(),
                    ts,
                );
                commit_map.insert(commit_id.clone(), commit);
            }

            // 8. Build View
            let working_copy = commit_map
                .get(&working_copy_commit_id)
                .cloned()
                .ok_or(sqlx::Error::RowNotFound)?;
            let heads: Vec<Commit> = head_ids
                .iter()
                .filter_map(|id| commit_map.get(id).cloned())
                .collect();

            Ok(View::new(Some(view_id), working_copy, heads))
        })
    }

    async fn save_commit_with_tx(
        conn: &mut SqliteConnection,
        commit: &Commit,
        tree: Option<&Tree>,
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

        // 2. Save tree data if provided
        if let Some(tree) = tree {
            debug_assert_eq!(tree.id(), commit.tree_id());

            // Check if tree already exists (blobs and entries must exist too)
            let (tree_exists,): (bool,) =
                sqlx::query_as("SELECT EXISTS(SELECT 1 FROM trees WHERE tree_id = ?)")
                    .bind(tree.id())
                    .fetch_one(&mut *conn)
                    .await?;

            if !tree_exists {
                // 3. Insert blobs
                for blob in tree.blobs().values() {
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
                    .bind(tree.id())
                    .execute(&mut *conn)
                    .await?;

                // 5. Insert tree entries
                for (path, blob) in tree.blobs() {
                    sqlx::query(
                        "INSERT OR IGNORE INTO tree_entries (tree_id, path, blob_id) VALUES (?, ?, ?)",
                    )
                    .bind(tree.id())
                    .bind(path)
                    .bind(blob.id())
                    .execute(&mut *conn)
                    .await?;
                }
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
        .bind(commit.tree_id())
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

    pub fn describe(&self, query: RevisionQuery, message: String) -> Result<(), sqlx::Error> {
        let resolved = self.revision(query)?;
        let commit_id = resolved.commit().commit_id();
        let view = self.view()?;
        let working_copy_commit_id = view.working_copy().commit_id().to_string();
        let head_ids: Vec<String> = view
            .heads()
            .iter()
            .map(|c| c.commit_id().to_string())
            .collect();

        self.runtime.block_on(async {
            let head_placeholders: String = head_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

            // Load target commit metadata
            let (change_id, tree_id, existing_message): (String, String, String) = sqlx::query_as(
                "SELECT change_id, tree_id, message FROM commits WHERE commit_id = ?",
            )
            .bind(commit_id)
            .fetch_one(&self.pool)
            .await?;

            // 4. Early return if message unchanged
            if existing_message == message {
                return Ok(());
            }

            let parent_rows: Vec<(String, i32)> = sqlx::query_as(
                "SELECT parent_commit_id, parent_order FROM commit_parents WHERE commit_id = ? ORDER BY parent_order",
            )
            .bind(commit_id)
            .fetch_all(&self.pool)
            .await?;
            let target_parents: Vec<String> = parent_rows.into_iter().map(|(id, _)| id).collect();

            let mut tx = self.pool.begin().await?;

            // 5. Create rewritten commit with new message
            let change_uuid =
                Uuid::parse_str(&change_id).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            let new_commit = Commit::new(
                change_uuid,
                &target_parents,
                tree_id.clone(),
                message.clone(),
            );
            Self::save_commit_with_tx(&mut tx, &new_commit, None).await?;

            // 6. Find descendants via forward walk
            // First, gather all ancestors in view for scoping
            let descendant_query = format!(
                "WITH RECURSIVE ancestors AS ( \
                     SELECT c.commit_id \
                     FROM commits c \
                     WHERE c.commit_id IN ({head_placeholders}) \
                     UNION \
                     SELECT cp.parent_commit_id \
                     FROM ancestors a \
                     JOIN commit_parents cp ON cp.commit_id = a.commit_id \
                 ) \
                 SELECT cp.commit_id, c.change_id, c.tree_id, c.message \
                 FROM commit_parents cp \
                 JOIN commits c ON c.commit_id = cp.commit_id \
                 WHERE cp.parent_commit_id = ? \
                 AND cp.commit_id IN (SELECT commit_id FROM ancestors)"
            );

            // BFS to find all descendants
            struct DescendantInfo {
                commit_id: String,
                change_id: String,
                tree_id: String,
                message: String,
                parents: Vec<String>,
            }

            let mut old_to_new: HashMap<String, Commit> = HashMap::new();
            old_to_new.insert(commit_id.to_string(), new_commit.clone());

            // Collect all descendants with their info
            let mut descendants: Vec<DescendantInfo> = Vec::new();
            let mut descendant_set: HashSet<String> = HashSet::new();
            let mut queue: VecDeque<String> = VecDeque::new();
            queue.push_back(commit_id.to_string());

            while let Some(parent) = queue.pop_front() {
                // Find children of this parent within the view
                let mut q = sqlx::query_as::<_, (String, String, String, String)>(&descendant_query);
                for head_id in &head_ids {
                    q = q.bind(head_id);
                }
                q = q.bind(&parent);
                let children: Vec<(String, String, String, String)> = q.fetch_all(&mut *tx).await?;

                for (child_id, child_change_id, child_tree_id, child_message) in children {
                    if descendant_set.insert(child_id.clone()) {
                        // Load this child's parents
                        let child_parent_rows: Vec<(String, i32)> = sqlx::query_as(
                            "SELECT parent_commit_id, parent_order FROM commit_parents WHERE commit_id = ? ORDER BY parent_order",
                        )
                        .bind(&child_id)
                        .fetch_all(&mut *tx)
                        .await?;
                        let child_parents: Vec<String> =
                            child_parent_rows.into_iter().map(|(id, _)| id).collect();

                        descendants.push(DescendantInfo {
                            commit_id: child_id.clone(),
                            change_id: child_change_id,
                            tree_id: child_tree_id,
                            message: child_message,
                            parents: child_parents,
                        });
                        queue.push_back(child_id);
                    }
                }
            }

            // 7. Topological sort (Kahn's algorithm)
            if !descendants.is_empty() {
                // Build in-degree map within descendant set
                let desc_ids: HashSet<&str> = descendants.iter().map(|d| d.commit_id.as_str()).collect();
                let mut in_degree: HashMap<&str, usize> = HashMap::new();
                let mut dependents: HashMap<&str, Vec<usize>> = HashMap::new();

                for (i, d) in descendants.iter().enumerate() {
                    let deg = d
                        .parents
                        .iter()
                        .filter(|p| desc_ids.contains(p.as_str()))
                        .count();
                    in_degree.insert(&d.commit_id, deg);
                    for p in &d.parents {
                        if desc_ids.contains(p.as_str()) {
                            dependents.entry(p.as_str()).or_default().push(i);
                        }
                    }
                }

                let mut topo_order: Vec<usize> = Vec::new();
                let mut ready: VecDeque<usize> = VecDeque::new();
                for (i, d) in descendants.iter().enumerate() {
                    if in_degree[d.commit_id.as_str()] == 0 {
                        ready.push_back(i);
                    }
                }

                while let Some(idx) = ready.pop_front() {
                    topo_order.push(idx);
                    let cid = descendants[idx].commit_id.as_str();
                    if let Some(deps) = dependents.get(cid) {
                        for &dep_idx in deps {
                            let dep_cid = descendants[dep_idx].commit_id.as_str();
                            let deg = in_degree.get_mut(dep_cid).unwrap();
                            *deg -= 1;
                            if *deg == 0 {
                                ready.push_back(dep_idx);
                            }
                        }
                    }
                }

                // Rewrite descendants in topological order
                for idx in topo_order {
                    let d = &descendants[idx];
                    let remapped_parents: Vec<String> = d
                        .parents
                        .iter()
                        .map(|p| {
                            old_to_new
                                .get(p)
                                .map(|c| c.commit_id().to_string())
                                .unwrap_or_else(|| p.clone())
                        })
                        .collect();

                    let desc_change_uuid = Uuid::parse_str(&d.change_id)
                        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
                    let new_desc_commit = Commit::new(
                        desc_change_uuid,
                        &remapped_parents,
                        d.tree_id.clone(),
                        d.message.clone(),
                    );
                    Self::save_commit_with_tx(&mut tx, &new_desc_commit, None).await?;

                    old_to_new.insert(d.commit_id.clone(), new_desc_commit);
                }
            }

            // 8. Apply mapping to heads and working_copy, create operation
            let new_working_copy = old_to_new
                .get(&working_copy_commit_id)
                .cloned()
                .unwrap_or_else(|| view.working_copy().clone());
            let new_heads: Vec<Commit> = view
                .heads()
                .iter()
                .map(|h| {
                    old_to_new
                        .get(h.commit_id())
                        .cloned()
                        .unwrap_or_else(|| h.clone())
                })
                .collect();

            let operation = Operation::new(
                Uuid::now_v7(),
                "describe".to_string(),
                new_working_copy,
                new_heads,
            );
            Self::save_operation_with_tx(&mut tx, &operation).await?;

            tx.commit().await?;
            Ok(())
        })
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new(&self, query: RevisionQuery) -> Result<(), sqlx::Error> {
        let resolved = self.revision(query)?;
        let view = self.view()?;

        let new_commit = Commit::new(
            Uuid::now_v7(),
            &[resolved.commit().commit_id().to_string()],
            resolved.commit().tree_id().to_string(),
            String::new(),
        );

        let new_heads: Vec<Commit> = if resolved.is_head() {
            view.heads()
                .iter()
                .map(|h| {
                    if h.commit_id() == resolved.commit().commit_id() {
                        new_commit.clone()
                    } else {
                        h.clone()
                    }
                })
                .collect()
        } else {
            let mut heads = view.heads().to_vec();
            heads.push(new_commit.clone());
            heads
        };

        let operation = Operation::new(
            Uuid::now_v7(),
            "new".to_string(),
            new_commit.clone(),
            new_heads,
        );

        self.runtime.block_on(async {
            let mut tx = self.pool.begin().await?;
            Self::save_commit_with_tx(&mut tx, &new_commit, None).await?;
            Self::save_operation_with_tx(&mut tx, &operation).await?;
            tx.commit().await?;
            Ok(())
        })
    }

    pub fn log(&self, n: usize) -> Result<Vec<Revision>, sqlx::Error> {
        let view = self.view()?;
        let view_id = view.view_id().to_string();

        self.runtime.block_on(async {
            // 1. Recursive CTE to walk ancestor graph
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

            // Collect all commit_ids we need
            let mut needed_commit_ids: Vec<String> = Vec::new();
            for change_id in &revision_order {
                if let Some(commits) = grouped.get(change_id) {
                    for (commit_id, _, _, _) in commits {
                        needed_commit_ids.push(commit_id.clone());
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

            // 5. Reconstruct Commit → Revision
            let mut revisions = Vec::new();
            for change_id in &revision_order {
                let Some(commit_entries) = grouped.get(change_id) else {
                    continue;
                };
                let change_uuid =
                    Uuid::parse_str(change_id).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

                let mut commits = Vec::new();

                for (commit_id, timestamp, tree_id, message) in commit_entries {
                    let mut parents: Vec<(String, i32)> =
                        parents_map.get(commit_id).cloned().unwrap_or_default();
                    parents.sort_by_key(|(_, order)| *order);
                    let parent_ids: Vec<String> = parents.into_iter().map(|(id, _)| id).collect();

                    let ts = UNIX_EPOCH + Duration::from_secs(*timestamp as u64);
                    let commit = Commit::restore(
                        commit_id.clone(),
                        change_uuid,
                        parent_ids,
                        tree_id.clone(),
                        message.clone(),
                        ts,
                    );

                    commits.push(commit);
                }

                revisions.push(Revision::new(commits, &view));
            }

            Ok(revisions)
        })
    }
    pub fn revision(&self, query: RevisionQuery) -> Result<Revision, sqlx::Error> {
        let view = self.view()?;
        let working_copy_commit_id = view.working_copy().commit_id().to_string();
        let head_ids: Vec<String> = view
            .heads()
            .iter()
            .map(|c| c.commit_id().to_string())
            .collect();
        let head_placeholders: String = head_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

        self.runtime.block_on(async {
            // 1. Resolve query to change_id
            let resolved: Vec<(String, String)> = match query {
                RevisionQuery::WorkingCopy => {
                    let (change_id,): (String,) =
                        sqlx::query_as("SELECT change_id FROM commits WHERE commit_id = ?")
                            .bind(&working_copy_commit_id)
                            .fetch_one(&self.pool)
                            .await?;
                    vec![(working_copy_commit_id.clone(), change_id)]
                }
                RevisionQuery::CommitId(prefix) => {
                    let query_str = format!(
                        "WITH RECURSIVE ancestors AS ( \
                             SELECT c.commit_id, c.change_id \
                             FROM commits c \
                             WHERE c.commit_id IN ({head_placeholders}) \
                             UNION \
                             SELECT c.commit_id, c.change_id \
                             FROM ancestors a \
                             JOIN commit_parents cp ON cp.commit_id = a.commit_id \
                             JOIN commits c ON c.commit_id = cp.parent_commit_id \
                         ) \
                         SELECT commit_id, change_id FROM ancestors \
                         WHERE commit_id LIKE ? || '%'"
                    );
                    let mut q = sqlx::query_as::<_, (String, String)>(&query_str);
                    for head_id in &head_ids {
                        q = q.bind(head_id);
                    }
                    q = q.bind(prefix);
                    q.fetch_all(&self.pool).await?
                }
                RevisionQuery::ChangeId(prefix) => {
                    let query_str = format!(
                        "WITH RECURSIVE ancestors AS ( \
                             SELECT c.commit_id, c.change_id \
                             FROM commits c \
                             WHERE c.commit_id IN ({head_placeholders}) \
                             UNION \
                             SELECT c.commit_id, c.change_id \
                             FROM ancestors a \
                             JOIN commit_parents cp ON cp.commit_id = a.commit_id \
                             JOIN commits c ON c.commit_id = cp.parent_commit_id \
                         ) \
                         SELECT commit_id, change_id FROM ancestors \
                         WHERE change_id LIKE ? || '%'"
                    );
                    let mut q = sqlx::query_as::<_, (String, String)>(&query_str);
                    for head_id in &head_ids {
                        q = q.bind(head_id);
                    }
                    q = q.bind(prefix);
                    q.fetch_all(&self.pool).await?
                }
            };

            if resolved.is_empty() {
                return Err(sqlx::Error::RowNotFound);
            }

            let distinct_change_ids: HashSet<&str> =
                resolved.iter().map(|(_, cid)| cid.as_str()).collect();
            if distinct_change_ids.len() > 1 {
                return Err(sqlx::Error::Protocol("ambiguous prefix".to_string()));
            }

            let change_id = resolved[0].1.clone();

            // 3. Find all commits with this change_id in the ancestor graph
            let all_query = format!(
                "WITH RECURSIVE ancestors AS ( \
                     SELECT c.commit_id \
                     FROM commits c \
                     WHERE c.commit_id IN ({head_placeholders}) \
                     UNION \
                     SELECT cp.parent_commit_id \
                     FROM ancestors a \
                     JOIN commit_parents cp ON cp.commit_id = a.commit_id \
                 ) \
                 SELECT c.commit_id, c.timestamp, c.tree_id, c.message \
                 FROM commits c \
                 WHERE c.change_id = ? \
                 AND c.commit_id IN (SELECT commit_id FROM ancestors)"
            );
            let mut q = sqlx::query_as::<_, (String, i64, String, String)>(&all_query);
            for head_id in &head_ids {
                q = q.bind(head_id);
            }
            q = q.bind(&change_id);
            let commit_rows: Vec<(String, i64, String, String)> = q.fetch_all(&self.pool).await?;

            let commit_ids: Vec<String> = commit_rows.iter().map(|(id, ..)| id.clone()).collect();

            // Batch-load parents
            let mut parents_map: HashMap<String, Vec<(String, i32)>> = HashMap::new();
            if !commit_ids.is_empty() {
                let placeholders: String = commit_ids
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
                for id in &commit_ids {
                    q = q.bind(id);
                }
                let rows: Vec<(String, String, i32)> = q.fetch_all(&self.pool).await?;
                for (cid, pid, order) in rows {
                    parents_map.entry(cid).or_default().push((pid, order));
                }
            }

            // Reconstruct commits
            let change_uuid =
                Uuid::parse_str(&change_id).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            let mut commits = Vec::new();

            for (commit_id, timestamp, tree_id, message) in &commit_rows {
                let mut parents: Vec<(String, i32)> =
                    parents_map.get(commit_id).cloned().unwrap_or_default();
                parents.sort_by_key(|(_, order)| *order);
                let parent_ids: Vec<String> = parents.into_iter().map(|(id, _)| id).collect();

                let ts = UNIX_EPOCH + Duration::from_secs(*timestamp as u64);
                let commit = Commit::restore(
                    commit_id.clone(),
                    change_uuid,
                    parent_ids,
                    tree_id.clone(),
                    message.clone(),
                    ts,
                );

                commits.push(commit);
            }

            Ok(Revision::new(commits, &view))
        })
    }

    pub fn parents(&self, revision: &Revision) -> Result<Vec<Revision>, sqlx::Error> {
        let view = self.view()?;

        // Collect parent commit IDs from all commits in the revision
        let commit_ids: HashSet<String> = revision
            .commits()
            .iter()
            .map(|c| c.commit_id().to_string())
            .collect();
        let parent_commit_ids: Vec<String> = revision
            .commits()
            .iter()
            .flat_map(|c| c.parents())
            .filter(|pid| !commit_ids.contains(*pid))
            .map(|pid| pid.to_string())
            .collect::<HashSet<String>>()
            .into_iter()
            .collect();

        if parent_commit_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.runtime.block_on(async {
            let placeholders: String = parent_commit_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");

            // Load parent commit data
            let query = format!(
                "SELECT commit_id, change_id, timestamp, tree_id, message \
                 FROM commits WHERE commit_id IN ({placeholders})"
            );
            let mut q = sqlx::query_as::<_, (String, String, i64, String, String)>(&query);
            for id in &parent_commit_ids {
                q = q.bind(id);
            }
            let parent_rows: Vec<(String, String, i64, String, String)> =
                q.fetch_all(&self.pool).await?;

            // Load parents of parent commits
            let mut pp_map: HashMap<String, Vec<(String, i32)>> = HashMap::new();
            {
                let query = format!(
                    "SELECT commit_id, parent_commit_id, parent_order \
                     FROM commit_parents WHERE commit_id IN ({placeholders}) \
                     ORDER BY commit_id, parent_order"
                );
                let mut q = sqlx::query_as::<_, (String, String, i32)>(&query);
                for id in &parent_commit_ids {
                    q = q.bind(id);
                }
                let rows: Vec<(String, String, i32)> = q.fetch_all(&self.pool).await?;
                for (cid, pid, order) in rows {
                    pp_map.entry(cid).or_default().push((pid, order));
                }
            }

            // Group by change_id and build parent revisions
            let mut grouped: HashMap<String, Vec<(String, i64, String, String)>> = HashMap::new();
            let mut change_order: Vec<String> = Vec::new();
            for (cid, change_id, ts, tid, msg) in &parent_rows {
                if !grouped.contains_key(change_id) {
                    change_order.push(change_id.clone());
                }
                grouped.entry(change_id.clone()).or_default().push((
                    cid.clone(),
                    *ts,
                    tid.clone(),
                    msg.clone(),
                ));
            }

            let mut parent_revisions = Vec::new();
            for p_change_id in &change_order {
                let Some(entries) = grouped.get(p_change_id) else {
                    continue;
                };
                let p_change_uuid = Uuid::parse_str(p_change_id)
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

                let mut p_commits = Vec::new();

                for (cid, ts, tid, msg) in entries {
                    let mut parents: Vec<(String, i32)> =
                        pp_map.get(cid).cloned().unwrap_or_default();
                    parents.sort_by_key(|(_, order)| *order);
                    let parent_ids: Vec<String> = parents.into_iter().map(|(id, _)| id).collect();

                    let timestamp = UNIX_EPOCH + Duration::from_secs(*ts as u64);
                    let commit = Commit::restore(
                        cid.clone(),
                        p_change_uuid,
                        parent_ids,
                        tid.clone(),
                        msg.clone(),
                        timestamp,
                    );

                    p_commits.push(commit);
                }

                parent_revisions.push(Revision::new(p_commits, &view));
            }

            Ok(parent_revisions)
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

    fn make_commit(content: &str, change_id: Uuid) -> (Commit, Tree) {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        let tree = Tree::new(blobs);
        let commit = Commit::new(
            change_id,
            &[],
            tree.id().to_string(),
            "test commit".to_string(),
        );
        (commit, tree)
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
        let (commit, tree) = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit, Some(&tree)).unwrap();

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
        let (commit, tree) = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit, Some(&tree)).unwrap();
        store.save_commit(&commit, Some(&tree)).unwrap();

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
        let (commit, tree) = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit, Some(&tree)).unwrap();

        let (blob_id, content, size): (String, String, i64) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT blob_id, content, size FROM blobs WHERE content = ?")
                    .bind("hello world")
                    .fetch_one(&store.pool),
            )
            .unwrap();

        let expected_blob = tree.get("root").unwrap();
        assert_eq!(blob_id, expected_blob.id());
        assert_eq!(content, "hello world");
        assert_eq!(size, 11);
    }

    #[test]
    fn test_save_commit_stores_tree_entries() {
        let store = Store::init_memory().unwrap();
        let (commit, tree) = make_commit("hello world", CHANGE_1);

        store.save_commit(&commit, Some(&tree)).unwrap();

        let (tree_id, path, blob_id): (String, String, String) = store
            .runtime
            .block_on(
                sqlx::query_as("SELECT tree_id, path, blob_id FROM tree_entries WHERE tree_id = ?")
                    .bind(tree.id())
                    .fetch_one(&store.pool),
            )
            .unwrap();

        assert_eq!(tree_id, tree.id());
        assert_eq!(path, "root");
        assert_eq!(blob_id, tree.get("root").unwrap().id());
    }

    #[test]
    fn test_save_commit_with_parent() {
        let store = Store::init_memory().unwrap();

        let (parent, parent_tree) = make_commit("v1", CHANGE_1);
        store.save_commit(&parent, Some(&parent_tree)).unwrap();

        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new("v2".to_string()));
        let child_tree = Tree::new(blobs);
        let child = Commit::new(
            CHANGE_2,
            &[parent.commit_id().to_string()],
            child_tree.id().to_string(),
            "child commit".to_string(),
        );
        store.save_commit(&child, Some(&child_tree)).unwrap();

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

        let (commit, tree) = make_commit("hello world", CHANGE_1);
        store.save_commit(&commit, Some(&tree)).unwrap();
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
    ) -> (Commit, Tree) {
        let mut blobs = HashMap::new();
        blobs.insert("root".to_string(), Blob::new(content.to_string()));
        let tree = Tree::new(blobs);
        let commit = Commit::new(
            change_id,
            parents,
            tree.id().to_string(),
            message.to_string(),
        );
        (commit, tree)
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
        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x21), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) = make_commit_with_parent(
            "b",
            make_change_id(0x22),
            &[a.commit_id().to_string()],
            "commit B",
        );
        store.save_commit(&b, Some(&b_tree)).unwrap();

        let (c, c_tree) = make_commit_with_parent(
            "c",
            make_change_id(0x23),
            &[b.commit_id().to_string()],
            "commit C",
        );
        store.save_commit(&c, Some(&c_tree)).unwrap();

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
            let (c, c_tree) = make_commit_with_parent(
                &format!("content-{i}"),
                make_change_id(0x30 + i),
                &[parent_id],
                &format!("commit {i}"),
            );
            store.save_commit(&c, Some(&c_tree)).unwrap();
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
        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x41), &[root_id.clone()], "branch A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) =
            make_commit_with_parent("b", make_change_id(0x42), &[root_id], "branch B");
        store.save_commit(&b, Some(&b_tree)).unwrap();

        let (merge, merge_tree) = make_commit_with_parent(
            "merged",
            make_change_id(0x43),
            &[a.commit_id().to_string(), b.commit_id().to_string()],
            "merge commit",
        );
        store.save_commit(&merge, Some(&merge_tree)).unwrap();

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

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x51), &[root_id.clone()], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) =
            make_commit_with_parent("b", make_change_id(0x52), &[root_id], "commit B");
        store.save_commit(&b, Some(&b_tree)).unwrap();

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

    // --- describe() tests ---

    fn setup_linear_chain(store: &Store) -> (String, Commit, Commit, Commit) {
        let root_id = get_root_commit_id(store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x61), &[root_id.clone()], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) = make_commit_with_parent(
            "b",
            make_change_id(0x62),
            &[a.commit_id().to_string()],
            "commit B",
        );
        store.save_commit(&b, Some(&b_tree)).unwrap();

        let (c, c_tree) = make_commit_with_parent(
            "c",
            make_change_id(0x63),
            &[b.commit_id().to_string()],
            "commit C",
        );
        store.save_commit(&c, Some(&c_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            c.clone(),
            vec![c.clone()],
        );
        store.save_operation(&op).unwrap();

        (root_id, a, b, c)
    }

    #[test]
    fn test_describe_changes_message() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x71), &[root_id], "old message");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        store
            .describe(
                RevisionQuery::CommitId(a.commit_id()),
                "new message".to_string(),
            )
            .unwrap();

        let revisions = store.log(20).unwrap();
        let rev = revisions
            .iter()
            .find(|r| r.commit().message() == "new message")
            .unwrap();
        assert!(rev.is_head());
        assert!(rev.is_working_copy());

        // Old message should not appear
        assert!(
            revisions
                .iter()
                .all(|r| r.commit().message() != "old message")
        );
    }

    #[test]
    fn test_describe_no_op_same_message() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x71), &[root_id], "same message");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        // Count operations before
        let (op_count_before,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM operations").fetch_one(&store.pool))
            .unwrap();

        store
            .describe(
                RevisionQuery::CommitId(a.commit_id()),
                "same message".to_string(),
            )
            .unwrap();

        // Count operations after — should be unchanged
        let (op_count_after,): (i64,) = store
            .runtime
            .block_on(sqlx::query_as("SELECT COUNT(*) FROM operations").fetch_one(&store.pool))
            .unwrap();
        assert_eq!(op_count_before, op_count_after);
    }

    #[test]
    fn test_describe_rewrites_descendants() {
        let store = Store::init_memory().unwrap();
        let (_root_id, a, b, c) = setup_linear_chain(&store);

        let old_b_id = b.commit_id().to_string();
        let old_c_id = c.commit_id().to_string();

        // Describe A with a new message
        store
            .describe(
                RevisionQuery::CommitId(a.commit_id()),
                "updated A".to_string(),
            )
            .unwrap();

        let revisions = store.log(20).unwrap();
        let messages: Vec<&str> = revisions.iter().map(|r| r.commit().message()).collect();

        // A's message should be updated
        assert!(messages.contains(&"updated A"));
        assert!(!messages.contains(&"commit A"));

        // B and C should still have their original messages
        assert!(messages.contains(&"commit B"));
        assert!(messages.contains(&"commit C"));

        // But B and C should have new commit_ids (they were rewritten)
        let rev_b = revisions
            .iter()
            .find(|r| r.commit().message() == "commit B")
            .unwrap();
        assert_ne!(rev_b.commit().commit_id(), old_b_id);

        let rev_c = revisions
            .iter()
            .find(|r| r.commit().message() == "commit C")
            .unwrap();
        assert_ne!(rev_c.commit().commit_id(), old_c_id);

        // C should still be head and working copy
        assert!(rev_c.is_head());
        assert!(rev_c.is_working_copy());
    }

    #[test]
    fn test_describe_updates_heads_and_working_copy() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x71), &[root_id], "head commit");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        let old_commit_id = a.commit_id().to_string();

        store
            .describe(
                RevisionQuery::CommitId(a.commit_id()),
                "renamed head".to_string(),
            )
            .unwrap();

        let revisions = store.log(20).unwrap();
        let head_rev = revisions
            .iter()
            .find(|r| r.commit().message() == "renamed head")
            .unwrap();

        // The head should have a new commit_id
        assert_ne!(head_rev.commit().commit_id(), old_commit_id);
        assert!(head_rev.is_head());
        assert!(head_rev.is_working_copy());

        // Old commit_id should not appear as a head
        assert!(
            revisions
                .iter()
                .all(|r| r.commit().commit_id() != old_commit_id)
        );
    }

    // --- revision() tests ---

    #[test]
    fn test_revision_working_copy() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x81), &[root_id.clone()], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        let rev = store.revision(RevisionQuery::WorkingCopy).unwrap();
        assert_eq!(rev.commit().message(), "commit A");
        assert!(rev.is_working_copy());
        assert!(rev.is_head());

        // Parent should be root
        let parents = store.parents(&rev).unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].commit().message(), "");
    }

    #[test]
    fn test_revision_by_commit_id() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x82), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        let rev = store
            .revision(RevisionQuery::CommitId(a.commit_id()))
            .unwrap();
        assert_eq!(rev.commit().commit_id(), a.commit_id());
        assert_eq!(rev.commit().message(), "commit A");
    }

    #[test]
    fn test_revision_by_commit_id_prefix() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x83), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        // Use first 8 characters as prefix
        let prefix = &a.commit_id()[..8];
        let rev = store.revision(RevisionQuery::CommitId(prefix)).unwrap();
        assert_eq!(rev.commit().commit_id(), a.commit_id());
    }

    #[test]
    fn test_revision_by_change_id() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let change = make_change_id(0x84);
        let (a, a_tree) = make_commit_with_parent("a", change, &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        let rev = store
            .revision(RevisionQuery::ChangeId(&change.to_string()))
            .unwrap();
        assert_eq!(rev.change_id(), change);
        assert_eq!(rev.commit().message(), "commit A");
    }

    #[test]
    fn test_revision_by_change_id_prefix() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let change = make_change_id(0x85);
        let (a, a_tree) = make_commit_with_parent("a", change, &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        // change_id = "85858585-8585-8585-8585-858585858585", prefix "8585"
        let rev = store.revision(RevisionQuery::ChangeId("8585")).unwrap();
        assert_eq!(rev.change_id(), change);
    }

    #[test]
    fn test_revision_ambiguous_prefix() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        // Two commits with change_ids sharing prefix "fa"
        let change_a = make_change_id(0xFA);
        let change_b = make_change_id(0xFB);
        let (a, a_tree) = make_commit_with_parent("a", change_a, &[root_id.clone()], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) = make_commit_with_parent("b", change_b, &[root_id], "commit B");
        store.save_commit(&b, Some(&b_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone(), b.clone()],
        );
        store.save_operation(&op).unwrap();

        // Prefix "f" matches both change_ids
        let result = store.revision(RevisionQuery::ChangeId("f"));
        assert!(result.is_err());
        match result.unwrap_err() {
            sqlx::Error::Protocol(msg) => assert_eq!(msg, "ambiguous prefix"),
            other => panic!("expected Protocol error, got: {other:?}"),
        }
    }

    #[test]
    fn test_revision_not_found() {
        let store = Store::init_memory().unwrap();

        let result = store.revision(RevisionQuery::CommitId("nonexistent"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), sqlx::Error::RowNotFound));
    }

    #[test]
    fn test_revision_parents() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        // A → B → C
        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x86), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) = make_commit_with_parent(
            "b",
            make_change_id(0x87),
            &[a.commit_id().to_string()],
            "commit B",
        );
        store.save_commit(&b, Some(&b_tree)).unwrap();

        let (c, c_tree) = make_commit_with_parent(
            "c",
            make_change_id(0x88),
            &[b.commit_id().to_string()],
            "commit C",
        );
        store.save_commit(&c, Some(&c_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            c.clone(),
            vec![c.clone()],
        );
        store.save_operation(&op).unwrap();

        let rev = store
            .revision(RevisionQuery::CommitId(c.commit_id()))
            .unwrap();
        assert_eq!(rev.commit().message(), "commit C");

        // C's parent is B
        let parents = store.parents(&rev).unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].commit().message(), "commit B");
    }

    #[test]
    fn test_revision_root_has_no_parents() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let rev = store.revision(RevisionQuery::CommitId(&root_id)).unwrap();
        assert_eq!(rev.commit().message(), "");
        let parents = store.parents(&rev).unwrap();
        assert!(parents.is_empty());
    }

    // --- view() tests ---

    #[test]
    fn test_view_initial() {
        let store = Store::init_memory().unwrap();
        let view = store.view().unwrap();

        // Root commit is both working copy and single head
        assert_eq!(view.working_copy().message(), "");
        assert!(view.working_copy().parents().is_empty());
        assert_eq!(view.heads().len(), 1);
        assert_eq!(view.heads()[0].commit_id(), view.working_copy().commit_id());
        assert!(!view.view_id().is_empty());
    }

    #[test]
    fn test_view_after_operation() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x91), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();
        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        let view = store.view().unwrap();

        assert_eq!(view.working_copy().commit_id(), a.commit_id());
        assert_eq!(view.working_copy().message(), "commit A");
        assert_eq!(view.heads().len(), 1);
        assert_eq!(view.heads()[0].commit_id(), a.commit_id());
        assert_eq!(view.view_id(), op.view_id());
    }

    #[test]
    fn test_view_multiple_heads() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x92), &[root_id.clone()], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) =
            make_commit_with_parent("b", make_change_id(0x93), &[root_id], "commit B");
        store.save_commit(&b, Some(&b_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone(), b.clone()],
        );
        store.save_operation(&op).unwrap();

        let view = store.view().unwrap();

        assert_eq!(view.working_copy().commit_id(), a.commit_id());
        assert_eq!(view.heads().len(), 2);

        let head_ids: Vec<&str> = view.heads().iter().map(|c| c.commit_id()).collect();
        assert!(head_ids.contains(&a.commit_id()));
        assert!(head_ids.contains(&b.commit_id()));
    }

    // --- new() tests ---

    #[test]
    fn test_new_on_working_copy() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0xA1), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        store.new(RevisionQuery::WorkingCopy).unwrap();

        let rev = store.revision(RevisionQuery::WorkingCopy).unwrap();
        assert_eq!(rev.commit().message(), "");
        assert!(rev.is_head());
        assert!(rev.is_working_copy());

        // Parent of new working copy is A
        let parents = store.parents(&rev).unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].commit().message(), "commit A");

        // View should have 1 head (A replaced by new commit)
        let view = store.view().unwrap();
        assert_eq!(view.heads().len(), 1);
        assert_eq!(view.heads()[0].commit_id(), rev.commit().commit_id());
    }

    #[test]
    fn test_new_on_non_head() {
        let store = Store::init_memory().unwrap();
        let (_root_id, a, _b, c) = setup_linear_chain(&store);

        // C is working copy and head; A is not a head
        store
            .new(RevisionQuery::CommitId(a.commit_id()))
            .unwrap();

        let rev = store.revision(RevisionQuery::WorkingCopy).unwrap();
        assert_eq!(rev.commit().message(), "");
        assert!(rev.is_head());
        assert!(rev.is_working_copy());

        // Parent of new working copy is A
        let parents = store.parents(&rev).unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].commit().message(), "commit A");

        // View should have 2 heads: C (still a head) + new commit
        let view = store.view().unwrap();
        assert_eq!(view.heads().len(), 2);
        let head_ids: Vec<&str> = view.heads().iter().map(|h| h.commit_id()).collect();
        assert!(head_ids.contains(&rev.commit().commit_id()));
        // C should still be a head (find it by its change_id since commit_id may differ)
        let rev_c = store
            .revision(RevisionQuery::ChangeId(&c.change_id().to_string()))
            .unwrap();
        assert!(rev_c.is_head());
    }

    #[test]
    fn test_new_preserves_tree() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0xA3), &[root_id], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone()],
        );
        store.save_operation(&op).unwrap();

        store.new(RevisionQuery::WorkingCopy).unwrap();

        let rev = store.revision(RevisionQuery::WorkingCopy).unwrap();
        assert_eq!(rev.commit().tree_id(), a.tree_id());
    }

    #[test]
    fn test_view_working_copy_in_heads() {
        let store = Store::init_memory().unwrap();
        let root_id = get_root_commit_id(&store);

        let (a, a_tree) =
            make_commit_with_parent("a", make_change_id(0x94), &[root_id.clone()], "commit A");
        store.save_commit(&a, Some(&a_tree)).unwrap();

        let (b, b_tree) =
            make_commit_with_parent("b", make_change_id(0x95), &[root_id], "commit B");
        store.save_commit(&b, Some(&b_tree)).unwrap();

        // Working copy is A, heads include both A and B
        let op = Operation::new(
            Uuid::now_v7(),
            "snapshot".to_string(),
            a.clone(),
            vec![a.clone(), b.clone()],
        );
        store.save_operation(&op).unwrap();

        let view = store.view().unwrap();

        // Working copy is present in heads (caller passes it explicitly)
        let head_ids: Vec<&str> = view.heads().iter().map(|c| c.commit_id()).collect();
        assert!(head_ids.contains(&view.working_copy().commit_id()));

        // Full commit data is present on working copy
        assert_eq!(view.working_copy().message(), "commit A");
        assert_eq!(view.working_copy().change_id(), make_change_id(0x94));
    }
}
