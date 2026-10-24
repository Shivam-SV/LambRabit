//! Public tunnels (Ngrok, Cloudflare quick tunnels) for sharing a site or a local port.
//! Requests are forwarded to the site's HTTP port with its own Host header, so virtual
//! hosts keep working behind the public URL.

use std::{collections::HashMap, process::Stdio, sync::Arc};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    sync::{Mutex, oneshot},
};

use crate::{
    config::Settings,
    error::{Error, Result},
    layout::{Layout, exe},
    logs::{self, Level},
    procgroup::{self, ProcGroup},
    sites::Site,
};

const KEYRING_SERVICE: &str = "lampRabit";
const KEYRING_NGROK: &str = "ngrok-authtoken";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Ngrok,
    Cloudflare,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    pub id: String,
    pub provider: Provider,
    /// The site being shared, if any.
    pub site: Option<String>,
    /// What the tunnel forwards to, e.g. "shop.test" or "localhost:3000".
    pub target: String,
    pub url: Option<String>,
    /// "starting", "online" or "error".
    pub status: &'static str,
    pub error: Option<String>,
}

struct Entry {
    info: Tunnel,
    stop: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct Tunnels {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

pub fn ngrok_token() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_NGROK).ok()?.get_password().ok()
}

pub fn set_ngrok_token(token: Option<&str>) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_NGROK).map_err(|e| Error::msg(format!("Keychain unavailable: {e}")))?;
    match token.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => entry.set_password(t).map_err(|e| Error::msg(format!("Couldn't save the token: {e}"))),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::msg(format!("Couldn't remove the token: {e}"))),
        },
    }
}

fn emit(app: &AppHandle) {
    let _ = app.emit("tunnels-changed", ());
}

impl Tunnels {
    pub async fn list(&self) -> Vec<Tunnel> {
        let mut list: Vec<Tunnel> = self.entries.lock().await.values().map(|e| e.info.clone()).collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    pub async fn stop(&self, app: &AppHandle, id: &str) {
        let entry = self.entries.lock().await.remove(id);
        if let Some(mut e) = entry {
            if let Some(tx) = e.stop.take() {
                let _ = tx.send(());
            }
            logs::system(app, Level::Info, format!("Stopped sharing {}", e.info.target));
        }
        emit(app);
    }

    pub async fn stop_all(&self) {
        for (_, mut e) in self.entries.lock().await.drain() {
            if let Some(tx) = e.stop.take() {
                let _ = tx.send(());
            }
        }
    }

    /// Starts sharing a site (by its HTTP port and domain) or a plain local port.
    pub async fn start(
        &self,
        app: &AppHandle,
        settings: &Settings,
        layout: &Layout,
        provider: Provider,
        site: Option<&Site>,
        port: Option<u16>,
    ) -> Result<Tunnel> {
        let (id, target, upstream, host) = match (site, port) {
            (Some(site), _) => {
                let (http, _) = settings.server_ports(site.server);
                (site.name.clone(), site.domain.clone(), format!("http://127.0.0.1:{http}"), Some(site.domain.clone()))
            }
            (None, Some(port)) => (format!("port-{port}"), format!("localhost:{port}"), format!("http://127.0.0.1:{port}"), None),
            (None, None) => return Err(Error::msg("Choose a site or a port to share.")),
        };
        if self.entries.lock().await.contains_key(&id) {
            return Err(Error::msg(format!("{target} is already being shared.")));
        }

        let (runtime, version) = match provider {
            Provider::Ngrok => ("ngrok", settings.versions.ngrok.clone()),
            Provider::Cloudflare => ("cloudflared", settings.versions.cloudflared.clone()),
        };
        let label = if provider == Provider::Ngrok { "Ngrok" } else { "Cloudflare Tunnel (cloudflared)" };
        let version = version.ok_or_else(|| Error::msg(format!("Install {label} from Services → Developer tools first.")))?;
        let program = layout.bin(runtime, &version).join(exe(runtime));

        let mut cmd = tokio::process::Command::new(&program);
        match provider {
            Provider::Ngrok => {
                let token = ngrok_token().ok_or_else(|| {
                    Error::msg("Add your Ngrok authtoken in Settings → Sharing first (free at dashboard.ngrok.com).")
                })?;
                cmd.args(["http", &upstream, "--log", "stdout", "--log-format", "json"]);
                if let Some(h) = &host {
                    cmd.args(["--host-header", h]);
                }
                cmd.env("NGROK_AUTHTOKEN", token);
            }
            Provider::Cloudflare => {
                cmd.args(["tunnel", "--no-autoupdate", "--url", &upstream]);
                if let Some(h) = &host {
                    cmd.args(["--http-host-header", h]);
                }
            }
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        procgroup::prepare(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't start {label}: {e}")))?;
        let group = ProcGroup::attach(&child).ok();

        let info = Tunnel { id: id.clone(), provider, site: site.map(|s| s.name.clone()), target: target.clone(), url: None, status: "starting", error: None };
        let (stop_tx, stop_rx) = oneshot::channel();
        self.entries.lock().await.insert(id.clone(), Entry { info: info.clone(), stop: Some(stop_tx) });
        logs::system(app, Level::Info, format!("Sharing {target} with {label}"));
        emit(app);

        for reader in [child.stdout.take().map(|r| Box::new(r) as Box<dyn AsyncRead + Unpin + Send>), child.stderr.take().map(|r| Box::new(r) as _)]
            .into_iter()
            .flatten()
        {
            watch_output(app.clone(), self.entries.clone(), id.clone(), provider, reader);
        }

        let entries = self.entries.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let stopped = tokio::select! {
                _ = child.wait() => false,
                _ = stop_rx => true,
            };
            if let Some(g) = &group {
                g.kill();
            } else {
                let _ = child.start_kill();
            }
            let _ = child.wait().await;
            if !stopped {
                // Exited on its own: keep the entry with its error so the UI can show why.
                let mut map = entries.lock().await;
                if let Some(e) = map.get_mut(&id) {
                    e.info.status = "error";
                    e.info.url = None;
                    if e.info.error.is_none() {
                        e.info.error = Some("The tunnel closed unexpectedly. Check the Logs page.".into());
                    }
                    logs::system(&app, Level::Warn, format!("Tunnel for {} stopped: {}", e.info.target, e.info.error.clone().unwrap_or_default()));
                }
                drop(map);
                emit(&app);
            }
        });
        Ok(info)
    }
}

/// Reads tunnel output, picking out the public URL and error messages.
fn watch_output(app: AppHandle, entries: Arc<Mutex<HashMap<String, Entry>>>, id: String, provider: Provider, reader: Box<dyn AsyncRead + Unpin + Send>) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            logs::push(&app, "system", "output", format!("[{}] {line}", if provider == Provider::Ngrok { "ngrok" } else { "cloudflared" }));
            let (url, error) = match provider {
                Provider::Ngrok => parse_ngrok(&line),
                Provider::Cloudflare => (parse_cloudflare(&line), None),
            };
            if url.is_none() && error.is_none() {
                continue;
            }
            let mut map = entries.lock().await;
            if let Some(e) = map.get_mut(&id) {
                if let Some(url) = url {
                    e.info.url = Some(url.clone());
                    e.info.status = "online";
                    logs::system(&app, Level::Info, format!("{} is public at {url}", e.info.target));
                }
                if let Some(err) = error {
                    e.info.error = Some(err);
                }
            }
            drop(map);
            emit(&app);
        }
    });
}

/// ngrok logs JSON lines; "started tunnel" carries the URL, "eror"/"crit" levels the reason.
fn parse_ngrok(line: &str) -> (Option<String>, Option<String>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return (None, None) };
    let url = (v["msg"] == "started tunnel").then(|| v["url"].as_str().map(str::to_string)).flatten();
    let error = matches!(v["lvl"].as_str(), Some("eror" | "crit"))
        .then(|| v["err"].as_str().or(v["msg"].as_str()).map(|e| e.lines().next().unwrap_or(e).to_string()))
        .flatten();
    (url, error)
}

/// cloudflared prints the quick-tunnel URL in a banner on stderr.
fn parse_cloudflare(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let url: String = line[start..].chars().take_while(|c| !c.is_whitespace() && *c != '|').collect();
    url.contains(".trycloudflare.com").then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ngrok_url_and_errors() {
        let ok = r#"{"addr":"http://127.0.0.1:80","lvl":"info","msg":"started tunnel","name":"command_line","obj":"tunnels","url":"https://ab12.ngrok-free.app"}"#;
        assert_eq!(parse_ngrok(ok).0.as_deref(), Some("https://ab12.ngrok-free.app"));
        let bad = r#"{"lvl":"eror","msg":"session closing","err":"authentication failed: The authtoken you specified is invalid.\n\nERR_NGROK_107"}"#;
        assert_eq!(parse_ngrok(bad).1.as_deref(), Some("authentication failed: The authtoken you specified is invalid."));
    }

    #[test]
    fn parses_cloudflare_banner() {
        let line = "2026-10-24T14:00:00Z INF |  https://quiet-river-1234.trycloudflare.com                                    |";
        assert_eq!(parse_cloudflare(line).as_deref(), Some("https://quiet-river-1234.trycloudflare.com"));
        assert_eq!(parse_cloudflare("INF Visit https://www.cloudflare.com/website-terms/"), None);
    }
}
