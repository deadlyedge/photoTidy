use std::fs;
#[cfg(unix)]
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::db::{Database, NewOperationLog, PlanRecord, PlanStatus};
use crate::error::Result;
use crate::plan::PLAN_SCHEMA_VERSION;

const EXECUTE_STAGE: &str = "execute";
const UNDO_STAGE: &str = "undo";

pub type ExecutionProgressEmitter = Arc<dyn Fn(ExecutionProgressPayload) + Send + Sync>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Copy,
    Move,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionMode::Copy => "copy",
            ExecutionMode::Move => "move",
        }
    }

    fn success_status(self) -> PlanStatus {
        match self {
            ExecutionMode::Copy => PlanStatus::Copied,
            ExecutionMode::Move => PlanStatus::Moved,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    pub mode: ExecutionMode,
    pub dry_run: bool,
    pub total_entries: usize,
    pub processed_entries: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub duplicate_entries: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoSummary {
    pub processed_entries: usize,
    pub restored: usize,
    pub missing: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionProgressPayload {
    pub stage: &'static str,
    pub processed: usize,
    pub total: usize,
    pub current: Option<String>,
}

pub fn run_execution(
    _config: &AppConfig,
    database: &Database,
    mode: ExecutionMode,
    dry_run: bool,
    emitter: ExecutionProgressEmitter,
) -> Result<ExecutionSummary> {
    let entries = database.plan_entries_with_status(&[PlanStatus::Pending])?;
    let total = entries.len();

    emit_progress(&emitter, EXECUTE_STAGE, 0, total, None);

    if total == 0 {
        return Ok(ExecutionSummary {
            mode,
            dry_run,
            total_entries: 0,
            processed_entries: 0,
            succeeded: 0,
            failed: 0,
            duplicate_entries: 0,
        });
    }

    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for (idx, entry) in entries.iter().enumerate() {
        let origin_path = to_native_path(&entry.origin_full_path);
        let target_dir = to_native_path(&entry.target_path);
        let target_path = target_dir.join(&entry.target_file_name);
        let current_path = Some(entry.origin_full_path.clone());

        let origin_exists = origin_path.exists();
        let target_exists = target_path.exists();

        if dry_run {
            if !origin_exists || target_exists {
                failed += 1;
            } else {
                succeeded += 1;
            }

            emit_progress(&emitter, EXECUTE_STAGE, idx + 1, total, current_path);
            continue;
        }

        if !origin_exists {
            failed += 1;
            record_failure(
                database,
                entry,
                Some(PlanStatus::Failed),
                mode.as_str(),
                "origin file missing",
            )?;
            emit_progress(&emitter, EXECUTE_STAGE, idx + 1, total, current_path);
            continue;
        }

        if target_exists {
            failed += 1;
            record_failure(
                database,
                entry,
                Some(PlanStatus::Failed),
                mode.as_str(),
                "target file already exists",
            )?;
            emit_progress(&emitter, EXECUTE_STAGE, idx + 1, total, current_path);
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let op_result = match mode {
            ExecutionMode::Copy => copy_file(&origin_path, &target_path),
            ExecutionMode::Move => move_file(&origin_path, &target_path),
        };

        match op_result {
            Ok(()) => {
                succeeded += 1;
                database.update_plan_status(entry.id, mode.success_status())?;
                database.append_operation_log(NewOperationLog {
                    plan_entry_id: entry.id,
                    operation: mode.as_str().into(),
                    status: "success".into(),
                    error: None,
                })?;
            }
            Err(err) => {
                failed += 1;
                record_failure(
                    database,
                    entry,
                    Some(PlanStatus::Failed),
                    mode.as_str(),
                    &err.to_string(),
                )?;
            }
        }

        emit_progress(&emitter, EXECUTE_STAGE, idx + 1, total, current_path);
    }

    database.set_meta("plan_schema_version", &PLAN_SCHEMA_VERSION.to_string())?;

    let duplicate_entries = entries.iter().filter(|entry| entry.is_duplicate).count();

    Ok(ExecutionSummary {
        mode,
        dry_run,
        total_entries: total,
        processed_entries: total,
        succeeded,
        failed,
        duplicate_entries,
    })
}

pub fn undo_moves(
    _config: &AppConfig,
    database: &Database,
    emitter: ExecutionProgressEmitter,
) -> Result<UndoSummary> {
    let moved_entries = database.plan_entries_with_status(&[PlanStatus::Moved])?;
    let total = moved_entries.len();

    emit_progress(&emitter, UNDO_STAGE, 0, total, None);

    if total == 0 {
        return Ok(UndoSummary {
            processed_entries: 0,
            restored: 0,
            missing: 0,
            failed: 0,
        });
    }

    let mut restored = 0usize;
    let mut missing = 0usize;
    let mut failed = 0usize;

    for (idx, entry) in moved_entries.iter().enumerate() {
        let origin_path = to_native_path(&entry.origin_full_path);
        let target_dir = to_native_path(&entry.target_path);
        let target_path = target_dir.join(&entry.target_file_name);
        let current_path = Some(entry.origin_full_path.clone());

        if !target_path.exists() {
            missing += 1;
            record_failure(database, entry, None, "undo", "target missing during undo")?;
            emit_progress(&emitter, UNDO_STAGE, idx + 1, total, current_path);
            continue;
        }

        if let Some(parent) = origin_path.parent() {
            fs::create_dir_all(parent)?;
        }

        match move_file(&target_path, &origin_path) {
            Ok(()) => {
                restored += 1;
                database.update_plan_status(entry.id, PlanStatus::Pending)?;
                database.append_operation_log(NewOperationLog {
                    plan_entry_id: entry.id,
                    operation: "undo".into(),
                    status: "success".into(),
                    error: None,
                })?;
            }
            Err(err) => {
                failed += 1;
                record_failure(database, entry, None, "undo", &err.to_string())?;
            }
        }

        emit_progress(&emitter, UNDO_STAGE, idx + 1, total, current_path);
    }

    Ok(UndoSummary {
        processed_entries: total,
        restored,
        missing,
        failed,
    })
}

fn emit_progress(
    emitter: &ExecutionProgressEmitter,
    stage: &'static str,
    processed: usize,
    total: usize,
    current: Option<String>,
) {
    let payload = ExecutionProgressPayload {
        stage,
        processed,
        total,
        current,
    };
    (emitter)(payload);
}

fn to_native_path(path: &str) -> PathBuf {
    PathBuf::from(path)
}

fn copy_file(origin: &Path, target: &Path) -> IoResult<()> {
    fs::copy(origin, target)?;
    Ok(())
}

fn move_file(origin: &Path, target: &Path) -> IoResult<()> {
    match fs::rename(origin, target) {
        Ok(()) => Ok(()),
        Err(err) => {
            if should_fallback_copy(&err) {
                fs::copy(origin, target)?;
                fs::remove_file(origin)?;
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

#[cfg(unix)]
fn should_fallback_copy(err: &std::io::Error) -> bool {
    err.kind() == ErrorKind::CrossDeviceLink
}

#[cfg(not(unix))]
fn should_fallback_copy(_err: &std::io::Error) -> bool {
    false
}

fn record_failure(
    database: &Database,
    entry: &PlanRecord,
    status: Option<PlanStatus>,
    operation: &str,
    message: &str,
) -> Result<()> {
    if let Some(status) = status {
        database.update_plan_status(entry.id, status)?;
    }
    database.append_operation_log(NewOperationLog {
        plan_entry_id: entry.id,
        operation: operation.into(),
        status: "failure".into(),
        error: Some(message.to_string()),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SCHEMA_VERSION;
    use crate::db::InventoryRecord;
    use crate::plan::{generate_plan, PlanProgressEmitter};
    use serde_json::Value;
    use std::collections::HashSet;
    use tempfile::tempdir;

    #[test]
    fn copy_execution_copies_files_and_updates_status() -> Result<()> {
        let setup = TestHarness::new()?;
        let plan_emitter: PlanProgressEmitter = Arc::new(|_| {});
        generate_plan(&setup.config, &setup.database, plan_emitter)?;

        let exec_emitter: ExecutionProgressEmitter = Arc::new(|_| {});
        let summary = run_execution(
            &setup.config,
            &setup.database,
            ExecutionMode::Copy,
            false,
            exec_emitter.clone(),
        )?;

        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed, 0);
        assert!(setup.target_one().exists());
        assert!(setup.duplicate_target().exists());
        assert!(setup.origin_one().exists());
        assert!(setup.origin_duplicate().exists());

        let statuses = setup.database.plan_entries()?;
        assert!(statuses
            .iter()
            .all(|entry| entry.status == PlanStatus::Copied));

        Ok(())
    }

    #[test]
    fn move_and_undo_restore_origins() -> Result<()> {
        let setup = TestHarness::new()?;
        let plan_emitter: PlanProgressEmitter = Arc::new(|_| {});
        generate_plan(&setup.config, &setup.database, plan_emitter)?;

        let exec_emitter: ExecutionProgressEmitter = Arc::new(|_| {});
        let summary = run_execution(
            &setup.config,
            &setup.database,
            ExecutionMode::Move,
            false,
            exec_emitter.clone(),
        )?;
        assert_eq!(summary.succeeded, 2);
        assert!(!setup.origin_one().exists());
        assert!(!setup.origin_duplicate().exists());
        assert!(setup.target_one().exists());
        assert!(setup.duplicate_target().exists());

        let undo_summary = undo_moves(&setup.config, &setup.database, exec_emitter)?;
        assert_eq!(undo_summary.restored, 2);
        assert!(setup.origin_one().exists());
        assert!(setup.origin_duplicate().exists());
        assert!(!setup.target_one().exists());
        assert!(!setup.duplicate_target().exists());

        let statuses = setup.database.plan_entries()?;
        assert!(statuses
            .iter()
            .all(|entry| entry.status == PlanStatus::Pending));

        Ok(())
    }

    struct TestHarness {
        config: crate::config::AppConfig,
        database: Database,
        unique_source: PathBuf,
        duplicate_source: PathBuf,
    }

    impl TestHarness {
        #[allow(deprecated)]
        fn new() -> Result<Self> {
            let root_dir = tempdir()?.into_path();
            let output_dir = tempdir()?.into_path();
            let duplicates_dir = output_dir.join("duplicates");
            fs::create_dir_all(&duplicates_dir)?;

            let db_path = output_dir.join("exec.sqlite3");
            let config = crate::config::AppConfig {
                schema_version: SCHEMA_VERSION,
                home_dir: root_dir.clone(),
                app_data_dir: output_dir.clone(),
                database_path: db_path.clone(),
                image_root: root_dir.clone(),
                image_root_default_name: "images".into(),
                output_root: output_dir.clone(),
                output_root_name: "output".into(),
                duplicates_dir: duplicates_dir.clone(),
                duplicates_folder_name: "duplicates".into(),
                origin_info_path: output_dir.join("origin.json"),
                target_plan_path: output_dir.join("plan.json"),
                image_exts: HashSet::from([".jpg".into()]),
                config_file_path: root_dir.join("config.json"),
                sample_image_root: None,
            };

            let database = Database::initialize(&config)?;

            let unique_dir = root_dir.join("A");
            fs::create_dir_all(&unique_dir)?;
            let unique_file = unique_dir.join("IMG_0001.JPG");
            fs::write(&unique_file, b"unique")?;

            let duplicate_dir = root_dir.join("B");
            fs::create_dir_all(&duplicate_dir)?;
            let duplicate_file = duplicate_dir.join("IMG_0001.JPG");
            fs::write(&duplicate_file, b"dup")?;

            let records = vec![
                InventoryRecord {
                    id: None,
                    file_hash: "hash-unique".into(),
                    blake3_hash: None,
                    file_size: 6,
                    file_name: "IMG_0001.JPG".into(),
                    relative_path: "A/IMG_0001.JPG".into(),
                    captured_at: Some("2024-01-02_10-00-00".into()),
                    modified_at: "2024-01-02_10-00-00".into(),
                    exif_model: None,
                    exif_make: None,
                    exif_artist: None,
                    is_duplicate: false,
                },
                InventoryRecord {
                    id: None,
                    file_hash: "hash-dup".into(),
                    blake3_hash: None,
                    file_size: 3,
                    file_name: "IMG_0001.JPG".into(),
                    relative_path: "B/IMG_0001.JPG".into(),
                    captured_at: Some("2024-01-02_10-00-00".into()),
                    modified_at: "2024-01-02_10-00-00".into(),
                    exif_model: None,
                    exif_make: None,
                    exif_artist: None,
                    is_duplicate: true,
                },
            ];
            database.replace_inventory(&records)?;

            Ok(Self {
                config,
                database,
                unique_source: unique_file,
                duplicate_source: duplicate_file,
            })
        }

        fn origin_one(&self) -> PathBuf {
            self.unique_source.clone()
        }

        fn origin_duplicate(&self) -> PathBuf {
            self.duplicate_source.clone()
        }

        fn target_one(&self) -> PathBuf {
            self.plan_path_for("hash-unique")
        }

        fn duplicate_target(&self) -> PathBuf {
            self.plan_path_for("hash-dup")
        }

        fn plan_path_for(&self, hash: &str) -> PathBuf {
            let plan_json = fs::read_to_string(&self.config.target_plan_path).expect("plan json");
            let plan: Vec<Value> = serde_json::from_str(&plan_json).expect("parse plan json");
            let entry = plan
                .iter()
                .find(|value| value["fileHash"] == hash)
                .expect("plan entry");
            let base = entry["newPath"].as_str().expect("newPath");
            let file = entry["newFileName"].as_str().expect("newFileName");
            PathBuf::from(base).join(file)
        }
    }
}
