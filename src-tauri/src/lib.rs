mod comfort;
mod config;
mod configs;
mod diagnose;
mod error;
mod extensions;
mod layout;
mod logs;
mod node;
mod php;
mod procgroup;
mod projects;
mod runtimes;
mod services;
mod sites;
mod ssl;
mod tunnels;

use std::{collections::HashSet, path::PathBuf};

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
    /// Cached result of the (slow) trust-store lookup; reset when trust changes.
    ca_trusted: Mutex<Option<bool>>,
    tunnels: tunnels::Tunnels,
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
        Runtime::Nginx => &mut settings.versions.nginx,
        Runtime::Mailpit => &mut settings.versions.mailpit,
        Runtime::Adminer => &mut settings.versions.adminer,
        Runtime::Phpmyadmin => &mut settings.versions.phpmyadmin,
        Runtime::Ngrok => &mut settings.versions.ngrok,
        Runtime::Cloudflared => &mut settings.versions.cloudflared,
        Runtime::Redis => &mut settings.versions.redis,
        Runtime::Dotnet => &mut settings.versions.dotnet,
    }
}

/// Services that load the given runtime and must restart when it changes.
fn dependents(runtime: Runtime) -> &'static [ServiceKind] {
    match runtime {
        Runtime::Apache => &[ServiceKind::Apache],
        Runtime::Nginx => &[ServiceKind::Nginx],
        // PHP pools are owned by the web servers; restarting them restarts PHP.
        Runtime::Php => &[ServiceKind::Apache, ServiceKind::Nginx],
        Runtime::Mariadb => &[ServiceKind::Mariadb],
        Runtime::Mailpit => &[ServiceKind::Mailpit],
        // Served by Apache through their own virtual hosts.
        Runtime::Adminer | Runtime::Phpmyadmin => &[ServiceKind::Apache],
        // Tunnels start on demand; nothing to restart.
        Runtime::Ngrok | Runtime::Cloudflared => &[],
        Runtime::Redis | Runtime::Dotnet => &[ServiceKind::Redis],
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
    /// "windows", "macos" or "linux", so the UI can hide OS-specific options.
    os: &'static str,
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
async fn save_settings(app: AppHandle, state: State<'_, AppState>, mut settings: Settings) -> Result<Settings> {
    check_ports(&settings)?;
    let layout = Layout::new(&settings.root_dir);
    layout.ensure()?;
    reconcile_versions(&mut settings);
    settings.save(&state.settings_path)?;
    *state.settings.write().await = settings.clone();
    *state.ca_trusted.lock().await = None;
    logs::set_system_file(&app, layout.logs().join("lampRabit.log"));
    logs::system(&app, logs::Level::Info, "Settings saved");
    for kind in [ServiceKind::Apache, ServiceKind::Nginx] {
        if !kind.enabled(&settings) && state.supervisor.is_running(kind).await {
            logs::system(&app, logs::Level::Info, format!("{} is switched off in Settings; stopping it", kind.label()));
            state.supervisor.stop(&app, kind).await;
        }
    }
    Ok(settings)
}

/// Rejects settings where two services would listen on the same port.
fn check_ports(settings: &Settings) -> Result<()> {
    use config::Server;
    let p = &settings.ports;
    let mut used: Vec<(u16, String)> = vec![
        (p.mysql, "MariaDB".into()),
        (p.mail_smtp, "Mailpit SMTP".into()),
        (p.mail_ui, "Mailpit inbox".into()),
        (p.redis, "Redis".into()),
        (p.memcached, "Memcached".into()),
    ];
    for server in [Server::Apache, Server::Nginx] {
        if settings.server_enabled(server) {
            let (http, https) = settings.server_ports(server);
            let name = if server == Server::Apache { "Apache" } else { "Nginx" };
            used.push((http, format!("{name} HTTP")));
            used.push((https, format!("{name} HTTPS")));
        }
    }
    let php_block = p.php_fpm..p.php_fpm.saturating_add(100);
    for (i, (port, a)) in used.iter().enumerate() {
        if let Some((_, b)) = used[i + 1..].iter().find(|(q, _)| q == port) {
            return Err(Error::msg(format!("{a} and {b} are both set to port {port}. Give each its own port.")));
        }
        if php_block.contains(port) {
            return Err(Error::msg(format!(
                "{a} (port {port}) overlaps the PHP FastCGI ports ({}-{}). Move one of them.",
                php_block.start,
                php_block.end - 1
            )));
        }
    }
    Ok(())
}

/// Restarts whichever web servers are running, so they (and their PHP pools) pick up
/// changed sites or PHP versions together.
async fn restart_web_servers(app: &AppHandle, state: &AppState) -> Result<()> {
    let mut running = Vec::new();
    for kind in [ServiceKind::Apache, ServiceKind::Nginx] {
        if state.supervisor.is_running(kind).await {
            state.supervisor.stop(app, kind).await;
            running.push(kind);
        }
    }
    for &kind in &running {
        state.supervisor.wait_stopped(kind).await;
    }
    let (settings, layout) = state.snapshot().await;
    for kind in running {
        state.supervisor.start(app, kind, &settings, &layout).await?;
    }
    Ok(())
}

#[tauri::command]
async fn env_paths(state: State<'_, AppState>) -> Result<EnvPaths> {
    let (_, layout) = state.snapshot().await;
    Ok(EnvPaths {
        root: layout.root().to_path_buf(),
        www: layout.www(),
        logs: layout.logs(),
        settings_file: state.settings_path.clone(),
        os: std::env::consts::OS,
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
        if !info.installed || !info.kind.enabled(&settings) {
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
async fn restart_service(app: AppHandle, state: State<'_, AppState>, kind: ServiceKind) -> Result<()> {
    logs::system(&app, logs::Level::Info, format!("Restarting {}", kind.label()));
    state.supervisor.stop(&app, kind).await;
    state.supervisor.wait_stopped(kind).await;
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(&app, kind, &settings, &layout).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SitesInfo {
    sites: Vec<sites::Site>,
    /// Installed developer tools and their domains.
    tools: Vec<sites::Site>,
    tld: String,
    /// Apache is running with a different site list than what is in www/ now.
    needs_apply: bool,
    /// The hosts file is in sync, or does not need to be (`.localhost`, or management off).
    hosts_ok: bool,
    manage_hosts: bool,
    /// None while no site uses HTTPS.
    ca_trusted: Option<bool>,
    ca_path: PathBuf,
}

/// Every folder in www/ is a site, most recently modified first.
#[tauri::command]
async fn list_sites(state: State<'_, AppState>) -> Result<SitesInfo> {
    let (settings, layout) = state.snapshot().await;
    let served = sites::served(&settings, &layout);
    let plan = php::plan(&settings, &layout, &served);
    let mut needs_apply = false;
    for kind in [ServiceKind::Apache, ServiceKind::Nginx] {
        if state.supervisor.is_running(kind).await {
            needs_apply |= sites::config_stale(&settings, &layout, &served, &plan, kind.web_server().unwrap());
        }
    }
    let hosts_ok = !sites::needs_hosts_file(&settings) || sites::hosts_in_sync(&served);
    let (tools, sites): (Vec<_>, Vec<_>) = served.into_iter().partition(|s| s.tool.is_some());
    let ca_trusted = if sites.iter().any(|s| s.ssl) && ssl::ca_thumbprint(&layout).is_some() {
        let mut cached = state.ca_trusted.lock().await;
        if cached.is_none() {
            *cached = Some(ssl::ca_trusted(&layout).await);
        }
        *cached
    } else if sites.iter().any(|s| s.ssl) {
        Some(false)
    } else {
        None
    };
    Ok(SitesInfo {
        sites,
        tools,
        tld: settings.tld.clone(),
        needs_apply,
        hosts_ok,
        manage_hosts: settings.manage_hosts,
        ca_trusted,
        ca_path: ssl::ca_cert_path(&layout),
    })
}

/// Makes Apache and the hosts file match www/: restarts Apache if it is running
/// (which regenerates vhosts and certificates), otherwise just syncs the hosts file.
#[tauri::command]
async fn apply_sites(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    logs::system(&app, logs::Level::Info, "Applying site changes");
    let any_running =
        state.supervisor.is_running(ServiceKind::Apache).await || state.supervisor.is_running(ServiceKind::Nginx).await;
    if any_running {
        restart_web_servers(&app, &state).await
    } else {
        let sites = sites::served(&settings, &layout);
        if sites::needs_hosts_file(&settings) && !sites::hosts_in_sync(&sites) {
            sites::update_hosts(&layout, &sites).await?;
            logs::system(&app, logs::Level::Info, "Hosts file updated");
        }
        Ok(())
    }
}

#[tauri::command]
async fn set_site_ssl(app: AppHandle, state: State<'_, AppState>, name: String, ssl: bool) -> Result<()> {
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().ssl = Some(ssl)).await?;
    logs::system(&app, logs::Level::Info, format!("HTTPS {} for {name}", if ssl { "enabled" } else { "disabled" }));
    restart_web_servers(&app, &state).await
}

/// Sets a site's PHP version (None = follow the active version).
#[tauri::command]
async fn set_site_php(app: AppHandle, state: State<'_, AppState>, name: String, version: Option<String>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    if let Some(v) = &version {
        if !runtimes::installed(&layout, Runtime::Php).contains(v) {
            return Err(Error::msg(format!("PHP {v} is not installed")));
        }
    }
    let label = version.clone().unwrap_or_else(|| "the active version".into());
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().php = version).await?;
    logs::system(&app, logs::Level::Info, format!("{name} now uses PHP {label}"));
    restart_web_servers(&app, &state).await
}

/// Picks the web server for a site when both are enabled.
#[tauri::command]
async fn set_site_server(app: AppHandle, state: State<'_, AppState>, name: String, server: Option<config::Server>) -> Result<()> {
    state.update_settings(|s| s.sites.entry(name.clone()).or_default().server = server).await?;
    logs::system(&app, logs::Level::Info, format!("{name} is now served by {}", match server {
        Some(config::Server::Nginx) => "Nginx",
        Some(config::Server::Apache) => "Apache",
        None => "the default server",
    }));
    restart_web_servers(&app, &state).await
}

/// Creates a project in www/ from a preset and returns its URL. Progress is streamed as
/// `project-progress` events; a failed project is removed so it can be retried.
#[tauri::command]
async fn create_project(app: AppHandle, state: State<'_, AppState>, request: projects::NewProject) -> Result<String> {
    let (_, layout) = state.snapshot().await;
    projects::validate(&request, &layout)?;
    let name = request.name.trim().to_string();
    state
        .update_settings(|s| {
            let site = s.sites.entry(name.clone()).or_default();
            site.php = request.php.clone();
            site.server = request.server;
            site.ssl = Some(request.ssl);
        })
        .await?;
    let (settings, layout) = state.snapshot().await;
    let url = sites::url_for(&settings, &name).ok_or_else(|| Error::msg("invalid project name"))?;
    let ctx = projects::Ctx { app: &app, http: &state.http, settings: &settings, layout: &layout, name: &name };
    logs::system(&app, logs::Level::Info, format!("Creating project {name} ({:?})", request.preset));

    let result = async {
        if let Some(db) = &request.database {
            if !state.supervisor.is_running(ServiceKind::Mariadb).await {
                ctx.step("Starting MariaDB");
                state.supervisor.start(&app, ServiceKind::Mariadb, &settings, &layout).await?;
            }
            // The process is up before it accepts connections.
            for _ in 0..30 {
                if matches!(diagnose::probe_mysql(settings.ports.mysql).await, diagnose::Probe::Ok) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            projects::create_database(&ctx, db).await?;
        }
        projects::build(&ctx, &request, &url).await
    }
    .await;

    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(layout.www().join(&name));
        state.update_settings(|s| { s.sites.remove(&name); }).await?;
        logs::system(&app, logs::Level::Error, format!("Creating project {name} failed: {e}"));
        return Err(e);
    }
    ctx.step("Adding the site to the web server");
    restart_web_servers(&app, &state).await?;
    logs::system(&app, logs::Level::Info, format!("Created project {name} at {url}"));
    Ok(url)
}

/// Shares a site (by folder name) or a local port on a public URL.
#[tauri::command]
async fn tunnel_start(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: tunnels::Provider,
    site: Option<String>,
    port: Option<u16>,
) -> Result<tunnels::Tunnel> {
    let (settings, layout) = state.snapshot().await;
    let site = match site {
        Some(name) => Some(
            sites::served(&settings, &layout)
                .into_iter()
                .find(|s| s.name == name)
                .ok_or_else(|| Error::msg(format!("No site called {name}")))?,
        ),
        None => None,
    };
    if let Some(s) = &site {
        let kind = if s.server == config::Server::Nginx { ServiceKind::Nginx } else { ServiceKind::Apache };
        if !state.supervisor.is_running(kind).await {
            return Err(Error::msg(format!("Start {} first; the tunnel forwards to it.", kind.label())));
        }
    }
    state.tunnels.start(&app, &settings, &layout, provider, site.as_ref(), port).await
}

#[tauri::command]
async fn tunnel_stop(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    state.tunnels.stop(&app, &id).await;
    Ok(())
}

#[tauri::command]
async fn tunnel_list(state: State<'_, AppState>) -> Result<Vec<tunnels::Tunnel>> {
    Ok(state.tunnels.list().await)
}

#[tauri::command]
fn ngrok_token_saved() -> bool {
    tunnels::ngrok_token().is_some()
}

/// Saves the Ngrok authtoken in the system keychain (None removes it).
#[tauri::command]
fn set_ngrok_token(app: AppHandle, token: Option<String>) -> Result<()> {
    tunnels::set_ngrok_token(token.as_deref())?;
    logs::system(&app, logs::Level::Info, if token.is_some() { "Ngrok authtoken saved to the keychain" } else { "Ngrok authtoken removed" });
    Ok(())
}

#[tauri::command]
async fn node_status(app: AppHandle) -> Result<node::NodeStatus> {
    Ok(node::status(&app).await)
}

#[tauri::command]
async fn node_available(state: State<'_, AppState>) -> Result<Vec<node::NodeRelease>> {
    node::available(&state.http).await
}

#[tauri::command]
async fn node_install(app: AppHandle, version: String) -> Result<()> {
    node::install_version(&app, &version).await
}

#[tauri::command]
async fn node_use(app: AppHandle, version: String) -> Result<()> {
    node::use_version(&app, &version).await
}

#[tauri::command]
async fn node_uninstall(app: AppHandle, version: String) -> Result<()> {
    node::uninstall_version(&app, &version).await
}

#[tauri::command]
async fn nvm_install(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    node::install_nvm(&app, &state.http, &layout).await
}

#[tauri::command]
async fn runtime_updates(state: State<'_, AppState>) -> Result<Vec<runtimes::UpdateNotice>> {
    let (_, layout) = state.snapshot().await;
    Ok(runtimes::updates(&state.http, &layout).await)
}

#[tauri::command]
async fn open_terminal(state: State<'_, AppState>, dir: Option<PathBuf>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    comfort::open_terminal(&settings, &layout, &dir.unwrap_or_else(|| layout.www()))
}

#[tauri::command]
async fn open_in_editor(state: State<'_, AppState>, dir: PathBuf) -> Result<()> {
    let (settings, _) = state.snapshot().await;
    comfort::open_in_editor(&settings, &dir).await
}

#[tauri::command]
fn vc_runtime_status() -> comfort::VcRuntime {
    comfort::vc_runtime()
}

#[tauri::command]
async fn vc_runtime_install(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    logs::system(&app, logs::Level::Info, "Installing the Microsoft Visual C++ runtime");
    comfort::install_vc_runtime(&state.http, &layout).await?;
    logs::system(&app, logs::Level::Info, "Microsoft Visual C++ runtime installed");
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open lampRabit", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "start", "Start all", true, None::<&str>)?,
            &MenuItem::with_id(app, "stop", "Stop all", true, None::<&str>)?,
            &MenuItem::with_id(app, "www", "Open www folder", true, None::<&str>)?,
            &MenuItem::with_id(app, "terminal", "Open terminal", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit lampRabit", true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("lampRabit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let app = app.clone();
            match event.id.as_ref() {
                "open" => show_main(&app),
                "quit" => app.exit(0),
                id => {
                    let id = id.to_string();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        match id.as_str() {
                            "start" => {
                                let _ = start_all(app.clone(), state).await;
                            }
                            "stop" => {
                                let _ = stop_all(app.clone(), state).await;
                            }
                            "www" => {
                                let (_, layout) = state.snapshot().await;
                                use tauri_plugin_opener::OpenerExt;
                                let _ = app.opener().open_path(layout.www().to_string_lossy(), None::<&str>);
                            }
                            "terminal" => {
                                let _ = open_terminal(state, None).await;
                            }
                            _ => {}
                        }
                    });
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
async fn php_extensions(state: State<'_, AppState>, version: String) -> Result<Vec<extensions::ExtInfo>> {
    let (settings, layout) = state.snapshot().await;
    extensions::list(&state.http, &settings, &layout, &version).await
}

/// Switches an extension on or off for one PHP version and restarts PHP to apply it.
#[tauri::command]
async fn set_php_extension(app: AppHandle, state: State<'_, AppState>, version: String, name: String, enabled: bool) -> Result<()> {
    state
        .update_settings(|s| {
            let choices = s.php_extensions.entry(version.clone()).or_default();
            if enabled {
                choices.disabled.remove(&name);
                if !extensions::DEFAULTS.contains(&name.as_str()) {
                    choices.enabled.insert(name.clone());
                }
            } else {
                choices.enabled.remove(&name);
                choices.disabled.insert(name.clone());
            }
        })
        .await?;
    logs::system(&app, logs::Level::Info, format!("PHP {version}: {name} {}", if enabled { "enabled" } else { "disabled" }));
    restart_web_servers(&app, &state).await
}

/// Downloads an extra extension for one PHP version and enables it.
#[tauri::command]
async fn install_php_extension(app: AppHandle, state: State<'_, AppState>, version: String, name: String) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    logs::system(&app, logs::Level::Info, format!("Installing {name} for PHP {version}"));
    let ext_version = extensions::install(&state.http, &layout, &version, &name).await?;
    logs::system(&app, logs::Level::Info, format!("Installed {name} {ext_version} for PHP {version}"));
    set_php_extension(app, state, version, name, true).await
}

#[tauri::command]
async fn php_status(state: State<'_, AppState>) -> Result<Vec<php::PoolStatus>> {
    Ok(state.supervisor.php.status().await)
}

/// Adds the local CA to the user's trust store (the OS asks for confirmation).
#[tauri::command]
async fn trust_ca(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    *state.ca_trusted.lock().await = None;
    match ssl::trust_ca(&layout).await {
        Ok(()) => {
            logs::system(&app, logs::Level::Info, "Local certificate authority trusted");
            Ok(())
        }
        Err(e) => {
            logs::system(&app, logs::Level::Warn, e.to_string());
            Err(e)
        }
    }
}

/// Opens the MariaDB command-line client in a new terminal window.
#[tauri::command]
async fn open_db_client(state: State<'_, AppState>) -> Result<()> {
    let (settings, layout) = state.snapshot().await;
    let home = configs::mariadb_home(&settings, &layout)?;
    let client = home.join("bin").join(layout::exe("mariadb"));
    let defaults = format!("--defaults-file={}", layout.etc("mariadb").join("my.ini").display());
    if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "MariaDB"])
            .arg(&client)
            .args([defaults.as_str(), "--user=root"])
            .spawn()?;
        Ok(())
    } else {
        Err(Error::msg(format!(
            "Opening a terminal is not supported on this OS yet. Run: {} {defaults} --user=root",
            client.display()
        )))
    }
}

#[tauri::command]
async fn log_history(app: AppHandle, state: State<'_, AppState>) -> Result<logs::History> {
    let (_, layout) = state.snapshot().await;
    Ok(logs::history(&app, &layout.logs()))
}

#[tauri::command]
async fn clear_logs(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let (_, layout) = state.snapshot().await;
    let failed = logs::clear(&app, &layout.logs());
    logs::system(&app, logs::Level::Info, "Logs cleared");
    if failed.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(format!("Could not clear {} (in use by a running service?)", failed.join(", "))))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PortStatus {
    /// Settings key, e.g. "http" or "mailSmtp".
    key: &'static str,
    port: u16,
    free: bool,
    /// Set when one of lampRabit's own services holds the port.
    used_by_service: Option<&'static str>,
    owner: Option<diagnose::PortOwner>,
}

/// Who, if anyone, is on each configured port.
#[tauri::command]
async fn port_status(state: State<'_, AppState>) -> Result<Vec<PortStatus>> {
    let (settings, layout) = state.snapshot().await;
    let p = &settings.ports;
    let mut ports = vec![("http", p.http), ("https", p.https), ("mysql", p.mysql), ("mailSmtp", p.mail_smtp), ("mailUi", p.mail_ui), ("phpFpm", p.php_fpm), ("redis", p.redis), ("memcached", p.memcached)];
    if settings.web_server == config::WebServer::Both {
        ports.extend([("nginxHttp", p.nginx_http), ("nginxHttps", p.nginx_https)]);
    }
    let mut out = Vec::new();
    for (key, port) in ports {
        let free = services::port_free(port);
        let mut used_by_service = None;
        for kind in ServiceKind::all() {
            if state.supervisor.is_running(kind).await && services::required_ports(kind, &settings, &layout).contains(&port) {
                used_by_service = Some(kind.label());
            }
        }
        if state.supervisor.php.status().await.iter().any(|p| (p.port..p.port + p.workers).contains(&port)) {
            used_by_service = Some("PHP");
        }
        let owner = if free || used_by_service.is_some() { None } else { diagnose::port_owner(port).await };
        out.push(PortStatus { key, port, free, used_by_service, owner });
    }
    Ok(out)
}

/// Stops a process that is blocking a port. Refuses system processes and lampRabit's own.
#[tauri::command]
async fn stop_process(app: AppHandle, state: State<'_, AppState>, pid: u32) -> Result<()> {
    if pid == 0 || pid == 4 || pid == std::process::id() || state.supervisor.pids().await.contains_key(&pid) {
        return Err(Error::msg("lampRabit won't stop that process."));
    }
    let status = if cfg!(windows) {
        let mut cmd = tokio::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/F"]);
        procgroup::prepare(&mut cmd);
        cmd.status().await?
    } else {
        tokio::process::Command::new("kill").arg(pid.to_string()).status().await?
    };
    if !status.success() {
        return Err(Error::msg(format!(
            "Couldn't stop process {pid}. It may belong to another user or need administrator rights."
        )));
    }
    logs::system(&app, logs::Level::Info, format!("Stopped process {pid} to free its port"));
    Ok(())
}

#[tauri::command]
fn clear_problem(state: State<'_, AppState>, kind: ServiceKind) {
    state.supervisor.clear_problem(kind);
}

#[tauri::command]
fn save_text(path: PathBuf, contents: String) -> Result<()> {
    std::fs::write(path, contents)?;
    Ok(())
}

/// A Markdown report of everything useful for troubleshooting.
#[tauri::command]
async fn diagnostics(state: State<'_, AppState>, redact: bool) -> Result<String> {
    use std::fmt::Write;
    let (settings, layout) = state.snapshot().await;
    let mut r = String::new();
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());

    let _ = writeln!(r, "# lampRabit diagnostics\n");
    let generated = now
        .format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))
        .unwrap_or_default();
    let _ = writeln!(r, "Generated {generated}\n");
    let _ = writeln!(r, "## System\n");
    let _ = writeln!(r, "- lampRabit {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(r, "- OS: {} {} ({})", std::env::consts::OS, os_version().await, std::env::consts::ARCH);
    if cfg!(windows) {
        let sys = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
        for dll in ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"] {
            let found = if sys.join(dll).exists() { "found" } else { "MISSING (install the VC++ 2015–2022 Redistributable)" };
            let _ = writeln!(r, "- {dll}: {found}");
        }
    }

    let _ = writeln!(r, "\n## Environment\n");
    let _ = writeln!(r, "- Root: {}", layout.root().display());
    let _ = writeln!(r, "- Domain suffix: .{}", settings.tld);
    let _ = writeln!(r, "- Web server: {:?}", settings.web_server);
    let _ = writeln!(r, "- HTTPS by default: {}", settings.https_by_default);
    let _ = writeln!(r, "- Manage hosts file: {}", settings.manage_hosts);
    let _ = writeln!(r, "- PHP mail() to Mailpit: {}", settings.route_mail);

    let _ = writeln!(r, "\n## Runtimes\n\n| Runtime | Active | Installed |\n|---|---|---|");
    for runtime in Runtime::ALL {
        let mut s = settings.clone();
        let active = active_version(&mut s, runtime).clone().unwrap_or_else(|| "—".into());
        let installed = runtimes::installed(&layout, runtime).join(", ");
        let _ = writeln!(r, "| {} | {active} | {} |", runtime.id(), if installed.is_empty() { "—".into() } else { installed });
    }

    let _ = writeln!(r, "\n## Services\n\n| Service | State | PID | Port | Health | Last problem |\n|---|---|---|---|---|---|");
    for s in state.supervisor.list(&settings, &layout).await {
        let health = s.health.as_ref().map_or("—".to_string(), |h| match &h.detail {
            Some(d) => format!("{} ({d})", h.status),
            None => h.status.to_string(),
        });
        let problem = s.problem.as_ref().map_or("—".to_string(), |p| format!("{}: {}", p.title, p.detail));
        let _ = writeln!(
            r,
            "| {} {} | {:?} | {} | {} | {health} | {} |",
            s.label,
            s.version.as_deref().unwrap_or("(not installed)"),
            s.state,
            s.pid.map_or("—".into(), |p| p.to_string()),
            s.port,
            problem.replace('|', "\\|"),
        );
    }

    let _ = writeln!(r, "\n## Ports\n\n| Setting | Port | Status |\n|---|---|---|");
    for p in port_status(state.clone()).await? {
        let status = if p.free {
            "free".to_string()
        } else if let Some(svc) = p.used_by_service {
            format!("in use by lampRabit ({svc})")
        } else if let Some(o) = &p.owner {
            format!("in use by {} (PID {})", o.name, o.pid)
        } else {
            "in use (owner unknown)".into()
        };
        let _ = writeln!(r, "| {} | {} | {status} |", p.key, p.port);
    }

    let _ = writeln!(r, "\n## PHP workers\n");
    let pools = state.supervisor.php.status().await;
    if pools.is_empty() {
        let _ = writeln!(r, "None running (they start with a web server).");
    }
    for p in pools {
        let _ = writeln!(r, "- PHP {}: {}/{} workers alive, ports {}-{}", p.version, p.alive, p.workers, p.port, p.port + p.workers - 1);
    }

    for kind in [ServiceKind::Apache, ServiceKind::Nginx] {
        let _ = writeln!(r, "\n## {} configuration test\n", kind.label());
        let _ = match services::config_test(kind, &settings, &layout).await {
            None => writeln!(r, "Not run (not installed or never started)."),
            Some(Ok(())) => writeln!(r, "Syntax OK"),
            Some(Err(e)) => writeln!(r, "```text\n{e}\n```"),
        };
    }

    let served = sites::served(&settings, &layout);
    let _ = writeln!(r, "\n## Sites and HTTPS\n");
    let _ = writeln!(r, "- Sites: {}", served.iter().map(|s| s.domain.as_str()).collect::<Vec<_>>().join(", "));
    let _ = writeln!(r, "- Hosts file in sync: {}", !sites::needs_hosts_file(&settings) || sites::hosts_in_sync(&served));
    let _ = writeln!(r, "- Local CA: {}", if ssl::ca_thumbprint(&layout).is_some() {
        if ssl::ca_trusted(&layout).await { "created, trusted" } else { "created, NOT trusted" }
    } else {
        "not created"
    });

    let _ = writeln!(r, "\n## Recent warnings and errors\n");
    for file in ["lampRabit.log", "apache-error.log", "mariadb-error.log", "php-error.log", "mailpit-output.log"] {
        let tail = diagnose::tail(&layout.logs().join(file), 64 * 1024);
        let lines: Vec<&str> = tail
            .lines()
            .filter(|l| {
                let l = l.to_lowercase();
                ["error", "warn", "fatal", "crit", "emerg"].iter().any(|k| l.contains(k))
            })
            .collect();
        let start = lines.len().saturating_sub(15);
        if !lines.is_empty() {
            let _ = writeln!(r, "### {file}\n\n```text\n{}\n```\n", lines[start..].join("\n"));
        }
    }

    if redact {
        if let Some(home) = dirs::home_dir() {
            let home = home.to_string_lossy().to_string();
            r = r.replace(&home, "~").replace(&home.replace('\\', "/"), "~");
        }
        for var in ["USERNAME", "USER"] {
            if let Some(user) = std::env::var_os(var).map(|u| u.to_string_lossy().to_string()).filter(|u| u.len() > 2) {
                r = r.replace(&user, "<user>");
            }
        }
    }
    Ok(r)
}

async fn os_version() -> String {
    let (program, args): (&str, &[&str]) = if cfg!(windows) {
        ("cmd", &["/C", "ver"])
    } else if cfg!(target_os = "macos") {
        ("sw_vers", &["-productVersion"])
    } else {
        ("uname", &["-r"])
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    procgroup::prepare(&mut cmd);
    cmd.output().await.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
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
    let label = format!("{} {}", release.runtime.id(), release.version);
    logs::system(&app, logs::Level::Info, format!("Installing {label} from {}", release.url));
    let result = runtimes::install(&app, &state.http, &layout, &release).await;
    state.installing.lock().await.remove(&key);
    match &result {
        Ok(()) => logs::system(&app, logs::Level::Info, format!("Installed {label}")),
        Err(e) => logs::system(&app, logs::Level::Error, format!("Installing {label} failed: {e}")),
    }
    result?;

    // The first installed version of a runtime becomes the active one.
    let mut activated = false;
    state
        .update_settings(|s| {
            let active = active_version(s, release.runtime);
            if active.is_none() {
                *active = Some(release.version.clone());
                activated = true;
            }
        })
        .await?;
    // Garnet (Redis) runs on .NET; install the runtime privately the first time.
    if release.runtime == Runtime::Redis && state.settings.read().await.versions.dotnet.is_none() {
        let dotnet = runtimes::available(&state.http, Runtime::Dotnet)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::msg("Couldn't find a .NET runtime download"))?;
        logs::system(&app, logs::Level::Info, format!("Installing the .NET {} runtime for Redis", dotnet.version));
        runtimes::install(&app, &state.http, &layout, &dotnet).await?;
        state.update_settings(|s| s.versions.dotnet = Some(dotnet.version.clone())).await?;
    }
    // A new tool brings a domain and, for Mailpit, PHP mail settings: Apache picks both up on restart.
    if activated && is_tool(release.runtime) {
        restart_if_running(&app, &state, ServiceKind::Apache).await?;
    }
    Ok(())
}

fn is_tool(runtime: Runtime) -> bool {
    matches!(runtime, Runtime::Mailpit | Runtime::Adminer | Runtime::Phpmyadmin)
}

async fn restart_if_running(app: &AppHandle, state: &AppState, kind: ServiceKind) -> Result<()> {
    if !state.supervisor.is_running(kind).await {
        return Ok(());
    }
    state.supervisor.stop(app, kind).await;
    state.supervisor.wait_stopped(kind).await;
    let (settings, layout) = state.snapshot().await;
    state.supervisor.start(app, kind, &settings, &layout).await
}

#[tauri::command]
async fn remove_runtime(app: AppHandle, state: State<'_, AppState>, runtime: Runtime, version: String) -> Result<()> {
    let (mut settings, layout) = state.snapshot().await;
    let was_active = active_version(&mut settings, runtime).as_deref() == Some(version.as_str());
    // Web tools are just files served by Apache, which restarts below instead of blocking.
    if was_active && !matches!(runtime, Runtime::Adminer | Runtime::Phpmyadmin) {
        for &kind in dependents(runtime) {
            if state.supervisor.is_running(kind).await {
                return Err(Error::msg(format!("Stop {} before removing the version it is using", kind.label())));
            }
        }
    }
    runtimes::remove(&layout, runtime, &version)?;
    logs::system(&app, logs::Level::Info, format!("Removed {} {version}", runtime.id()));
    state.update_settings(reconcile_versions).await?;
    if was_active && is_tool(runtime) {
        restart_if_running(&app, &state, ServiceKind::Apache).await?;
    }
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
        state.supervisor.wait_stopped(kind).await;
    }

    logs::system(&app, logs::Level::Info, format!("Switching {} to {version}", runtime.id()));
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .on_window_event(|window, event| {
            // Closing keeps lampRabit (and its services) running in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let to_tray = tauri::async_runtime::block_on(async { state.settings.read().await.close_to_tray });
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let settings_path = config_dir.join("settings.toml");
            // Pre-release builds used the identifier "dev.lamprabit.app"; carry their settings over.
            if !settings_path.exists() {
                if let Some(old) = config_dir.parent().map(|p| p.join("dev.lamprabit.app").join("settings.toml")) {
                    if old.exists() {
                        let _ = std::fs::create_dir_all(&config_dir);
                        let _ = std::fs::copy(&old, &settings_path);
                    }
                }
            }
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
                ca_trusted: Mutex::new(None),
                tunnels: tunnels::Tunnels::default(),
            });
            app.manage(logs::LogHub::default());
            {
                let root = app.state::<AppState>().settings.try_read().map(|s| s.root_dir.clone()).unwrap_or_default();
                logs::set_system_file(app.handle(), Layout::new(root).logs().join("lampRabit.log"));
            }
            logs::system(app.handle(), logs::Level::Info, format!("lampRabit {} started on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS));

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let state = handle.state::<AppState>();
                    let (settings, _) = state.snapshot().await;
                    state.supervisor.check_health(&handle, &settings, &state.http).await;
                }
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut tailer = logs::Tailer::default();
                loop {
                    let dir = Layout::new(&handle.state::<AppState>().settings.read().await.root_dir).logs();
                    tailer.poll(&handle, &dir);
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                }
            });

            setup_tray(app)?;
            // Started with the OS: stay in the tray.
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                if state.settings.read().await.start_services_on_launch {
                    logs::system(&handle, logs::Level::Info, "Starting services on launch");
                    let _ = start_all(handle.clone(), state).await;
                }
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
            port_status,
            stop_process,
            clear_problem,
            diagnostics,
            save_text,
            log_history,
            clear_logs,
            restart_service,
            list_sites,
            apply_sites,
            set_site_ssl,
            set_site_php,
            set_site_server,
            php_status,
            create_project,
            php_extensions,
            set_php_extension,
            install_php_extension,
            open_terminal,
            open_in_editor,
            vc_runtime_status,
            vc_runtime_install,
            runtime_updates,
            node_status,
            node_available,
            node_install,
            node_use,
            node_uninstall,
            nvm_install,
            tunnel_start,
            tunnel_stop,
            tunnel_list,
            ngrok_token_saved,
            set_ngrok_token,
            trust_ca,
            open_db_client,
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
                tauri::async_runtime::block_on(async {
                    state.tunnels.stop_all().await;
                    state.supervisor.shutdown().await;
                });
            }
        });
}
