use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use parking_lot::RwLock;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;

use crate::error::{AppError, Result};
use crate::utils::fs::{ensure_dir, ensure_parent_dir};
use crate::utils::path::{ensure_trailing_separator, join_and_normalize, to_posix_string};

const DEFAULT_CONFIG_JSON: &str = include_str!("../../config/config.json");

pub const SCHEMA_VERSION: i32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawConfig {
    #[serde(default)]
    image_root: Option<String>,
    image_root_default_name: String,
    image_exts: Vec<String>,
    output_root_name: String,
    origin_info_json: String,
    target_file_structure_json: String,
    folder_for_duplicates: String,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub schema_version: i32,
    pub home_dir: PathBuf,
    pub app_data_dir: PathBuf,
    pub database_path: PathBuf,
    pub image_root: PathBuf,
    pub image_root_default_name: String,
    pub output_root: PathBuf,
    pub output_root_name: String,
    pub duplicates_dir: PathBuf,
    pub duplicates_folder_name: String,
    pub origin_info_path: PathBuf,
    pub target_plan_path: PathBuf,
    pub image_exts: HashSet<String>,
    pub config_file_path: PathBuf,
    pub sample_image_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigPayload {
    pub schema_version: i32,
    pub database_path: String,
    pub image_root: String,
    pub image_root_default_name: String,
    pub output_root: String,
    pub output_root_name: String,
    pub duplicates_dir: String,
    pub duplicates_folder_name: String,
    pub origin_info_json: String,
    pub target_plan_json: String,
    pub image_exts: Vec<String>,
    pub sample_image_root: Option<String>,
}

pub struct ConfigService {
    inner: RwLock<AppConfig>,
}

impl ConfigService {
    pub fn initialize() -> Result<Self> {
        let raw: RawConfig = serde_json::from_str(DEFAULT_CONFIG_JSON)?;
        let config_file_path =
            locate_runtime_config().unwrap_or_else(|| PathBuf::from("config/config.json"));
        let raw = if config_file_path.exists() {
            match crate::utils::json::read_json::<RawConfig>(&config_file_path) {
                Ok(cfg) => cfg,
                Err(err) => {
                    debug!(error = ?err, "failed to read runtime config override");
                    raw
                }
            }
        } else {
            raw
        };

        let app_config = build_app_config(raw, config_file_path)?;
        Ok(Self {
            inner: RwLock::new(app_config),
        })
    }

    pub fn snapshot(&self) -> AppConfig {
        self.inner.read().clone()
    }

    pub fn payload(&self) -> ConfigPayload {
        ConfigPayload::from(&*self.inner.read())
    }
}

fn build_app_config(raw: RawConfig, config_file_path: PathBuf) -> Result<AppConfig> {
    let base_dirs = BaseDirs::new()
        .ok_or_else(|| AppError::Config("unable to determine home directory".into()))?;
    let home_dir = resolve_home_dir(&base_dirs)?;

    let app_data_dir = resolve_data_dir(&base_dirs)?;
    ensure_dir(&app_data_dir)?;

    let database_path = app_data_dir.join("phototidy.sqlite3");
    ensure_parent_dir(&database_path)?;

    let image_root = home_dir.join(&raw.image_root_default_name);
    ensure_dir(&image_root)?;

    let output_root = home_dir.join(&raw.output_root_name);
    ensure_dir(&output_root)?;

    let duplicates_dir = output_root.join(&raw.folder_for_duplicates);
    ensure_dir(&duplicates_dir)?;

    let origin_info_path = output_root.join(&raw.origin_info_json);
    let target_plan_path = output_root.join(&raw.target_file_structure_json);

    let image_exts: HashSet<String> = raw
        .image_exts
        .into_iter()
        .map(|ext| ext.to_ascii_lowercase())
        .collect();

    let sample_image_root = raw
        .image_root
        .and_then(|value| join_and_normalize(env::current_dir().ok()?, Path::new(&value)).ok());

    Ok(AppConfig {
        schema_version: SCHEMA_VERSION,
        home_dir,
        app_data_dir,
        database_path,
        image_root,
        image_root_default_name: raw.image_root_default_name,
        output_root,
        output_root_name: raw.output_root_name,
        duplicates_dir,
        duplicates_folder_name: raw.folder_for_duplicates,
        origin_info_path,
        target_plan_path,
        image_exts,
        config_file_path,
        sample_image_root,
    })
}

impl From<&AppConfig> for ConfigPayload {
    fn from(config: &AppConfig) -> Self {
        let image_root = ensure_trailing_separator(&config.image_root);
        let output_root = ensure_trailing_separator(&config.output_root);
        let duplicates_dir = ensure_trailing_separator(&config.duplicates_dir);

        let mut image_exts = config.image_exts.iter().cloned().collect::<Vec<_>>();
        image_exts.sort();

        Self {
            schema_version: config.schema_version,
            database_path: to_posix_string(&config.database_path).into_owned(),
            image_root: to_posix_string(&image_root).into_owned(),
            image_root_default_name: config.image_root_default_name.clone(),
            output_root: to_posix_string(&output_root).into_owned(),
            output_root_name: config.output_root_name.clone(),
            duplicates_dir: to_posix_string(&duplicates_dir).into_owned(),
            duplicates_folder_name: config.duplicates_folder_name.clone(),
            origin_info_json: to_posix_string(&config.origin_info_path).into_owned(),
            target_plan_json: to_posix_string(&config.target_plan_path).into_owned(),
            image_exts,
            sample_image_root: config
                .sample_image_root
                .as_ref()
                .map(|path| to_posix_string(path).into_owned()),
        }
    }
}

fn resolve_home_dir(base_dirs: &BaseDirs) -> Result<PathBuf> {
    if let Ok(path) = env::var("PHOTOTIDY_HOME") {
        return Ok(PathBuf::from(path));
    }
    Ok(PathBuf::from(base_dirs.home_dir()))
}

fn resolve_data_dir(base_dirs: &BaseDirs) -> Result<PathBuf> {
    if let Ok(path) = env::var("PHOTOTIDY_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(PathBuf::from(base_dirs.data_local_dir()).join("photoTidy"))
}

fn locate_runtime_config() -> Option<PathBuf> {
    let search_paths = [
        PathBuf::from("config/config.json"),
        PathBuf::from("../config/config.json"),
        PathBuf::from("../../config/config.json"),
    ];

    search_paths.into_iter().find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_formats_paths_with_forward_slashes() -> Result<()> {
        let home = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::env::set_var("PHOTOTIDY_HOME", home.path());
        std::env::set_var("PHOTOTIDY_DATA_DIR", data.path());

        let raw: RawConfig = serde_json::from_str(DEFAULT_CONFIG_JSON)?;
        let config = build_app_config(raw, PathBuf::from("config/config.json"))?;
        let payload = ConfigPayload::from(&config);
        assert!(payload.image_root.ends_with('/'));
        std::env::remove_var("PHOTOTIDY_HOME");
        std::env::remove_var("PHOTOTIDY_DATA_DIR");
        Ok(())
    }
}
