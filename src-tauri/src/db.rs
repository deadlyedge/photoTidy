use std::convert::TryFrom;
use std::time::Duration;

use crate::config::{AppConfig, SCHEMA_VERSION};
use crate::error::{AppError, Result};
use parking_lot::{Mutex, MutexGuard};
use rusqlite::{params, Connection};

const DB_VERSION: i32 = 3;

#[derive(Debug, Clone)]
pub struct InventoryRecord {
    pub id: Option<i64>,
    pub file_hash: String,
    pub blake3_hash: Option<String>,
    pub file_size: u64,
    pub file_name: String,
    pub relative_path: String,
    pub captured_at: Option<String>,
    pub modified_at: String,
    pub exif_model: Option<String>,
    pub exif_make: Option<String>,
    pub exif_artist: Option<String>,
    pub is_duplicate: bool,
}

#[derive(Debug, Clone)]
pub struct PlanRecord {
    pub id: i64,
    pub file_hash: String,
    pub file_size: u64,
    pub origin_file_name: String,
    pub origin_full_path: String,
    pub target_path: String,
    pub target_file_name: String,
    pub is_duplicate: bool,
    pub status: PlanStatus,
}

#[derive(Debug, Clone)]
pub struct NewPlanEntry {
    pub file_hash: String,
    pub file_size: u64,
    pub origin_file_name: String,
    pub origin_full_path: String,
    pub target_path: String,
    pub target_file_name: String,
    pub is_duplicate: bool,
}

#[derive(Debug, Clone)]
pub struct NewOperationLog {
    pub plan_entry_id: i64,
    pub operation: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    Copied,
    Moved,
    Failed,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Copied => "copied",
            Self::Moved => "moved",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for PlanStatus {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "copied" => Ok(Self::Copied),
            "moved" => Ok(Self::Moved),
            "failed" => Ok(Self::Failed),
            other => Err(AppError::internal(format!(
                "unsupported plan status: {other}"
            ))),
        }
    }
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn initialize(config: &AppConfig) -> Result<Self> {
        let mut connection = Connection::open(&config.database_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        apply_migrations(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.connection.lock()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO app_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn inventory_snapshot(&self) -> Result<Vec<InventoryRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_hash, blake3_hash, file_size, file_name, relative_path, captured_at, \
             modified_at, exif_model, exif_make, exif_artist, is_duplicate FROM media_inventory",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (
                id,
                file_hash,
                blake3_hash,
                file_size,
                file_name,
                relative_path,
                captured_at,
                modified_at,
                exif_model,
                exif_make,
                exif_artist,
                is_duplicate,
            ) = row?;

            let file_size = u64::try_from(file_size)
                .map_err(|_| AppError::internal("negative file size in inventory"))?;

            records.push(InventoryRecord {
                id,
                file_hash,
                blake3_hash,
                file_size,
                file_name,
                relative_path,
                captured_at,
                modified_at,
                exif_model,
                exif_make,
                exif_artist,
                is_duplicate: is_duplicate != 0,
            });
        }
        Ok(records)
    }

    pub fn replace_inventory(&self, records: &[InventoryRecord]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM media_inventory", [])?;
        for record in records {
            let file_size = i64::try_from(record.file_size)
                .map_err(|_| AppError::internal("file size exceeds sqlite limits"))?;
            tx.execute(
                "INSERT INTO media_inventory (file_hash, blake3_hash, file_size, file_name, \
                 relative_path, captured_at, modified_at, exif_model, exif_make, exif_artist, \
                 is_duplicate, hash_algo, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                params![
                    record.file_hash,
                    record.blake3_hash,
                    file_size,
                    record.file_name,
                    record.relative_path,
                    record.captured_at,
                    record.modified_at,
                    record.exif_model,
                    record.exif_make,
                    record.exif_artist,
                    if record.is_duplicate { 1 } else { 0 },
                    "md5",
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn replace_plan_entries(&self, entries: &[NewPlanEntry]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM operation_logs", [])?;
        tx.execute("DELETE FROM plan_entries", [])?;
        for entry in entries {
            let file_size = i64::try_from(entry.file_size)
                .map_err(|_| AppError::internal("file size exceeds sqlite limits"))?;
            tx.execute(
                "INSERT INTO plan_entries (file_hash, file_size, origin_file_name, origin_full_path, \
                 target_path, target_file_name, is_duplicate, status, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                params![
                    entry.file_hash,
                    file_size,
                    entry.origin_file_name,
                    entry.origin_full_path,
                    entry.target_path,
                    entry.target_file_name,
                    if entry.is_duplicate { 1 } else { 0 },
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn plan_entries(&self) -> Result<Vec<PlanRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_hash, file_size, origin_file_name, origin_full_path, target_path, \
             target_file_name, is_duplicate, status FROM plan_entries ORDER BY id",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (
                id,
                file_hash,
                file_size,
                origin_file_name,
                origin_full_path,
                target_path,
                target_file_name,
                is_duplicate,
                status,
            ) = row?;

            let status = PlanStatus::try_from(status.as_str())?;
            let file_size = u64::try_from(file_size)
                .map_err(|_| AppError::internal("negative file size in plan entry"))?;

            records.push(PlanRecord {
                id,
                file_hash,
                file_size,
                origin_file_name,
                origin_full_path,
                target_path,
                target_file_name,
                is_duplicate: is_duplicate != 0,
                status,
            });
        }

        Ok(records)
    }

    pub fn plan_entries_with_status(&self, statuses: &[PlanStatus]) -> Result<Vec<PlanRecord>> {
        if statuses.is_empty() {
            return self.plan_entries();
        }

        let entries = self.plan_entries()?;
        Ok(entries
            .into_iter()
            .filter(|entry| statuses.contains(&entry.status))
            .collect())
    }

    pub fn update_plan_status(&self, id: i64, status: PlanStatus) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE plan_entries SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status.as_str(), id],
        )?;
        Ok(())
    }

    pub fn append_operation_log(&self, log: NewOperationLog) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO operation_logs (plan_entry_id, operation, status, error) VALUES (?1, ?2, ?3, ?4)",
            params![log.plan_entry_id, log.operation, log.status, log.error],
        )?;
        Ok(())
    }

    pub fn clear_operation_logs(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM operation_logs", [])?;
        Ok(())
    }
}

fn apply_migrations(connection: &mut Connection) -> Result<()> {
    let current_version: i32 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;

    let tx = connection.transaction()?;

    if current_version < DB_VERSION {
        tx.execute("DROP TABLE IF EXISTS media_inventory", [])?;
        tx.execute("DROP TABLE IF EXISTS plan_entries", [])?;
        tx.execute("DROP TABLE IF EXISTS operation_logs", [])?;
    }

    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS media_inventory (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_hash TEXT NOT NULL,
            blake3_hash TEXT,
            file_size INTEGER NOT NULL,
            file_name TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            captured_at TEXT,
            modified_at TEXT NOT NULL,
            exif_model TEXT,
            exif_make TEXT,
            exif_artist TEXT,
            is_duplicate INTEGER NOT NULL DEFAULT 0,
            hash_algo TEXT NOT NULL DEFAULT 'md5',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS plan_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_hash TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            origin_file_name TEXT NOT NULL,
            origin_full_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            target_file_name TEXT NOT NULL,
            is_duplicate INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS operation_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plan_entry_id INTEGER NOT NULL,
            operation TEXT NOT NULL,
            status TEXT NOT NULL,
            error TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(plan_entry_id) REFERENCES plan_entries(id)
        );

        CREATE INDEX IF NOT EXISTS idx_media_inventory_hash ON media_inventory(file_hash);
        CREATE INDEX IF NOT EXISTS idx_media_inventory_relative_path ON media_inventory(relative_path);
        CREATE INDEX IF NOT EXISTS idx_plan_entries_status ON plan_entries(status);
        "#,
    )?;

    tx.execute(
        "INSERT OR REPLACE INTO app_meta (key, value) VALUES ('schema_version', ?1)",
        params![SCHEMA_VERSION.to_string()],
    )?;

    tx.execute_batch(&format!("PRAGMA user_version = {DB_VERSION};"))?;
    tx.commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use tempfile::{tempdir, NamedTempFile};

    #[test]
    fn database_initializes_and_sets_schema_version() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let config = temp_config(temp.path().to_path_buf());

        let db = Database::initialize(&config)?;
        let value: String = db.conn().query_row(
            "SELECT value FROM app_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(value, SCHEMA_VERSION.to_string());
        Ok(())
    }

    #[test]
    fn inventory_round_trip() -> Result<()> {
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("db.sqlite3");
        let config = temp_config(db_path.clone());
        let db = Database::initialize(&config)?;

        let record = InventoryRecord {
            id: None,
            file_hash: "md5".into(),
            blake3_hash: Some("blake3".into()),
            file_size: 42,
            file_name: "image.jpg".into(),
            relative_path: "2024/01/image.jpg".into(),
            captured_at: Some("2024-01-01_10-00-00".into()),
            modified_at: "2024-01-01_10-00-00".into(),
            exif_model: Some("Cam".into()),
            exif_make: Some("Make".into()),
            exif_artist: None,
            is_duplicate: false,
        };

        db.replace_inventory(&[record.clone()])?;
        let snapshot = db.inventory_snapshot()?;
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].file_hash, record.file_hash);
        assert_eq!(snapshot[0].blake3_hash, record.blake3_hash);
        Ok(())
    }

    #[test]
    fn plan_entries_round_trip() -> Result<()> {
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("db.sqlite3");
        let config = temp_config(db_path.clone());
        let db = Database::initialize(&config)?;

        let entry = NewPlanEntry {
            file_hash: "hash".into(),
            file_size: 64,
            origin_file_name: "IMG_0001.JPG".into(),
            origin_full_path: "/origin/IMG_0001.JPG".into(),
            target_path: "/target/2024-01-01/".into(),
            target_file_name: "2024-01-01_00-00-00.IMG_0001.JPG".into(),
            is_duplicate: false,
        };

        db.replace_plan_entries(&[entry.clone()])?;
        let stored = db.plan_entries()?;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].status, PlanStatus::Pending);
        assert_eq!(stored[0].target_file_name, entry.target_file_name);

        db.update_plan_status(stored[0].id, PlanStatus::Copied)?;
        let copied = db.plan_entries()?;
        assert_eq!(copied[0].status, PlanStatus::Copied);

        let copied_only = db.plan_entries_with_status(&[PlanStatus::Copied])?;
        assert_eq!(copied_only.len(), 1);
        let pending_only = db.plan_entries_with_status(&[PlanStatus::Pending])?;
        assert!(pending_only.is_empty());

        db.append_operation_log(NewOperationLog {
            plan_entry_id: copied[0].id,
            operation: "copy".into(),
            status: "success".into(),
            error: None,
        })?;

        {
            let conn = db.conn();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM operation_logs", [], |row| row.get(0))?;
            assert_eq!(count, 1);
        }

        db.clear_operation_logs()?;
        {
            let conn = db.conn();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM operation_logs", [], |row| row.get(0))?;
            assert_eq!(count, 0);
        }

        Ok(())
    }

    #[allow(deprecated)]
    fn temp_config(db_path: PathBuf) -> AppConfig {
        let temp_root = tempdir().expect("tempdir").into_path();
        let output_root = tempdir().expect("output").into_path();
        let duplicates_dir = output_root.join("duplicates");
        std::fs::create_dir_all(&duplicates_dir).unwrap();

        AppConfig {
            schema_version: SCHEMA_VERSION,
            home_dir: temp_root.clone(),
            app_data_dir: temp_root.clone(),
            database_path: db_path,
            image_root: temp_root.clone(),
            image_root_default_name: "images".into(),
            output_root: output_root.clone(),
            output_root_name: "output".into(),
            duplicates_dir,
            duplicates_folder_name: "duplicates".into(),
            origin_info_path: temp_root.join("origin.json"),
            target_plan_path: temp_root.join("plan.json"),
            image_exts: HashSet::from([".jpg".into()]),
            config_file_path: PathBuf::from("config/config.json"),
            sample_image_root: None,
        }
    }
}
