use std::time::Duration;

use parking_lot::{Mutex, MutexGuard};
use rusqlite::{params, Connection};
use tracing::info;

use crate::config::{AppConfig, SCHEMA_VERSION};
use crate::error::Result;

const DB_VERSION: i32 = 1;

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
}

fn apply_migrations(connection: &mut Connection) -> Result<()> {
    let current_version: i32 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if current_version >= DB_VERSION {
        info!(current_version = current_version, "database is up to date");
        return Ok(());
    }

    let tx = connection.transaction()?;
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS media_inventory (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_hash TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            file_name TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            captured_at TEXT,
            modified_at TEXT,
            is_duplicate INTEGER NOT NULL DEFAULT 0,
            hash_algo TEXT NOT NULL DEFAULT 'md5',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS plan_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_hash TEXT NOT NULL,
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
    use tempfile::NamedTempFile;

    #[test]
    fn database_initializes_and_sets_schema_version() -> Result<()> {
        let temp = NamedTempFile::new()?;
        let config = AppConfig {
            schema_version: SCHEMA_VERSION,
            home_dir: std::env::temp_dir(),
            app_data_dir: std::env::temp_dir(),
            database_path: temp.path().to_path_buf(),
            image_root: std::env::temp_dir(),
            image_root_default_name: "images".into(),
            output_root: std::env::temp_dir(),
            output_root_name: "output".into(),
            duplicates_dir: std::env::temp_dir(),
            duplicates_folder_name: "duplicates".into(),
            origin_info_path: std::env::temp_dir().join("origin.json"),
            target_plan_path: std::env::temp_dir().join("plan.json"),
            image_exts: HashSet::from([".jpg".into()]),
            config_file_path: PathBuf::from("config/config.json"),
            sample_image_root: None,
        };

        let db = Database::initialize(&config)?;
        let value: String = db.conn().query_row(
            "SELECT value FROM app_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(value, SCHEMA_VERSION.to_string());
        Ok(())
    }
}
