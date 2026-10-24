//! Sites are the folders in `www/`. Each gets a virtual host at `<name>.<tld>`
//! (plus sub-domains), optionally over HTTPS, and an entry in the system hosts file.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    config::Settings,
    error::{Error, Result},
    layout::Layout,
    ssl,
};

const HOSTS_BEGIN: &str = "# lampRabit begin (managed automatically, do not edit)";
const HOSTS_END: &str = "# lampRabit end";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub name: String,
    pub domain: String,
    pub path: PathBuf,
    /// Served directory: `public/` when the project has one (Laravel, Symfony, ...).
    pub docroot: PathBuf,
    pub ssl: bool,
    pub url: String,
    /// Unix time in milliseconds of the folder's last modification.
    pub modified: u64,
    /// Set for built-in tools (adminer, phpmyadmin, mailpit) rather than www/ folders.
    pub tool: Option<&'static str>,
    /// Forward requests to this local URL instead of serving files.
    pub proxy: Option<String>,
}

/// Folder name → DNS label: lowercase letters, digits and dashes.
fn label(name: &str) -> Option<String> {
    let label: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    (!label.is_empty() && label.len() <= 63).then_some(label)
}

/// Browsers resolve `*.localhost` to this machine on their own.
pub fn needs_hosts_file(settings: &Settings) -> bool {
    settings.tld != "localhost"
}

fn site_url(settings: &Settings, domain: &str, ssl: bool) -> String {
    match (ssl, settings.ports.https, settings.ports.http) {
        (true, 443, _) => format!("https://{domain}/"),
        (true, port, _) => format!("https://{domain}:{port}/"),
        (false, _, 80) => format!("http://{domain}/"),
        (false, _, port) => format!("http://{domain}:{port}/"),
    }
}

/// Installed developer tools get their own domains: adminer.test, phpmyadmin.test, mail.test.
pub fn tools(settings: &Settings, layout: &Layout) -> Vec<Site> {
    let v = &settings.versions;
    let mut tools = Vec::new();
    let mut add = |id: &'static str, sub: &str, dir: Option<PathBuf>, proxy: Option<String>| {
        let domain = format!("{sub}.{}", settings.tld);
        let ssl = settings.sites.get(id).and_then(|s| s.ssl).unwrap_or(settings.https_by_default);
        let path = dir.unwrap_or_else(|| layout.data(id));
        tools.push(Site {
            name: id.to_string(),
            url: site_url(settings, &domain, ssl),
            domain,
            docroot: path.clone(),
            path,
            ssl,
            modified: 0,
            tool: Some(id),
            proxy,
        });
    };
    if let Some(ver) = &v.adminer {
        add("adminer", "adminer", Some(layout.bin("adminer", ver)), None);
    }
    if let Some(ver) = &v.phpmyadmin {
        add("phpmyadmin", "phpmyadmin", Some(layout.bin("phpmyadmin", ver)), None);
    }
    if v.mailpit.is_some() {
        add("mailpit", "mail", None, Some(format!("http://127.0.0.1:{}/", settings.ports.mail_ui)));
    }
    tools
}

/// Everything Apache serves: folders in www/ plus installed tools. A folder wins if it
/// happens to use a tool's domain.
pub fn served(settings: &Settings, layout: &Layout) -> Vec<Site> {
    let mut all = scan(settings, layout);
    let taken: BTreeSet<String> = all.iter().map(|s| s.domain.clone()).collect();
    all.extend(tools(settings, layout).into_iter().filter(|t| !taken.contains(&t.domain)));
    all
}

pub fn scan(settings: &Settings, layout: &Layout) -> Vec<Site> {
    let Ok(entries) = std::fs::read_dir(layout.www()) else { return Vec::new() };
    let mut sites: Vec<Site> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.starts_with('.') {
                return None;
            }
            let domain = format!("{}.{}", label(&name)?, settings.tld);
            let path = e.path();
            let public = path.join("public");
            let docroot = if public.is_dir() { public } else { path.clone() };
            let ssl = settings.sites.get(&name).and_then(|s| s.ssl).unwrap_or(settings.https_by_default);
            let modified = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            Some(Site {
                url: site_url(settings, &domain, ssl),
                name,
                domain,
                path,
                docroot,
                ssl,
                modified,
                tool: None,
                proxy: None,
            })
        })
        .collect();
    sites.sort_by(|a, b| b.modified.cmp(&a.modified));
    // Two folders can map to the same domain (e.g. "My App" and "my-app"); keep the first.
    let mut seen = BTreeSet::new();
    sites.retain(|s| seen.insert(s.domain.clone()));
    sites
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn vhosts_path(layout: &Layout) -> PathBuf {
    layout.etc("apache").join("vhosts.conf")
}

/// The Apache virtual-host config for the given sites. Pure, so it can be compared with
/// what is on disk to tell whether Apache needs a restart.
pub fn render_vhosts(settings: &Settings, layout: &Layout, sites: &[Site]) -> String {
    let (http, https) = (settings.ports.http, settings.ports.https);
    // Tools are also reachable as http://localhost/<tool>/.
    let aliases: String = sites
        .iter()
        .filter(|s| s.tool.is_some() && s.proxy.is_none())
        .map(|s| {
            let dir = slash(&s.docroot);
            // <Directory> grants are per virtual host, so the alias needs its own.
            format!("    Alias /{} \"{dir}\"\n    <Directory \"{dir}\">\n        Require all granted\n    </Directory>\n", s.name)
        })
        .collect();
    let mut out = format!(
        "# Generated by lampRabit from the folders in www/. Regenerated on every Apache start.\n\n\
         <VirtualHost *:{http}>\n    ServerName localhost\n    DocumentRoot \"{}\"\n{aliases}</VirtualHost>\n",
        slash(&layout.www())
    );

    let any_ssl = sites.iter().any(|s| s.ssl);
    if any_ssl {
        out.push_str(&format!(
            "\nLoadModule ssl_module modules/mod_ssl.so\nListen {https}\nSSLSessionCache \"shmcb:{}/ssl_scache(512000)\"\nSSLProtocol all -SSLv3 -TLSv1 -TLSv1.1\n",
            slash(&layout.tmp())
        ));
    }

    for site in sites {
        let body = match &site.proxy {
            // `upgrade` lets the proxied app use WebSockets (Mailpit's live inbox).
            Some(target) => format!(
                "    ServerName {domain}\n    ProxyPreserveHost On\n    ProxyPass / {target} upgrade=websocket\n    ProxyPassReverse / {target}\n",
                domain = site.domain,
            ),
            None => {
                // phpMyAdmin lags behind new PHP releases; keep deprecation notices off its pages.
                let quiet = if site.tool == Some("phpmyadmin") && cfg!(windows) {
                    "        php_admin_flag display_errors off\n"
                } else {
                    ""
                };
                format!(
                    "    ServerName {domain}\n    ServerAlias *.{domain}\n    DocumentRoot \"{root}\"\n    <Directory \"{root}\">\n        Options Indexes FollowSymLinks\n        AllowOverride All\n        Require all granted\n{quiet}    </Directory>\n",
                    domain = site.domain,
                    root = slash(&site.docroot),
                )
            }
        };
        out.push_str(&format!("\n# {}\n<VirtualHost *:{http}>\n{body}</VirtualHost>\n", site.name));
        if site.ssl {
            let (crt, key) = ssl::site_cert_paths(layout, &site.domain);
            out.push_str(&format!(
                "<VirtualHost *:{https}>\n{body}    SSLEngine on\n    SSLCertificateFile \"{}\"\n    SSLCertificateKeyFile \"{}\"\n</VirtualHost>\n",
                slash(&crt),
                slash(&key),
            ));
        }
    }
    out
}

/// Issues any missing certificates and writes the vhost config.
/// Returns true when a new local CA had to be created.
pub fn write_vhosts(settings: &Settings, layout: &Layout, sites: &[Site]) -> Result<bool> {
    let mut created_ca = false;
    if sites.iter().any(|s| s.ssl) {
        let (ca, created) = ssl::ensure_ca(layout)?;
        created_ca = created;
        for site in sites.iter().filter(|s| s.ssl) {
            ssl::ensure_site_cert(layout, &ca, &site.domain)?;
        }
    }
    std::fs::write(vhosts_path(layout), render_vhosts(settings, layout, sites))?;
    Ok(created_ca)
}

/// Whether the running Apache was configured with a different site list.
pub fn vhosts_stale(settings: &Settings, layout: &Layout, sites: &[Site]) -> bool {
    std::fs::read_to_string(vhosts_path(layout)).ok().as_deref() != Some(&render_vhosts(settings, layout, sites))
}

pub fn hosts_file() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join(r"System32\drivers\etc\hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

/// Domains currently in lampRabit's block of the hosts file.
fn managed_domains(hosts: &str) -> BTreeSet<String> {
    let mut inside = false;
    let mut domains = BTreeSet::new();
    for line in hosts.lines() {
        let line = line.trim();
        if line == HOSTS_BEGIN {
            inside = true;
        } else if line == HOSTS_END {
            inside = false;
        } else if inside && !line.starts_with('#') {
            domains.extend(line.split_whitespace().skip(1).map(str::to_string));
        }
    }
    domains
}

pub fn hosts_in_sync(sites: &[Site]) -> bool {
    let wanted: BTreeSet<String> = sites.iter().map(|s| s.domain.clone()).collect();
    std::fs::read_to_string(hosts_file()).is_ok_and(|h| managed_domains(&h) == wanted)
}

fn render_hosts(current: &str, sites: &[Site]) -> String {
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut lines = Vec::new();
    let mut inside = false;
    for line in current.lines() {
        match line.trim() {
            HOSTS_BEGIN => inside = true,
            HOSTS_END => inside = false,
            _ if !inside => lines.push(line.to_string()),
            _ => {}
        }
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if !sites.is_empty() {
        lines.push(String::new());
        lines.push(HOSTS_BEGIN.to_string());
        lines.extend(sites.iter().map(|s| format!("127.0.0.1\t{}", s.domain)));
        lines.push(HOSTS_END.to_string());
    }
    lines.join(nl) + nl
}

/// Rewrites lampRabit's block of the hosts file. This needs administrator rights, so the
/// OS shows its elevation prompt; only the managed block changes.
pub async fn update_hosts(layout: &Layout, sites: &[Site]) -> Result<()> {
    let hosts = hosts_file();
    let current = std::fs::read_to_string(&hosts).unwrap_or_default();
    let staged = layout.tmp().join("hosts.lamprabit");
    std::fs::write(&staged, render_hosts(&current, sites))?;

    let status = if cfg!(windows) {
        use base64::Engine;
        let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let script = format!(
            "try {{ [IO.File]::WriteAllText('{}', [IO.File]::ReadAllText('{}')); Clear-DnsClientCache -ErrorAction SilentlyContinue; exit 0 }} catch {{ exit 2 }}",
            quote(&hosts),
            quote(&staged)
        );
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
        let launcher = format!(
            "try {{ $p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile','-EncodedCommand','{encoded}'; exit $p.ExitCode }} catch {{ exit 1 }}"
        );
        let mut cmd = tokio::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &launcher]);
        crate::procgroup::prepare(&mut cmd);
        cmd.status().await?
    } else if cfg!(target_os = "macos") {
        let script = format!(
            "do shell script \"cp '{}' /etc/hosts && dscacheutil -flushcache\" with administrator privileges",
            staged.display()
        );
        tokio::process::Command::new("osascript").args(["-e", &script]).status().await?
    } else {
        tokio::process::Command::new("pkexec").arg("cp").arg(&staged).arg(&hosts).status().await?
    };
    let _ = std::fs::remove_file(&staged);

    match status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(Error::msg("Updating the hosts file was cancelled.")),
        _ => Err(Error::msg(format!("Could not update {}.", hosts.display()))),
    }
}
