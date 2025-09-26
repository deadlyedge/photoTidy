mod config;
mod db;
mod error;
mod events;
mod logging;
pub mod utils;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info};

use crate::config::{AppConfig, ConfigPayload, ConfigService, SCHEMA_VERSION};
use crate::db::Database;
use crate::events::EVENT_BOOTSTRAP_CONFIG;
use crate::logging::init_logging;

#[derive(Clone)]
pub struct AppState {
    config: Arc<ConfigService>,
    database: Arc<Database>,
}

impl AppState {
    fn new(config: ConfigService, database: Database) -> Self {
        Self {
            config: Arc::new(config),
            database: Arc::new(database),
        }
    }

    pub fn config(&self) -> &ConfigService {
        self.config.as_ref()
    }

    pub fn database(&self) -> &Database {
        self.database.as_ref()
    }
}

#[tauri::command]
fn bootstrap_paths(state: tauri::State<'_, AppState>, app: AppHandle) -> ConfigPayload {
    let payload = state.config().payload();
    if let Err(err) = app.emit(EVENT_BOOTSTRAP_CONFIG, payload.clone()) {
        error!("failed to emit bootstrap event: {err:?}");
    }
    payload
}

pub fn run() {
    init_logging();

    let config_service = ConfigService::initialize().expect("failed to initialize config service");
    let config_snapshot: AppConfig = config_service.snapshot();
    let database =
        Database::initialize(&config_snapshot).expect("failed to initialize sqlite database");

    database
        .set_meta("schema_version", &SCHEMA_VERSION.to_string())
        .expect("failed to persist schema version");

    info!(
        db_path = %config_snapshot.database_path.display(),
        "database initialized"
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(config_service, database))
        .invoke_handler(tauri::generate_handler![bootstrap_paths])
        .setup(|app| {
            if let Some(state) = app.try_state::<AppState>() {
                let payload = state.config().payload();
                if let Err(err) = app.emit(EVENT_BOOTSTRAP_CONFIG, payload.clone()) {
                    error!("failed to emit bootstrap event from setup: {err:?}");
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
