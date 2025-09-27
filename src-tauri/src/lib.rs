mod config;
mod db;
mod error;
mod events;
mod execute;
mod logging;
mod plan;
mod scan;
mod system;
pub mod utils;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing::{error, info};

use crate::config::{AppConfig, ConfigPayload, ConfigService, SCHEMA_VERSION};
use crate::db::Database;
use crate::events::{
    EVENT_BOOTSTRAP_CONFIG, EVENT_EXECUTION_PROGRESS, EVENT_PLAN_PROGRESS, EVENT_SCAN_PROGRESS,
};
use crate::execute::{
    run_execution, undo_moves as undo_plan_moves, ExecutionMode, ExecutionProgressEmitter,
    ExecutionSummary, UndoSummary,
};
use crate::logging::init_logging;
use crate::plan::{generate_plan, PlanProgressEmitter, PlanSummary};
use crate::scan::{perform_scan, ProgressEmitter, ScanSummary};
use crate::system::{disk_status, DiskStatus};

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

    pub fn config_arc(&self) -> Arc<ConfigService> {
        Arc::clone(&self.config)
    }

    pub fn database_arc(&self) -> Arc<Database> {
        Arc::clone(&self.database)
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

#[tauri::command]
fn check_disk_space(state: tauri::State<'_, AppState>) -> Result<DiskStatus, String> {
    let snapshot = state.config().snapshot();
    disk_status(&snapshot.output_root).map_err(|err| err.to_string())
}

#[tauri::command]
async fn scan_media(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<ScanSummary, String> {
    let config = state.config_arc();
    let database = state.database_arc();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let emitter: ProgressEmitter = Arc::new(move |payload| {
            if let Err(err) = app_handle.emit(EVENT_SCAN_PROGRESS, payload.clone()) {
                tracing::debug!(error = ?err, "failed emitting scan progress");
            }
        });

        let snapshot = config.snapshot();
        perform_scan(&snapshot, database.as_ref(), emitter)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn plan_targets(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<PlanSummary, String> {
    let config = state.config_arc();
    let database = state.database_arc();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let emitter: PlanProgressEmitter = Arc::new(move |payload| {
            if let Err(err) = app_handle.emit(EVENT_PLAN_PROGRESS, payload.clone()) {
                tracing::debug!(error = ?err, "failed emitting plan progress");
            }
        });

        let snapshot = config.snapshot();
        generate_plan(&snapshot, database.as_ref(), emitter)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn execute_plan(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
    mode: ExecutionMode,
    dry_run: bool,
) -> Result<ExecutionSummary, String> {
    let config = state.config_arc();
    let database = state.database_arc();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let emitter: ExecutionProgressEmitter = Arc::new(move |payload| {
            if let Err(err) = app_handle.emit(EVENT_EXECUTION_PROGRESS, payload.clone()) {
                tracing::debug!(error = ?err, "failed emitting execution progress");
            }
        });

        let snapshot = config.snapshot();
        run_execution(&snapshot, database.as_ref(), mode, dry_run, emitter)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn undo_moves(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<UndoSummary, String> {
    let config = state.config_arc();
    let database = state.database_arc();
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let emitter: ExecutionProgressEmitter = Arc::new(move |payload| {
            if let Err(err) = app_handle.emit(EVENT_EXECUTION_PROGRESS, payload.clone()) {
                tracing::debug!(error = ?err, "failed emitting undo progress");
            }
        });

        let snapshot = config.snapshot();
        undo_plan_moves(&snapshot, database.as_ref(), emitter)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(config_service, database))
        .invoke_handler(tauri::generate_handler![
            bootstrap_paths,
            check_disk_space,
            scan_media,
            plan_targets,
            execute_plan,
            undo_moves
        ])
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
