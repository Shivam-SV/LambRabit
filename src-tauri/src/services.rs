use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

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
    error::{Error, Result},
    layout::{Layout, exe},
};

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
        },
        ServiceKind::Mariadb => LaunchSpec {
            program: home.join("bin").join(exe("mariadbd")),
            // --defaults-file must be the first argument.
            args: vec![format!("--defaults-file={}", path(layout.etc("mariadb").join("my.ini")))],
            cwd: home,
        },
        ServiceKind::PhpFpm => LaunchSpec {
            program: home.join(exe("php-fpm")),
            args: vec![
                "--nodaemonize".into(),
                "--fpm-config".into(),
                path(layout.etc("php").join("php-fpm.conf")),
            ],
            cwd: home,
        },
    };
    Ok(spec)
}

pub fn port_free(port: u16) -> bool {
    use std::net::TcpListener;
    TcpListener::bind(("0.0.0.0", port)).is_ok() && TcpListener::bind(("127.0.0.1", port)).is_ok()
}

struct Handle {
    pid: Option<u32>,
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
                let state = match handle {
                    Some(h) if h.stop_tx.is_none() => ServiceState::Stopping,
                    Some(_) => ServiceState::Running,
                    None => ServiceState::Stopped,
                };
                let installed = launch_spec(kind, settings, layout)
                    .map(|s| s.program.exists())
                    .unwrap_or(false);
                ServiceInfo {
                    kind,
                    label: kind.label(),
                    state,
                    pid: handle.and_then(|h| h.pid),
                    port: kind.port(settings),
                    version: kind.version(settings),
                    installed,
                }
            })
            .collect()
    }

    pub async fn start(
        &self,
        app: &AppHandle,
        kind: ServiceKind,
        settings: &Settings,
        layout: &Layout,
    ) -> Result<()> {
        let mut running = self.running.lock().await;
        if running.contains_key(&kind) {
            return Ok(());
        }

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

        std::fs::create_dir_all(layout.logs())?;
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
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn()?;
        pipe_output(app.clone(), kind, child.stdout.take(), log.clone());
        pipe_output(app.clone(), kind, child.stderr.take(), log);

        let (stop_tx, stop_rx) = oneshot::channel();
        running.insert(kind, Handle { pid: child.id(), stop_tx: Some(stop_tx) });
        drop(running);
        emit_status(app, kind, ServiceState::Running, None);

        let running = self.running.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let (state, message) = tokio::select! {
                status = child.wait() => match status {
                    Ok(s) if s.success() => (ServiceState::Stopped, None),
                    Ok(s) => (ServiceState::Error, Some(format!("{} exited with {s}", kind.label()))),
                    Err(e) => (ServiceState::Error, Some(e.to_string())),
                },
                _ = stop_rx => {
                    let _ = child.kill().await;
                    (ServiceState::Stopped, None)
                }
            };
            running.lock().await.remove(&kind);
            emit_status(&app, kind, state, message);
        });

        Ok(())
    }

    pub async fn stop(&self, app: &AppHandle, kind: ServiceKind) {
        let mut running = self.running.lock().await;
        if let Some(tx) = running.get_mut(&kind).and_then(|h| h.stop_tx.take()) {
            let _ = tx.send(());
            emit_status(app, kind, ServiceState::Stopping, None);
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
        for _ in 0..100 {
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
