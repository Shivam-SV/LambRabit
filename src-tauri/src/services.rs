use std::{collections::HashMap, ffi::OsString, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    process::Command,
    sync::{Mutex, oneshot},
};

use crate::{
    config::Settings,
    configs,
    error::{Error, Result},
    layout::{Layout, exe},
    procgroup::{self, ProcGroup},
};

/// How long a graceful stop may take before the process tree is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceKind {
    Apache,
    Mariadb,
    /// Windows loads PHP into Apache as a module; elsewhere it runs as php-fpm.
    PhpFpm,
}

impl ServiceKind {
    pub fn all() -> Vec<Self> {
        let mut kinds = vec![Self::Apache, Self::Mariadb];
        if !cfg!(windows) {
            kinds.push(Self::PhpFpm);
        }
        kinds
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Mariadb => "mariadb",
            Self::PhpFpm => "php-fpm",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Apache => "Apache",
            Self::Mariadb => "MariaDB",
            Self::PhpFpm => "PHP-FPM",
        }
    }

    fn runtime(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Mariadb => "mariadb",
            Self::PhpFpm => "php",
        }
    }

    fn version(self, settings: &Settings) -> Option<String> {
        match self {
            Self::Apache => settings.versions.apache.clone(),
            Self::Mariadb => settings.versions.mariadb.clone(),
            Self::PhpFpm => settings.versions.php.clone(),
        }
    }

    fn port(self, settings: &Settings) -> u16 {
        match self {
            Self::Apache => settings.ports.http,
            Self::Mariadb => settings.ports.mysql,
            Self::PhpFpm => settings.ports.php_fpm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub kind: ServiceKind,
    pub label: &'static str,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub port: u16,
    pub version: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEvent {
    pub kind: ServiceKind,
    pub state: ServiceState,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub kind: ServiceKind,
    pub line: String,
}

struct LaunchSpec {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    /// Directories prepended to PATH (e.g. PHP's DLLs for mod_php).
    path_prepend: Vec<PathBuf>,
    /// Command that asks the service to shut down cleanly.
    stop_cmd: Option<(PathBuf, Vec<String>)>,
}

fn launch_spec(kind: ServiceKind, settings: &Settings, layout: &Layout) -> Result<LaunchSpec> {
    let version = kind
        .version(settings)
        .ok_or_else(|| Error::msg(format!("{} is not installed", kind.label())))?;
    let home = layout.bin(kind.runtime(), &version);
    let path = |p: PathBuf| p.to_string_lossy().into_owned();

    let spec = match kind {
        ServiceKind::Apache => LaunchSpec {
            program: home.join("bin").join(exe("httpd")),
            args: vec![
                "-d".into(),
                path(home.clone()),
                "-f".into(),
                path(layout.etc("apache").join("httpd.conf")),
            ],
            cwd: home,
            path_prepend: settings.versions.php.iter().map(|v| layout.bin("php", v)).collect(),
            stop_cmd: None,
        },
        ServiceKind::Mariadb => {
            let defaults = format!("--defaults-file={}", path(layout.etc("mariadb").join("my.ini")));
            LaunchSpec {
                program: home.join("bin").join(exe("mariadbd")),
                // --defaults-file must be the first argument.
                args: vec![defaults.clone()],
                stop_cmd: Some((
                    home.join("bin").join(exe("mariadb-admin")),
                    vec![defaults, "--user=root".into(), "shutdown".into()],
                )),
                cwd: home,
                path_prepend: vec![],
            }
        }
        ServiceKind::PhpFpm => LaunchSpec {
            program: home.join(exe("php-fpm")),
            args: vec![
                "--nodaemonize".into(),
                "--fpm-config".into(),
                path(layout.etc("php").join("php-fpm.conf")),
                "-c".into(),
                path(layout.etc("php").join("php.ini")),
            ],
            cwd: home,
            path_prepend: vec![],
            stop_cmd: None,
        },
    };
    Ok(spec)
}

/// Writes configs and does first-run setup for a service.
async fn prepare(kind: ServiceKind, settings: &Settings, layout: &Layout) -> Result<()> {
    layout.ensure()?;
    match kind {
        ServiceKind::Apache => {
            if cfg!(windows) && settings.versions.php.is_some() {
                configs::write_php(settings, layout)?;
            }
            configs::write_apache(settings, layout)
        }
        ServiceKind::Mariadb => {
            configs::write_mariadb(settings, layout)?;
            configs::init_mariadb_data(settings, layout).await
        }
        ServiceKind::PhpFpm => configs::write_php(settings, layout),
    }
}

pub fn port_free(port: u16) -> bool {
    use std::net::TcpListener;
    TcpListener::bind(("0.0.0.0", port)).is_ok() && TcpListener::bind(("127.0.0.1", port)).is_ok()
}

struct Handle {
    pid: Option<u32>,
    state: ServiceState,
    /// Taken when a stop has been requested.
    stop_tx: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct Supervisor {
    running: Arc<Mutex<HashMap<ServiceKind, Handle>>>,
}

impl Supervisor {
    pub async fn list(&self, settings: &Settings, layout: &Layout) -> Vec<ServiceInfo> {
        let running = self.running.lock().await;
        ServiceKind::all()
            .into_iter()
            .map(|kind| {
                let handle = running.get(&kind);
                let installed = launch_spec(kind, settings, layout)
                    .map(|s| s.program.exists())
                    .unwrap_or(false);
                ServiceInfo {
                    kind,
                    label: kind.label(),
                    state: handle.map_or(ServiceState::Stopped, |h| h.state),
                    pid: handle.and_then(|h| h.pid),
                    port: kind.port(settings),
                    version: kind.version(settings),
                    installed,
                }
            })
            .collect()
    }

    pub async fn is_running(&self, kind: ServiceKind) -> bool {
        self.running.lock().await.contains_key(&kind)
    }

    pub async fn start(
        &self,
        app: &AppHandle,
        kind: ServiceKind,
        settings: &Settings,
        layout: &Layout,
    ) -> Result<()> {
        {
            let mut running = self.running.lock().await;
            if running.contains_key(&kind) {
                return Ok(());
            }
            // Reserve the slot so concurrent starts don't race during setup.
            running.insert(kind, Handle { pid: None, state: ServiceState::Starting, stop_tx: None });
        }
        emit_status(app, kind, ServiceState::Starting, None);

        match self.spawn(app, kind, settings, layout).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.running.lock().await.remove(&kind);
                emit_status(app, kind, ServiceState::Stopped, None);
                Err(e)
            }
        }
    }

    async fn spawn(&self, app: &AppHandle, kind: ServiceKind, settings: &Settings, layout: &Layout) -> Result<()> {
        let spec = launch_spec(kind, settings, layout)?;
        if !spec.program.exists() {
            return Err(Error::msg(format!(
                "{} binary not found at {}",
                kind.label(),
                spec.program.display()
            )));
        }
        let port = kind.port(settings);
        if !port_free(port) {
            return Err(Error::msg(format!("Port {port} is already in use")));
        }
        prepare(kind, settings, layout).await?;

        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(layout.logs().join(format!("{}.log", kind.id())))
            .await?;
        let log = Arc::new(Mutex::new(log));

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if !spec.path_prepend.is_empty() {
            let existing = std::env::var_os("PATH").unwrap_or_default();
            let paths = spec.path_prepend.iter().cloned().chain(std::env::split_paths(&existing));
            cmd.env("PATH", std::env::join_paths(paths).unwrap_or(OsString::from(existing)));
        }
        procgroup::prepare(&mut cmd);

        let mut child = cmd.spawn()?;
        let group = ProcGroup::attach(&child);
        if let Err(e) = &group {
            eprintln!("could not attach {} to a process group: {e}", kind.label());
        }
        pipe_output(app.clone(), kind, child.stdout.take(), log.clone());
        pipe_output(app.clone(), kind, child.stderr.take(), log);

        let (stop_tx, stop_rx) = oneshot::channel();
        self.running.lock().await.insert(
            kind,
            Handle { pid: child.id(), state: ServiceState::Running, stop_tx: Some(stop_tx) },
        );
        emit_status(app, kind, ServiceState::Running, None);

        let running = self.running.clone();
        let app = app.clone();
        let stop_cmd = spec.stop_cmd;
        tauri::async_runtime::spawn(async move {
            let group = group.ok();
            let (state, message) = tokio::select! {
                status = child.wait() => match status {
                    Ok(s) if s.success() => (ServiceState::Stopped, None),
                    Ok(s) => (
                        ServiceState::Error,
                        Some(format!("{} exited with {s}. See the Logs page for details.", kind.label())),
                    ),
                    Err(e) => (ServiceState::Error, Some(e.to_string())),
                },
                _ = stop_rx => {
                    let asked = match &stop_cmd {
                        Some((program, args)) => {
                            let mut c = Command::new(program);
                            c.args(args).stdout(Stdio::null()).stderr(Stdio::null());
                            procgroup::prepare(&mut c);
                            c.status().await.is_ok_and(|s| s.success())
                        }
                        None => group.as_ref().is_some_and(|g| g.signal_term()),
                    };
                    if !asked || tokio::time::timeout(STOP_TIMEOUT, child.wait()).await.is_err() {
                        match &group {
                            Some(g) => g.kill(),
                            None => { let _ = child.start_kill(); }
                        }
                    }
                    let _ = child.wait().await;
                    (ServiceState::Stopped, None)
                }
            };
            // Clean up anything the service left behind (e.g. Apache's worker process).
            if let Some(g) = &group {
                g.kill();
            }
            drop(group);
            running.lock().await.remove(&kind);
            emit_status(&app, kind, state, message);
        });

        Ok(())
    }

    pub async fn stop(&self, app: &AppHandle, kind: ServiceKind) {
        let mut running = self.running.lock().await;
        if let Some(handle) = running.get_mut(&kind) {
            if let Some(tx) = handle.stop_tx.take() {
                handle.state = ServiceState::Stopping;
                let _ = tx.send(());
                emit_status(app, kind, ServiceState::Stopping, None);
            }
        }
    }

    /// Stops everything and waits (bounded) for the processes to exit.
    pub async fn shutdown(&self) {
        {
            let mut running = self.running.lock().await;
            for handle in running.values_mut() {
                if let Some(tx) = handle.stop_tx.take() {
                    let _ = tx.send(());
                }
            }
        }
        let deadline = tokio::time::Instant::now() + STOP_TIMEOUT + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            if self.running.lock().await.is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

fn emit_status(app: &AppHandle, kind: ServiceKind, state: ServiceState, message: Option<String>) {
    let _ = app.emit("service-status", StatusEvent { kind, state, message });
}

fn pipe_output<R>(app: AppHandle, kind: ServiceKind, reader: Option<R>, log: Arc<Mutex<File>>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let Some(reader) = reader else { return };
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = log.lock().await.write_all(format!("{line}\n").as_bytes()).await;
            let _ = app.emit("service-log", LogEvent { kind, line });
        }
    });
}
