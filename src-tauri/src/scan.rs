use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use exif::{In, Tag, Value};
use pathdiff::diff_paths;
use rayon::prelude::*;
use serde::Serialize;
use time::{
    format_description::FormatItem, macros::format_description, OffsetDateTime, PrimitiveDateTime,
};
use walkdir::WalkDir;

use crate::config::AppConfig;
use crate::db::{Database, InventoryRecord};
use crate::error::{AppError, Result};
use crate::utils::{
    fs::matches_extension,
    hash::{blake3_file, md5_file},
    path::to_posix_string,
    time as time_utils,
};

const EXIF_DATETIME_FORMAT: &[FormatItem<'_>] =
    format_description!("[year]:[month]:[day] [hour]:[minute]:[second]");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub total_files: usize,
    pub hashed_files: usize,
    pub skipped_files: usize,
    pub duplicate_files: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressPayload {
    pub stage: &'static str,
    pub processed: usize,
    pub total: usize,
    pub current: Option<String>,
}

pub type ProgressEmitter = Arc<dyn Fn(ScanProgressPayload) + Send + Sync>;

struct FileSnapshot {
    absolute_path: PathBuf,
    relative_path: String,
    file_name: String,
    file_size: u64,
    modified_at: String,
}

#[derive(Debug, Default, Clone)]
struct ExifMetadata {
    captured_at: Option<String>,
    camera_model: Option<String>,
    camera_make: Option<String>,
    artist: Option<String>,
}

pub fn perform_scan(
    config: &AppConfig,
    database: &Database,
    emitter: ProgressEmitter,
) -> Result<ScanSummary> {
    let root_dir = config
        .sample_image_root
        .as_ref()
        .unwrap_or(&config.image_root);

    let files = enumerate_files(root_dir, &config.image_exts, &emitter)?;
    if files.is_empty() {
        database.replace_inventory(&[])?;
        emit_progress(&emitter, "scan", 0, 0, None);
        emit_progress(&emitter, "diff", 0, 0, None);
        emit_progress(&emitter, "hash", 0, 0, None);
        return Ok(ScanSummary {
            total_files: 0,
            hashed_files: 0,
            skipped_files: 0,
            duplicate_files: 0,
        });
    }

    let snapshots = build_snapshots(root_dir, files)?;
    let total_files = snapshots.len();

    let existing_records = database.inventory_snapshot()?;
    let mut existing_map: HashMap<String, InventoryRecord> = existing_records
        .into_iter()
        .map(|record| (record.relative_path.clone(), record))
        .collect();

    let mut reused_records = Vec::new();
    let mut to_process = Vec::new();
    let mut skipped = 0usize;

    for snapshot in snapshots {
        if let Some(existing) = existing_map.remove(&snapshot.relative_path) {
            if existing.file_size == snapshot.file_size
                && existing.modified_at == snapshot.modified_at
                && existing.blake3_hash.is_some()
            {
                let mut record = existing;
                record.file_name = snapshot.file_name.clone();
                record.relative_path = snapshot.relative_path.clone();
                record.file_size = snapshot.file_size;
                record.modified_at = snapshot.modified_at.clone();
                record.is_duplicate = false;
                reused_records.push(record);
                skipped += 1;
                continue;
            }
        }
        to_process.push(snapshot);
    }

    emit_progress(&emitter, "diff", skipped, total_files, None);

    let hash_total = to_process.len();
    let hashed_records = hash_and_extract(to_process, &emitter)?;

    let mut all_records = Vec::with_capacity(reused_records.len() + hashed_records.len());
    all_records.extend(reused_records);
    all_records.extend(hashed_records);

    let duplicate_files = mark_duplicates(&mut all_records);

    all_records.sort_by(|a, b| {
        let a_key = a.captured_at.as_ref().unwrap_or(&a.modified_at);
        let b_key = b.captured_at.as_ref().unwrap_or(&b.modified_at);
        match a_key.cmp(b_key) {
            std::cmp::Ordering::Equal => a.relative_path.cmp(&b.relative_path),
            ordering => ordering,
        }
    });

    database.replace_inventory(&all_records)?;

    Ok(ScanSummary {
        total_files,
        hashed_files: hash_total,
        skipped_files: skipped,
        duplicate_files,
    })
}

fn enumerate_files(
    root: &Path,
    extensions: &HashSet<String>,
    emitter: &ProgressEmitter,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && matches_extension(path, extensions) {
            files.push(path.to_path_buf());
            let processed = files.len();
            emit_progress(
                emitter,
                "scan",
                processed,
                processed,
                Some(to_posix_string(path).into_owned()),
            );
        }
    }

    files.sort();
    emit_progress(emitter, "scan", files.len(), files.len(), None);
    Ok(files)
}

fn build_snapshots(root: &Path, files: Vec<PathBuf>) -> Result<Vec<FileSnapshot>> {
    let mut snapshots = Vec::with_capacity(files.len());

    for path in files {
        let metadata = match path.metadata() {
            Ok(meta) => meta,
            Err(err) => {
                tracing::warn!(path = %path.display(), error = ?err, "failed to read metadata");
                continue;
            }
        };

        let relative_path = diff_paths(&path, root)
            .and_then(|p| p.to_str().map(|s| s.replace('\\', "/")))
            .ok_or_else(|| {
                AppError::Config(format!(
                    "failed to compute relative path for {}",
                    path.display()
                ))
            })?;

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::Internal(format!("invalid file name for {}", path.display())))?
            .to_string();

        let file_size = metadata.len();
        let modified_time = metadata.modified()?;
        let modified_dt = OffsetDateTime::from(modified_time);
        let modified_at = time_utils::format_timestamp(modified_dt)?;

        snapshots.push(FileSnapshot {
            absolute_path: path,
            relative_path,
            file_name,
            file_size,
            modified_at,
        });
    }

    Ok(snapshots)
}

fn hash_and_extract(
    snapshots: Vec<FileSnapshot>,
    emitter: &ProgressEmitter,
) -> Result<Vec<InventoryRecord>> {
    if snapshots.is_empty() {
        emit_progress(emitter, "hash", 0, 0, None);
        return Ok(Vec::new());
    }

    let counter = AtomicUsize::new(0);
    let total = snapshots.len();
    let emitter_clone = emitter.clone();

    let results: Result<Vec<InventoryRecord>> = snapshots
        .into_par_iter()
        .map(|snapshot| {
            let md5 = md5_file(&snapshot.absolute_path)?;
            let blake3 = blake3_file(&snapshot.absolute_path)?;
            let exif = extract_exif(&snapshot.absolute_path);

            let captured_at = exif
                .captured_at
                .unwrap_or_else(|| snapshot.modified_at.clone());

            let record = InventoryRecord {
                id: None,
                file_hash: md5,
                blake3_hash: Some(blake3),
                file_size: snapshot.file_size,
                file_name: snapshot.file_name,
                relative_path: snapshot.relative_path.clone(),
                captured_at: Some(captured_at),
                modified_at: snapshot.modified_at.clone(),
                exif_model: exif.camera_model,
                exif_make: exif.camera_make,
                exif_artist: exif.artist,
                is_duplicate: false,
            };

            let processed = counter.fetch_add(1, Ordering::Relaxed) + 1;
            emit_progress(
                &emitter_clone,
                "hash",
                processed,
                total,
                Some(snapshot.relative_path),
            );

            Ok(record)
        })
        .collect();

    emit_progress(&emitter_clone, "hash", total, total, None);
    results
}

fn mark_duplicates(records: &mut [InventoryRecord]) -> usize {
    let mut seen = HashSet::new();
    let mut duplicates = 0usize;

    for record in records.iter_mut() {
        if !seen.insert(record.file_hash.clone()) {
            record.is_duplicate = true;
            duplicates += 1;
        } else {
            record.is_duplicate = false;
        }
    }

    duplicates
}

fn extract_exif(path: &Path) -> ExifMetadata {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) => {
            tracing::debug!(path = %path.display(), error = ?err, "unable to open file for EXIF");
            return ExifMetadata::default();
        }
    };

    let mut reader = BufReader::new(file);
    let exif_reader = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(reader) => reader,
        Err(err) => {
            tracing::debug!(path = %path.display(), error = ?err, "no EXIF metadata");
            return ExifMetadata::default();
        }
    };

    ExifMetadata {
        captured_at: exif_reader
            .get_field(Tag::DateTimeOriginal, In::PRIMARY)
            .and_then(|field| exif_ascii_value(&field.value))
            .and_then(normalize_exif_timestamp),
        camera_model: exif_reader
            .get_field(Tag::Model, In::PRIMARY)
            .and_then(|field| exif_ascii_value(&field.value))
            .map(|s| s.to_string()),
        camera_make: exif_reader
            .get_field(Tag::Make, In::PRIMARY)
            .and_then(|field| exif_ascii_value(&field.value))
            .map(|s| s.to_string()),
        artist: exif_reader
            .get_field(Tag::Artist, In::PRIMARY)
            .and_then(|field| exif_ascii_value(&field.value))
            .map(|s| s.to_string()),
    }
}

fn exif_ascii_value(value: &Value) -> Option<&str> {
    match value {
        Value::Ascii(ref vec) if !vec.is_empty() => {
            std::str::from_utf8(&vec[0]).ok().map(|s| s.trim())
        }
        _ => None,
    }
}

fn normalize_exif_timestamp(raw: &str) -> Option<String> {
    let trimmed = raw.trim_matches('\0');
    let parsed = PrimitiveDateTime::parse(trimmed, EXIF_DATETIME_FORMAT).ok()?;
    let offset = parsed.assume_utc();
    time_utils::format_timestamp(offset).ok()
}

fn emit_progress(
    emitter: &ProgressEmitter,
    stage: &'static str,
    processed: usize,
    total: usize,
    current: Option<String>,
) {
    let payload = ScanProgressPayload {
        stage,
        processed,
        total,
        current,
    };
    (*emitter)(payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;

    use crate::config::SCHEMA_VERSION;

    #[allow(deprecated)]
    #[test]
    fn scan_detects_duplicates_and_skips_cached_files() -> Result<()> {
        let root_dir = tempdir()?.into_path();
        let output_dir = tempdir()?.into_path();
        let duplicates_dir = output_dir.join("duplicates");
        fs::create_dir_all(&duplicates_dir)?;

        let file_one = root_dir.join("one.jpg");
        let nested_dir = root_dir.join("nested");
        fs::create_dir_all(&nested_dir)?;
        let file_duplicate = nested_dir.join("dup.jpg");
        let file_unique = root_dir.join("unique.jpg");

        fs::write(&file_one, b"same")?;
        fs::write(&file_duplicate, b"same")?;
        fs::write(&file_unique, b"different")?;

        let config = AppConfig {
            schema_version: SCHEMA_VERSION,
            home_dir: root_dir.clone(),
            app_data_dir: output_dir.clone(),
            database_path: output_dir.join("scan.sqlite3"),
            image_root: root_dir.clone(),
            image_root_default_name: "images".into(),
            output_root: output_dir.clone(),
            output_root_name: "output".into(),
            duplicates_dir: duplicates_dir.clone(),
            duplicates_folder_name: "duplicates".into(),
            origin_info_path: output_dir.join("origin.json"),
            target_plan_path: output_dir.join("plan.json"),
            image_exts: HashSet::from([".jpg".into()]),
            config_file_path: PathBuf::from("config/config.json"),
            sample_image_root: None,
        };

        let database = Database::initialize(&config)?;
        let emitter: ProgressEmitter = Arc::new(|_| {});

        let summary_first = perform_scan(&config, &database, emitter.clone())?;
        assert_eq!(summary_first.total_files, 3);
        assert_eq!(summary_first.hashed_files, 3);
        assert_eq!(summary_first.duplicate_files, 1);

        let summary_second = perform_scan(&config, &database, emitter)?;
        assert_eq!(summary_second.hashed_files, 0);
        assert_eq!(summary_second.skipped_files, 3);

        let stored = database.inventory_snapshot()?;
        assert_eq!(stored.len(), 3);
        assert!(stored.iter().any(|record| record.is_duplicate));
        Ok(())
    }
}
