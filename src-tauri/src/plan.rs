use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::config::AppConfig;
use crate::db::{Database, NewPlanEntry};
use crate::error::Result;
use crate::utils::json;
use crate::utils::path::{ensure_trailing_separator, to_posix_string};
use crate::utils::time::now_timestamp;

const PLAN_STAGE: &str = "plan";
pub const PLAN_SCHEMA_VERSION: i32 = 1;

pub type PlanProgressEmitter = Arc<dyn Fn(PlanProgressPayload) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanProgressPayload {
    pub stage: &'static str,
    pub processed: usize,
    pub total: usize,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub file_hash: String,
    pub file_size: u64,
    pub origin_file_name: String,
    pub origin_full_path: String,
    pub new_file_name: String,
    pub new_path: String,
    pub is_duplicate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub generated_at: String,
    pub total_entries: usize,
    pub duplicate_entries: usize,
    pub unique_entries: usize,
    pub destination_buckets: usize,
    pub total_bytes: u64,
    pub plan_json_path: String,
    pub entries: Vec<PlanItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyPlanItem {
    pub file_hash: String,
    pub file_size: u64,
    pub origin_file_name: String,
    pub origin_full_path: String,
    pub new_file_name: String,
    pub new_path: String,
}

pub fn generate_plan(
    config: &AppConfig,
    database: &Database,
    emitter: PlanProgressEmitter,
) -> Result<PlanSummary> {
    let inventory = database.inventory_snapshot()?;
    let total = inventory.len();

    emit_progress(&emitter, 0, total, None);

    if inventory.is_empty() {
        database.replace_plan_entries(&[])?;
        database.set_meta("plan_entry_count", "0")?;
        database.set_meta("plan_schema_version", &PLAN_SCHEMA_VERSION.to_string())?;
        database.set_meta("plan_total_bytes", "0")?;

        let generated_at = now_timestamp()?;
        let plan_json_path = to_posix_string(&config.target_plan_path).into_owned();
        json::write_json(&config.target_plan_path, &Vec::<LegacyPlanItem>::new())?;

        return Ok(PlanSummary {
            generated_at,
            total_entries: 0,
            duplicate_entries: 0,
            unique_entries: 0,
            destination_buckets: 0,
            total_bytes: 0,
            plan_json_path,
            entries: Vec::new(),
        });
    }

    let root_dir = config
        .sample_image_root
        .as_ref()
        .unwrap_or(&config.image_root);

    let mut used_targets: HashSet<String> = HashSet::new();
    let mut destinations: HashSet<String> = HashSet::new();
    let mut plan_items = Vec::with_capacity(total);
    let mut db_entries = Vec::with_capacity(total);

    for (idx, record) in inventory.iter().enumerate() {
        let timestamp = record.captured_at.as_deref().unwrap_or(&record.modified_at);
        let date_bucket = bucket_from_timestamp(timestamp);

        let mut target_dir = if record.is_duplicate {
            config.duplicates_dir.clone()
        } else {
            config.output_root.join(date_bucket)
        };
        target_dir = ensure_trailing_separator(&target_dir);
        let target_path_string = to_posix_string(&target_dir).into_owned();
        destinations.insert(target_path_string.clone());

        let base_file_name = format!("{timestamp}.{}", record.file_name);
        let unique_file_name =
            reserve_target_name(&mut used_targets, &target_path_string, &base_file_name);

        let origin_full_path = join_origin(root_dir, &record.relative_path);
        let origin_full_path_string = to_posix_string(&origin_full_path).into_owned();

        plan_items.push(PlanItem {
            file_hash: record.file_hash.clone(),
            file_size: record.file_size,
            origin_file_name: record.file_name.clone(),
            origin_full_path: origin_full_path_string.clone(),
            new_file_name: unique_file_name.clone(),
            new_path: target_path_string.clone(),
            is_duplicate: record.is_duplicate,
        });

        db_entries.push(NewPlanEntry {
            file_hash: record.file_hash.clone(),
            file_size: record.file_size,
            origin_file_name: record.file_name.clone(),
            origin_full_path: origin_full_path_string,
            target_path: target_path_string.clone(),
            target_file_name: unique_file_name,
            is_duplicate: record.is_duplicate,
        });

        emit_progress(
            &emitter,
            idx + 1,
            total,
            Some(to_posix_string(&origin_full_path).into_owned()),
        );
    }

    database.replace_plan_entries(&db_entries)?;

    let total_bytes: u64 = plan_items.iter().map(|item| item.file_size).sum();

    let generated_at = now_timestamp()?;
    database.set_meta("plan_generated_at", &generated_at)?;
    database.set_meta("plan_entry_count", &plan_items.len().to_string())?;
    database.set_meta("plan_schema_version", &PLAN_SCHEMA_VERSION.to_string())?;
    database.set_meta("plan_total_bytes", &total_bytes.to_string())?;

    let legacy: Vec<LegacyPlanItem> = plan_items
        .iter()
        .map(|item| LegacyPlanItem {
            file_hash: item.file_hash.clone(),
            file_size: item.file_size,
            origin_file_name: item.origin_file_name.clone(),
            origin_full_path: item.origin_full_path.clone(),
            new_file_name: item.new_file_name.clone(),
            new_path: item.new_path.clone(),
        })
        .collect();
    json::write_json(&config.target_plan_path, &legacy)?;

    let duplicate_entries = inventory
        .iter()
        .filter(|record| record.is_duplicate)
        .count();
    let plan_json_path = to_posix_string(&config.target_plan_path).into_owned();

    Ok(PlanSummary {
        generated_at,
        total_entries: plan_items.len(),
        duplicate_entries,
        unique_entries: plan_items.len().saturating_sub(duplicate_entries),
        destination_buckets: destinations.len(),
        total_bytes,
        plan_json_path,
        entries: plan_items,
    })
}

fn emit_progress(
    emitter: &PlanProgressEmitter,
    processed: usize,
    total: usize,
    current: Option<String>,
) {
    let payload = PlanProgressPayload {
        stage: PLAN_STAGE,
        processed,
        total,
        current,
    };
    (emitter)(payload);
}

fn bucket_from_timestamp(timestamp: &str) -> &str {
    timestamp.split('_').next().unwrap_or(timestamp)
}

fn join_origin(root: &Path, relative: &str) -> PathBuf {
    let rel_path = Path::new(relative);
    root.join(rel_path)
}

fn reserve_target_name(used: &mut HashSet<String>, path: &str, base_name: &str) -> String {
    let mut attempt = 0usize;
    loop {
        let candidate = if attempt == 0 {
            base_name.to_string()
        } else {
            add_duplicate_suffix(base_name, attempt)
        };
        let key = format!("{path}{candidate}");
        if used.insert(key) {
            return candidate;
        }
        attempt += 1;
    }
}

fn add_duplicate_suffix(name: &str, attempt: usize) -> String {
    let suffix = format!("_dup{attempt}");
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}{suffix}.{ext}"),
        None => format!("{name}{suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SCHEMA_VERSION;
    use crate::db::{InventoryRecord, PlanStatus};
    use std::collections::HashSet as StdHashSet;
    use std::fs;
    use tempfile::tempdir;

    #[allow(deprecated)]
    #[test]
    fn generate_plan_builds_entries_and_persists_json() -> Result<()> {
        let root_dir = tempdir()?.into_path();
        let output_dir = tempdir()?.into_path();
        let duplicates_dir = output_dir.join("duplicates");
        fs::create_dir_all(&duplicates_dir)?;

        let db_path = output_dir.join("plan.sqlite3");
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
            image_exts: StdHashSet::from([".jpg".into()]),
            config_file_path: root_dir.join("config.json"),
            sample_image_root: None,
        };

        let database = Database::initialize(&config)?;
        let records = vec![
            InventoryRecord {
                id: None,
                file_hash: "hash-1".into(),
                blake3_hash: None,
                file_size: 100,
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
                file_hash: "hash-2".into(),
                blake3_hash: None,
                file_size: 100,
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

        let emitter: PlanProgressEmitter = Arc::new(|_| {});
        let summary = generate_plan(&config, &database, emitter)?;
        assert_eq!(summary.total_bytes, 200);

        assert_eq!(summary.total_entries, 2);
        assert_eq!(summary.duplicate_entries, 1);
        assert_eq!(summary.destination_buckets >= 1, true);
        assert!(summary.entries.iter().any(|item| item.is_duplicate));

        let stored = database.plan_entries()?;
        assert_eq!(stored.len(), 2);
        assert!(stored.iter().any(|entry| entry.is_duplicate));
        assert!(stored
            .iter()
            .all(|entry| entry.status == PlanStatus::Pending));

        let json_contents = fs::read_to_string(&config.target_plan_path)?;
        assert!(json_contents.contains("2024-01-02"));
        Ok(())
    }
}
