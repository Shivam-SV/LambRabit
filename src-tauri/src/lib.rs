mod config;
mod error;
mod layout;
mod services;

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, State};
use tokio::sync::RwLock;

use config::Settings;
use error::Result;
use layout::Layout;
use services::{ServiceInfo, ServiceKind, Supervisor};

struct AppState {
    settings: RwLock<Settings>,
    settings_path: PathBuf,
    supervisor: Supervisor,
}

impl AppState {
    async fn snapshot(&self) -> (Settings, Layout) {
        let settings = self.settings.read().await.clone();
        let layout = Layout::new(&settings.root_dir);
        (settings, layout)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvPaths {
    root: PathBuf,
    www: PathBuf,
    logs: PathBuf,
    settings_file: PathBuf,
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    Layout::new(&settings.root_dir).ensure()?;
    settings.save(&state.settings_path)?;
    *state.settings.write().await = settings.clone();
    Ok(settings)
}

#[tauri::command]
async fn env_paths(state: State<'_, AppState>) -> Result<EnvPaths> {
    let (_, layout) = state.snapshot().await;
    Ok(EnvPaths {
        root: layout.root().to_path_buf(),
        www: layout.www(),
        logs: layout.logs(),
        settings_file: state.settings_path.clone(),
    })
}

#[tauri::command]
async fn list_services(state: State<'_, AppState>) -> Result<Vec<ServiceInfo>> {
    let (settings, layout) = state.snapshot().await;
    Ok(state.supervisor.list(&settings, &layout).await)
}

#[tauri::command]
async fn start_service(app: AppHandle, state: State<'_, AppState>, kind: ServiceKind) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(&app, kind, &settings, &layout).await
}

#[tauri::command]
async fn stop_service(app: AppHandle, state: State<'_, AppState>, kind: ServiceKind) -> Result<()> {
    state.supervisor.stop(&app, kind).await;
    Ok(())
}

/// Starts every installed service; returns one error message per failure.
#[tauri::command]
async fn start_all(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<String>> {
    let (settings, layout) = state.snapshot().await;
    let mut errors = Vec::new();
    for kind in ServiceKind::all() {
        if let Err(e) = state.supervisor.start(&app, kind, &settings, &layout).await {
            errors.push(format!("{}: {e}", kind.label()));
        }
    }
    Ok(errors)
}

#[tauri::command]
async fn stop_all(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    for kind in ServiceKind::all() {
        state.supervisor.stop(&app, kind).await;
    }
    Ok(())
}

#[tauri::command]
fn check_port(port: u16) -> bool {
    services::port_free(port)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.toml");
            let settings = Settings::load(&settings_path)?;
            if let Err(e) = Layout::new(&settings.root_dir).ensure() {
                eprintln!("could not create {}: {e}", settings.root_dir.display());
            }
            app.manage(AppState {
                settings: RwLock::new(settings),
                settings_path,
                supervisor: Supervisor::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            env_paths,
            list_services,
            start_service,
            stop_service,
            start_all,
            stop_all,
            check_port,
        ])
        .build(tauri::generate_context!())
        .expect("error while building lampRabit")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                let state = app.state::<AppState>();
                tauri::async_runtime::block_on(state.supervisor.shutdown());
            }
        });
}
