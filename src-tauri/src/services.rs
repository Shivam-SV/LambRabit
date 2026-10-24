use std::{collections::HashMap, ffi::OsString, path::{Path, PathBuf}, process::Stdio, sync::Arc, time::Duration};

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
    config::Server,
    configs,
    diagnose::{self, Probe, Problem},
    php::{self, PhpPools, PoolPlan},
    error::{Error, Result},
    logs::{self, Level},
    sites,
    layout::{Layout, exe},
    procgroup::{self, ProcGroup},
};

/// How long a graceful stop may take before the process tree is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceKind {
    Apache,
    Nginx,
    Mariadb,
    /// Catches outgoing mail (SMTP) and shows it in a web inbox.
    Mailpit,
    /// Redis-compatible cache (Garnet).
    Redis,
    /// Memcached from the system (macOS/Linux).
    Memcached,
}

impl ServiceKind {
    pub fn all() -> Vec<Self> {
        vec![Self::Apache, Self::Nginx, Self::Mariadb, Self::Mailpit, Self::Redis, Self::Memcached]
    }

    pub fn web_server(self) -> Option<Server> {
        match self {
            Self::Apache => Some(Server::Apache),
            Self::Nginx => Some(Server::Nginx),
            _ => None,
        }
    }

    /// Web servers only run when Settings → Web server includes them.
    pub fn enabled(self, settings: &Settings) -> bool {
        self.web_server().is_none_or(|s| settings.server_enabled(s))
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Mariadb => "mariadb",
            Self::Mailpit => "mailpit",
            Self::Redis => "redis",
            Self::Memcached => "memcached",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Apache => "Apache",
            Self::Nginx => "Nginx",
            Self::Mariadb => "MariaDB",
            Self::Mailpit => "Mailpit",
            Self::Redis => "Redis",
            Self::Memcached => "Memcached",
        }
    }

    fn runtime(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Mariadb => "mariadb",
            Self::Mailpit => "mailpit",
            Self::Redis => "redis",
            Self::Memcached => "memcached",
        }
    }

    fn version(self, settings: &Settings) -> Option<String> {
        match self {
            Self::Apache => settings.versions.apache.clone(),
            Self::Nginx => settings.versions.nginx.clone(),
            Self::Mariadb => settings.versions.mariadb.clone(),
            Self::Mailpit => settings.versions.mailpit.clone(),
            Self::Redis => settings.versions.redis.clone(),
            Self::Memcached => system_memcached().map(|(_, v)| v),
        }
    }

    pub fn config_path(self, layout: &Layout) -> PathBuf {
        match self {
            Self::Apache => layout.etc("apache").join("httpd.conf"),
            Self::Nginx => layout.etc("nginx").join("nginx.conf"),
            Self::Mariadb => layout.etc("mariadb").join("my.ini"),
            // Configured on the command line; their data folders.
            Self::Mailpit => layout.data("mailpit"),
            Self::Redis => layout.data("redis"),
            Self::Memcached => layout.data("memcached"),
        }
    }

    /// The service's own error log (stdout/stderr go to `<id>-output.log`).
    pub fn log_path(self, layout: &Layout) -> PathBuf {
        let file = match self {
            Self::Apache => "apache-error.log",
            Self::Nginx => "nginx-error.log",
            Self::Mariadb => "mariadb-error.log",
            Self::Mailpit => "mailpit-output.log",
            Self::Redis => "redis-output.log",
            Self::Memcached => "memcached-output.log",
        };
        layout.logs().join(file)
    }

    fn port(self, settings: &Settings) -> u16 {
        match self {
            Self::Apache => settings.server_ports(Server::Apache).0,
            Self::Nginx => settings.server_ports(Server::Nginx).0,
            Self::Mariadb => settings.ports.mysql,
            Self::Mailpit => settings.ports.mail_smtp,
            Self::Redis => settings.ports.redis,
            Self::Memcached => settings.ports.memcached,
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
    /// Unix time in milliseconds when the service came up.
    pub started_at: Option<u64>,
    pub config_path: PathBuf,
    pub log_path: PathBuf,
    /// Result of the latest health probe while running.
    pub health: Option<Health>,
    /// Why the last start failed or the service stopped unexpectedly.
    pub problem: Option<Problem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// "checking" right after start, then "healthy" or "degraded".
    pub status: &'static str,
    pub detail: Option<String>,
    pub checked_at: u64,
    #[serde(skip)]
    failures: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEvent {
    pub kind: ServiceKind,
    pub state: ServiceState,
    pub message: Option<String>,
}

struct LaunchSpec {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    /// Directories prepended to PATH (e.g. PHP's DLLs for mod_php).
    path_prepend: Vec<PathBuf>,
    /// Command that asks the service to shut down cleanly.
    stop_cmd: Option<(PathBuf, Vec<String>)>,
    env: Vec<(&'static str, String)>,
}

/// Memcached isn't downloadable (no maintained builds for Windows; macOS/Linux users
/// install it with their package manager), so an installed `memcached` is used.
pub fn system_memcached() -> Option<(PathBuf, String)> {
    if cfg!(windows) {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    let bin = std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin".into(), "/usr/local/bin".into(), "/usr/bin".into()])
        .map(|d| d.join("memcached"))
        .find(|p| p.exists())?;
    let out = std::process::Command::new(&bin).arg("-V").output().ok()?;
    let version = String::from_utf8_lossy(&out.stdout).split_whitespace().nth(1).unwrap_or("system").to_string();
    Some((bin, version))
}

fn launch_spec(kind: ServiceKind, settings: &Settings, layout: &Layout) -> Result<LaunchSpec> {
    if kind == ServiceKind::Memcached {
        let (bin, _) = system_memcached().ok_or_else(|| {
            Error::msg(if cfg!(windows) {
                "Memcached has no maintained Windows builds."
            } else {
                "Install memcached with your package manager (brew install memcached / apt install memcached)."
            })
        })?;
        return Ok(LaunchSpec {
            cwd: bin.parent().unwrap_or(Path::new("/")).to_path_buf(),
            program: bin,
            args: vec!["-l".into(), "127.0.0.1".into(), "-p".into(), settings.ports.memcached.to_string(), "-m".into(), "128".into()],
            path_prepend: vec![],
            stop_cmd: None,
            env: vec![],
        });
    }
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
            path_prepend: vec![],
            stop_cmd: None,
            env: vec![],
        },
        ServiceKind::Nginx => {
            let prefix = format!("{}/", path(layout.tmp().join("nginx")));
            let conf = path(layout.etc("nginx").join("nginx.conf"));
            LaunchSpec {
                program: home.join(exe("nginx")),
                args: vec!["-p".into(), prefix.clone(), "-c".into(), conf.clone()],
                stop_cmd: Some((home.join(exe("nginx")), vec!["-p".into(), prefix, "-c".into(), conf, "-s".into(), "quit".into()])),
                cwd: home,
                path_prepend: vec![],
                env: vec![],
            }
        }
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
                env: vec![],
            }
        }
        ServiceKind::Mailpit => LaunchSpec {
            program: home.join(exe("mailpit")),
            args: vec![
                "--smtp".into(),
                format!("127.0.0.1:{}", settings.ports.mail_smtp),
                "--listen".into(),
                format!("127.0.0.1:{}", settings.ports.mail_ui),
                "--database".into(),
                path(layout.data("mailpit").join("mailpit.db")),
                // Frameworks often send credentials even locally; accept anything.
                "--smtp-auth-accept-any".into(),
                "--smtp-auth-allow-insecure".into(),
            ],
            cwd: home,
            path_prepend: vec![],
            stop_cmd: None,
            env: vec![],
        },
        ServiceKind::Redis => {
            let dotnet = settings
                .versions
                .dotnet
                .as_ref()
                .map(|v| layout.bin("dotnet", v))
                .ok_or_else(|| Error::msg("Redis needs the .NET runtime; reinstall Redis from Services to add it."))?;
            LaunchSpec {
                program: home.join(exe("GarnetServer")),
                args: vec![
                    "--bind".into(),
                    "127.0.0.1".into(),
                    "--port".into(),
                    settings.ports.redis.to_string(),
                ],
                cwd: home,
                path_prepend: vec![],
                stop_cmd: None,
                env: vec![("DOTNET_ROOT", path(dotnet))],
            }
        }
        ServiceKind::Memcached => unreachable!("handled above"),
    };
    Ok(spec)
}

/// Writes configs and does first-run setup for a service. Web servers return the PHP
/// pools their sites need.
async fn prepare(app: &AppHandle, kind: ServiceKind, settings: &Settings, layout: &Layout) -> Result<Vec<PoolPlan>> {
    layout.ensure()?;
    match kind {
        ServiceKind::Apache | ServiceKind::Nginx => {
            let server = kind.web_server().unwrap();
            configs::write_tools(settings, layout)?;
            let sites = sites::served(settings, layout);
            let plan = php::plan(settings, layout, &sites);
            if sites::write_config(settings, layout, &sites, &plan, server)? {
                logs::system(app, Level::Info, "Created the local certificate authority for HTTPS sites");
            }
            if kind == ServiceKind::Apache {
                configs::write_apache(settings, layout, &plan)?;
                let _ = std::fs::remove_file(configs::apache_pid_file(layout));
            } else {
                configs::nginx_prefix(layout)?;
            }
            sync_hosts(app, settings, layout, &sites).await;
            Ok(plan)
        }
        ServiceKind::Mariadb => {
            configs::write_mariadb(settings, layout)?;
            configs::init_mariadb_data(settings, layout).await?;
            Ok(vec![])
        }
        ServiceKind::Mailpit | ServiceKind::Redis | ServiceKind::Memcached => {
            std::fs::create_dir_all(kind.config_path(layout))?;
            Ok(vec![])
        }
    }
}

/// Every port a service will listen on.
pub fn required_ports(kind: ServiceKind, settings: &Settings, layout: &Layout) -> Vec<u16> {
    let mut ports = vec![kind.port(settings)];
    match kind {
        ServiceKind::Apache | ServiceKind::Nginx => {
            let server = kind.web_server().unwrap();
            if sites::served(settings, layout).iter().any(|s| s.ssl && s.server == server) {
                ports.push(settings.server_ports(server).1);
            }
        }
        ServiceKind::Mailpit => ports.push(settings.ports.mail_ui),
        _ => {}
    }
    ports
}

/// `httpd -t`: catches config mistakes (including user files in etc/apache/sites/)
/// before Apache is started for real.
/// Apache's config test for the diagnostics report: None when Apache isn't installed or
/// has never been started (no generated config yet).
pub async fn config_test(kind: ServiceKind, settings: &Settings, layout: &Layout) -> Option<std::result::Result<(), String>> {
    let spec = launch_spec(kind, settings, layout).ok()?;
    if !spec.program.exists() || !kind.config_path(layout).exists() {
        return None;
    }
    Some(match server_config_test(&spec).await {
        None => Ok(()),
        Some(err) => Err(err),
    })
}

/// `httpd -t` / `nginx -t`. None when the config is valid, otherwise the error output.
async fn server_config_test(spec: &LaunchSpec) -> Option<String> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args).arg("-t").current_dir(&spec.cwd);
    if !spec.path_prepend.is_empty() {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let paths = spec.path_prepend.iter().cloned().chain(std::env::split_paths(&existing));
        cmd.env("PATH", std::env::join_paths(paths).unwrap_or(OsString::from(existing)));
    }
    procgroup::prepare(&mut cmd);
    let out = cmd.output().await.ok()?;
    if out.status.success() {
        return None;
    }
    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    Some(text.trim().to_string())
}

/// Brings the hosts file in line with the site list. A failure (e.g. a cancelled admin
/// prompt) is logged but never blocks Apache from starting.
pub async fn sync_hosts(app: &AppHandle, settings: &Settings, layout: &Layout, sites: &[sites::Site]) {
    if !settings.manage_hosts || !sites::needs_hosts_file(settings) || sites::hosts_in_sync(sites) {
        return;
    }
    logs::system(app, Level::Info, format!("Updating the hosts file for {} site(s)", sites.len()));
    match sites::update_hosts(layout, sites).await {
        Ok(()) => logs::system(app, Level::Info, "Hosts file updated"),
        Err(e) => logs::system(app, Level::Warn, format!("{e} Sites may not resolve until the hosts file is updated.")),
    }
}

pub fn port_free(port: u16) -> bool {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
    // A test bind alone can succeed on Windows while another program listens on the
    // IPv6 or loopback address, so first see whether anything answers.
    let answers = |addr: SocketAddr| TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_ok();
    if answers((Ipv4Addr::LOCALHOST, port).into()) || answers((Ipv6Addr::LOCALHOST, port).into()) {
        return false;
    }
    TcpListener::bind(("0.0.0.0", port)).is_ok() && TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

struct Handle {
    pid: Option<u32>,
    state: ServiceState,
    started_at: Option<u64>,
    /// Taken when a stop has been requested.
    stop_tx: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct Supervisor {
    running: Arc<Mutex<HashMap<ServiceKind, Handle>>>,
    /// PHP FastCGI pools, started and stopped with the web servers.
    pub php: Arc<PhpPools>,
    problems: Arc<std::sync::Mutex<HashMap<ServiceKind, Problem>>>,
    health: Arc<std::sync::Mutex<HashMap<ServiceKind, Health>>>,
}

impl Supervisor {
    pub async fn list(&self, settings: &Settings, layout: &Layout) -> Vec<ServiceInfo> {
        let running = self.running.lock().await;
        let problems = self.problems.lock().unwrap().clone();
        let health = self.health.lock().unwrap().clone();
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
                    started_at: handle.and_then(|h| h.started_at),
                    config_path: kind.config_path(layout),
                    log_path: kind.log_path(layout),
                    health: handle.filter(|h| h.state == ServiceState::Running).and_then(|_| health.get(&kind).cloned()),
                    problem: problems.get(&kind).cloned(),
                }
            })
            .collect()
    }

    pub fn clear_problem(&self, kind: ServiceKind) {
        self.problems.lock().unwrap().remove(&kind);
    }

    fn set_problem(&self, kind: ServiceKind, problem: Problem) {
        self.problems.lock().unwrap().insert(kind, problem);
    }

    /// PIDs of everything lampRabit runs, to recognise its own ports.
    pub async fn pids(&self) -> HashMap<u32, ServiceKind> {
        self.running.lock().await.iter().filter_map(|(k, h)| Some((h.pid?, *k))).collect()
    }

    /// Probes every running service and emits `service-health` when a status changes.
    pub async fn check_health(&self, app: &AppHandle, settings: &Settings, client: &reqwest::Client) {
        let running: Vec<(ServiceKind, u64)> = self
            .running
            .lock()
            .await
            .iter()
            .filter(|(_, h)| h.state == ServiceState::Running)
            .filter_map(|(k, h)| Some((*k, h.started_at?)))
            .collect();
        self.health.lock().unwrap().retain(|k, _| running.iter().any(|(r, _)| r == k));

        for (kind, started_at) in running {
            let probe = match kind {
                ServiceKind::Apache => {
                    diagnose::probe_http(client, &format!("http://127.0.0.1:{}/", settings.ports.http)).await
                }
                ServiceKind::Mariadb => diagnose::probe_mysql(settings.ports.mysql).await,
                ServiceKind::Nginx => {
                    diagnose::probe_http(client, &format!("http://127.0.0.1:{}/", settings.server_ports(Server::Nginx).0)).await
                }
                ServiceKind::Mailpit => {
                    diagnose::probe_http(client, &format!("http://127.0.0.1:{}/livez", settings.ports.mail_ui)).await
                }
                ServiceKind::Redis => diagnose::probe_line(settings.ports.redis, b"PING\r\n", "+PONG").await,
                ServiceKind::Memcached => diagnose::probe_line(settings.ports.memcached, b"version\r\n", "VERSION").await,
            };
            let young = now_ms().saturating_sub(started_at) < 8_000;
            let changed = {
                let mut map = self.health.lock().unwrap();
                let prev = map.get(&kind).cloned();
                let failures = match &probe {
                    Probe::Ok => 0,
                    Probe::Bad(_) => prev.as_ref().map_or(0, |p| p.failures) + 1,
                };
                // One failed probe can be a blip; two in a row is a real problem.
                let (status, detail) = match probe {
                    Probe::Ok => ("healthy", None),
                    Probe::Bad(_) if young => ("checking", None),
                    Probe::Bad(d) if failures >= 2 => ("degraded", Some(d)),
                    Probe::Bad(_) => prev.as_ref().map_or(("checking", None), |p| (p.status, p.detail.clone())),
                };
                let changed = prev.as_ref().is_none_or(|p| p.status != status || p.detail != detail);
                if changed && status == "degraded" {
                    logs::system(app, Level::Warn, format!("{} is not responding: {}", kind.label(), detail.clone().unwrap_or_default()));
                }
                if changed && status == "healthy" && prev.as_ref().is_some_and(|p| p.status == "degraded") {
                    logs::system(app, Level::Info, format!("{} is responding again", kind.label()));
                }
                map.insert(kind, Health { status, detail, checked_at: now_ms(), failures });
                changed
            };
            if changed {
                let _ = app.emit("service-health", kind);
            }
        }
    }

    pub async fn is_running(&self, kind: ServiceKind) -> bool {
        self.running.lock().await.contains_key(&kind)
    }

    /// Waits until a stopping service has fully exited.
    pub async fn wait_stopped(&self, kind: ServiceKind) {
        let deadline = tokio::time::Instant::now() + STOP_TIMEOUT + Duration::from_secs(2);
        while self.is_running(kind).await && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
            running.insert(
                kind,
                Handle { pid: None, state: ServiceState::Starting, started_at: None, stop_tx: None },
            );
        }
        self.clear_problem(kind);
        self.health.lock().unwrap().remove(&kind);
        emit_status(app, kind, ServiceState::Starting, None);
        let version = kind.version(settings).unwrap_or_default();
        logs::system(app, Level::Info, format!("Starting {} {version}", kind.label()));

        match self.spawn(app, kind, settings, layout).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.running.lock().await.remove(&kind);
                if !self.problems.lock().unwrap().contains_key(&kind) {
                    self.set_problem(kind, Problem::new(format!("{} couldn't start", kind.label()), e.to_string(), &[]));
                }
                // Error without a message: the UI records it as a failed start; the caller shows the error.
                emit_status(app, kind, ServiceState::Error, None);
                logs::system(app, Level::Error, format!("{} failed to start: {e}", kind.label()));
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
        for p in required_ports(kind, settings, layout) {
            if !port_free(p) {
                let problem = diagnose::port_problem(kind.label(), p, diagnose::port_owner(p).await);
                let message = problem.detail.clone();
                self.set_problem(kind, problem);
                return Err(Error::msg(message));
            }
        }
        let plan = prepare(app, kind, settings, layout).await?;
        if kind.web_server().is_some() {
            if let Some(output) = server_config_test(&spec).await {
                let line = output
                    .lines()
                    .find(|l| l.contains("rror") || l.contains("emerg"))
                    .unwrap_or(&output)
                    .to_string();
                self.set_problem(
                    kind,
                    Problem::new(
                        format!("{} configuration error", kind.label()),
                        line.clone(),
                        &["If the error points into etc/apache/sites/, fix or remove that file. lampRabit's own config files are regenerated on every start."],
                    ),
                );
                return Err(Error::msg(format!("{} configuration test failed: {line}", kind.label())));
            }
            // PHP has to be listening before the web server takes requests.
            if let Err(e) = self.php.ensure(app, settings, layout, &plan).await {
                self.set_problem(kind, Problem::new("PHP couldn't start", e.to_string(), &[]));
                return Err(e);
            }
        }

        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(layout.logs().join(format!("{}-output.log", kind.id())))
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
        for (k, v) in &spec.env {
            cmd.env(k, v);
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
        logs::system(
            app,
            Level::Info,
            format!("{} started (PID {}, port {port})", kind.label(), child.id().unwrap_or_default()),
        );
        self.running.lock().await.insert(
            kind,
            Handle {
                pid: child.id(),
                state: ServiceState::Running,
                started_at: Some(now_ms()),
                stop_tx: Some(stop_tx),
            },
        );
        emit_status(app, kind, ServiceState::Running, None);

        let running = self.running.clone();
        let problems = self.problems.clone();
        let pools = self.php.clone();
        let logs_to_read = [kind.log_path(layout), layout.logs().join(format!("{}-output.log", kind.id()))];
        let app = app.clone();
        let stop_cmd = spec.stop_cmd;
        tauri::async_runtime::spawn(async move {
            let group = group.ok();
            let (state, message) = tokio::select! {
                status = child.wait() => match status {
                    Ok(s) if s.success() => (ServiceState::Stopped, None),
                    Ok(s) => {
                        let tail: String = logs_to_read.iter().map(|p| diagnose::tail(p, 8 * 1024)).collect();
                        let problem = diagnose::explain_exit(kind.label(), s.code(), &tail);
                        let message = format!("{}: {}", problem.title, problem.detail);
                        problems.lock().unwrap().insert(kind, problem);
                        (ServiceState::Error, Some(message))
                    }
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
            let web_servers_left = {
                let mut running = running.lock().await;
                running.remove(&kind);
                running.keys().any(|k| k.web_server().is_some())
            };
            if kind.web_server().is_some() && !web_servers_left {
                pools.stop_all(&app).await;
            }
            match &message {
                Some(m) => logs::system(&app, Level::Error, m.clone()),
                None => logs::system(&app, Level::Info, format!("{} stopped", kind.label())),
            }
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
                logs::system(app, Level::Info, format!("Stopping {}", kind.label()));
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
            crate::logs::push(&app, kind.id(), "output", line);
        }
    });
}
