use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Root of the managed environment: binaries, configs, www, data, logs.
    pub root_dir: PathBuf,
    /// Top-level domain used for auto virtual hosts, e.g. `myapp.test`.
    pub tld: String,
    pub ports: Ports,
    pub versions: Versions,
    /// Which web servers run: Apache, Nginx, or both side by side.
    pub web_server: WebServer,
    /// php-cgi processes per PHP version on Windows (php-fpm manages its own pool elsewhere).
    pub php_workers: u8,
    /// HTTPS for sites that have no setting of their own.
    pub https_by_default: bool,
    /// Keep the system hosts file in sync with the site list when Apache starts.
    pub manage_hosts: bool,
    /// Point PHP's mail() at Mailpit while Mailpit is installed.
    pub route_mail: bool,
    /// Look for a new lampRabit release when the app starts.
    pub check_updates: bool,
    /// Closing the window keeps lampRabit (and its services) running in the tray.
    pub close_to_tray: bool,
    /// Put the shims folder (php, composer, mariadb…) on the user's PATH.
    pub add_to_path: bool,
    /// Start every installed service when lampRabit opens.
    pub start_services_on_launch: bool,
    /// Command used for "Open in editor" (e.g. "code", "cursor", "phpstorm").
    pub editor: String,
    /// Per-site overrides keyed by folder name.
    pub sites: BTreeMap<String, SiteSettings>,
    /// Extension choices per installed PHP version, on top of the defaults.
    pub php_extensions: BTreeMap<String, ExtensionChoices>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtensionChoices {
    pub enabled: std::collections::BTreeSet<String>,
    pub disabled: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SiteSettings {
    pub ssl: Option<bool>,
    /// PHP version for this site; the active version when unset.
    pub php: Option<String>,
    /// Web server for this site when both are enabled.
    pub server: Option<Server>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebServer {
    #[default]
    Apache,
    Nginx,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Server {
    Apache,
    Nginx,
}

impl Settings {
    pub fn server_enabled(&self, server: Server) -> bool {
        matches!(
            (self.web_server, server),
            (WebServer::Both, _) | (WebServer::Apache, Server::Apache) | (WebServer::Nginx, Server::Nginx)
        )
    }

    /// (http, https) for a web server. With both enabled, Apache keeps the main ports and
    /// Nginx uses its own; alone, either server uses the main ports.
    pub fn server_ports(&self, server: Server) -> (u16, u16) {
        match (server, self.web_server) {
            (Server::Nginx, WebServer::Both) => (self.ports.nginx_http, self.ports.nginx_https),
            _ => (self.ports.http, self.ports.https),
        }
    }

    /// The server that also hosts localhost and the developer tools.
    pub fn primary_server(&self) -> Server {
        if self.web_server == WebServer::Nginx { Server::Nginx } else { Server::Apache }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Ports {
    pub http: u16,
    pub https: u16,
    pub mysql: u16,
    /// First port for PHP FastCGI workers; each PHP version in use takes a block of 10.
    pub php_fpm: u16,
    /// Nginx's ports when it runs next to Apache.
    pub nginx_http: u16,
    pub nginx_https: u16,
    /// Mailpit's SMTP server, where PHP and frameworks send mail.
    pub mail_smtp: u16,
    pub redis: u16,
    pub memcached: u16,
    /// Mailpit's web inbox.
    pub mail_ui: u16,
}

/// Active version per runtime; `None` means not installed yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Versions {
    pub apache: Option<String>,
    pub nginx: Option<String>,
    pub php: Option<String>,
    pub mariadb: Option<String>,
    pub mailpit: Option<String>,
    pub adminer: Option<String>,
    pub phpmyadmin: Option<String>,
    pub ngrok: Option<String>,
    pub cloudflared: Option<String>,
    pub redis: Option<String>,
    pub dotnet: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root_dir: default_root(),
            tld: "test".into(),
            ports: Ports::default(),
            versions: Versions::default(),
            web_server: WebServer::Apache,
            php_workers: 4,
            https_by_default: true,
            manage_hosts: true,
            route_mail: true,
            check_updates: true,
            close_to_tray: true,
            add_to_path: false,
            start_services_on_launch: false,
            editor: "code".into(),
            sites: BTreeMap::new(),
            php_extensions: BTreeMap::new(),
        }
    }
}

impl Default for Ports {
    fn default() -> Self {
        Self {
            http: 80,
            https: 443,
            mysql: 3306,
            php_fpm: 9000,
            nginx_http: 8080,
            nginx_https: 8443,
            mail_smtp: 1025,
            redis: 6379,
            memcached: 11211,
            mail_ui: 8025,
        }
    }
}

fn default_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\lampRabit")
    } else {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("lampRabit")
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
