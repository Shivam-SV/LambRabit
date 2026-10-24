mod config;
mod configs;
mod error;
mod layout;
mod procgroup;
mod runtimes;
mod services;

use std::{collections::HashSet, path::PathBuf, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, State};
use tokio::sync::{Mutex, RwLock};

use config::Settings;
use error::{Error, Result};
use layout::Layout;
use runtimes::{Release, Runtime};
use services::{ServiceInfo, ServiceKind, Supervisor};

struct AppState {
    settings: RwLock<Settings>,
    settings_path: PathBuf,
    supervisor: Supervisor,
    http: reqwest::Client,
    installing: Mutex<HashSet<(Runtime, String)>>,
}

impl AppState {
    async fn snapshot(&self) -> (Settings, Layout) {
        let settings = self.settings.read().await.clone();
        let layout = Layout::new(&settings.root_dir);
        (settings, layout)
    }

    async fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut settings = self.settings.write().await;
        f(&mut settings);
        settings.save(&self.settings_path)?;
        Ok(settings.clone())
    }
}

fn active_version(settings: &mut Settings, runtime: Runtime) -> &mut Option<String> {
    match runtime {
        Runtime::Apache => &mut settings.versions.apache,
        Runtime::Php => &mut settings.versions.php,
        Runtime::Mariadb => &mut settings.versions.mariadb,
    }
}

/// Services that load the given runtime and must restart when it changes.
fn dependents(runtime: Runtime) -> &'static [ServiceKind] {
    match runtime {
        Runtime::Apache => &[ServiceKind::Apache],
        Runtime::Php if cfg!(windows) => &[ServiceKind::Apache],
        Runtime::Php => &[ServiceKind::PhpFpm],
        Runtime::Mariadb => &[ServiceKind::Mariadb],
    }
}

/// Drops active versions whose files are gone (e.g. deleted by hand).
fn reconcile_versions(settings: &mut Settings) {
    let layout = Layout::new(&settings.root_dir);
    for runtime in Runtime::ALL {
        let installed = runtimes::installed(&layout, runtime);
        let active = active_version(settings, runtime);
        if active.as_ref().is_none_or(|v| !installed.contains(v)) {
            *active = installed.into_iter().next();
        }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledRuntime {
    runtime: Runtime,
    active: Option<String>,
    versions: Vec<String>,
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn save_settings(state: State<'_, AppState>, mut settings: Settings) -> Result<Settings> {
    Layout::new(&settings.root_dir).ensure()?;
    reconcile_versions(&mut settings);
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
    for info in state.supervisor.list(&settings, &layout).await {
        if !info.installed {
            continue;
        }
        if let Err(e) = state.supervisor.start(&app, info.kind, &settings, &layout).await {
            errors.push(format!("{}: {e}", info.label));
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

#[tauri::command]
async fn runtime_catalog(state: State<'_, AppState>, runtime: Runtime) -> Result<Vec<Release>> {
    runtimes::available(&state.http, runtime).await
}

#[tauri::command]
async fn installed_runtimes(state: State<'_, AppState>) -> Result<Vec<InstalledRuntime>> {
    let (mut settings, layout) = state.snapshot().await;
    Ok(Runtime::ALL
        .into_iter()
        .map(|runtime| InstalledRuntime {
            runtime,
            active: active_version(&mut settings, runtime).clone(),
            versions: runtimes::installed(&layout, runtime),
        })
        .collect())
}

#[tauri::command]
async fn install_runtime(app: AppHandle, state: State<'_, AppState>, release: Release) -> Result<()> {
    let key = (release.runtime, release.version.clone());
    if !state.installing.lock().await.insert(key.clone()) {
        return Err(Error::msg(format!("{} {} is already being installed", release.runtime.id(), release.version)));
    }
    let (_, layout) = state.snapshot().await;
    let result = runtimes::install(&app, &state.http, &layout, &release).await;
    state.installing.lock().await.remove(&key);
    result?;

    // The first installed version of a runtime becomes the active one.
    state
        .update_settings(|s| {
            let active = active_version(s, release.runtime);
            if active.is_none() {
                *active = Some(release.version.clone());
            }
        })
        .await?;
    Ok(())
}

#[tauri::command]
async fn remove_runtime(state: State<'_, AppState>, runtime: Runtime, version: String) -> Result<()> {
    let (mut settings, layout) = state.snapshot().await;
    if active_version(&mut settings, runtime).as_deref() == Some(version.as_str()) {
        for &kind in dependents(runtime) {
            if state.supervisor.is_running(kind).await {
                return Err(Error::msg(format!("Stop {} before removing the version it is using", kind.label())));
            }
        }
    }
    runtimes::remove(&layout, runtime, &version)?;
    state.update_settings(reconcile_versions).await?;
    Ok(())
}

/// Switches the active version, restarting services that were using the old one.
#[tauri::command]
async fn set_active_version(
    app: AppHandle,
    state: State<'_, AppState>,
    runtime: Runtime,
    version: String,
) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    if !runtimes::installed(&layout, runtime).contains(&version) {
        return Err(Error::msg(format!("{} {version} is not installed", runtime.id())));
    }

    let mut restart = Vec::new();
    for &kind in dependents(runtime) {
        if state.supervisor.is_running(kind).await {
            state.supervisor.stop(&app, kind).await;
            restart.push(kind);
        }
    }
    for &kind in &restart {
        for _ in 0..400 {
            if !state.supervisor.is_running(kind).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    let settings = state.update_settings(|s| *active_version(s, runtime) = Some(version)).await?;
    let layout = Layout::new(&settings.root_dir);
    for kind in restart {
        state.supervisor.start(&app, kind, &settings, &layout).await?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.toml");
            let mut settings = Settings::load(&settings_path)?;
            if let Err(e) = Layout::new(&settings.root_dir).ensure() {
                eprintln!("could not create {}: {e}", settings.root_dir.display());
            }
            reconcile_versions(&mut settings);
            app.manage(AppState {
                settings: RwLock::new(settings),
                settings_path,
                supervisor: Supervisor::default(),
                http: runtimes::client(),
                installing: Mutex::new(HashSet::new()),
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
            runtime_catalog,
            installed_runtimes,
            install_runtime,
            remove_runtime,
            set_active_version,
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
