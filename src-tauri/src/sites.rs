//! Sites are the folders in `www/`. Each gets a virtual host at `<name>.<tld>`
//! (plus sub-domains) on Apache or Nginx, optionally over HTTPS, with its own PHP
//! version, and an entry in the system hosts file.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    config::{Server, Settings},
    error::{Error, Result},
    layout::Layout,
    php::{self, PoolPlan},
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
    /// PHP version chosen for this site; None means the active version.
    pub php: Option<String>,
    pub server: Server,
    /// Node version from the project's .nvmrc / .node-version.
    pub node: Option<String>,
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

fn site_url(settings: &Settings, server: Server, domain: &str, ssl: bool) -> String {
    let (http, https) = settings.server_ports(server);
    match (ssl, https, http) {
        (true, 443, _) => format!("https://{domain}/"),
        (true, port, _) => format!("https://{domain}:{port}/"),
        (false, _, 80) => format!("http://{domain}/"),
        (false, _, port) => format!("http://{domain}:{port}/"),
    }
}

/// The server a site runs on: its own choice when both are enabled, otherwise the one that is.
fn site_server(settings: &Settings, name: &str) -> Server {
    match settings.sites.get(name).and_then(|s| s.server) {
        Some(server) if settings.server_enabled(server) => server,
        _ => settings.primary_server(),
    }
}

/// The URL a folder in www/ will have, from the current settings.
pub fn url_for(settings: &Settings, name: &str) -> Option<String> {
    let domain = format!("{}.{}", label(name)?, settings.tld);
    let ssl = settings.sites.get(name).and_then(|s| s.ssl).unwrap_or(settings.https_by_default);
    Some(site_url(settings, site_server(settings, name), &domain, ssl))
}

/// Installed developer tools get their own domains: adminer.test, phpmyadmin.test, mail.test.
/// They are served by the primary server with the active PHP version.
pub fn tools(settings: &Settings, layout: &Layout) -> Vec<Site> {
    let v = &settings.versions;
    let server = settings.primary_server();
    let mut tools = Vec::new();
    let mut add = |id: &'static str, sub: &str, dir: Option<PathBuf>, proxy: Option<String>| {
        let domain = format!("{sub}.{}", settings.tld);
        let ssl = settings.sites.get(id).and_then(|s| s.ssl).unwrap_or(settings.https_by_default);
        let path = dir.unwrap_or_else(|| layout.data(id));
        tools.push(Site {
            name: id.to_string(),
            url: site_url(settings, server, &domain, ssl),
            domain,
            docroot: path.clone(),
            path,
            ssl,
            modified: 0,
            tool: Some(id),
            proxy,
            php: None,
            server,
            node: None,
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

/// Everything the web servers serve: folders in www/ plus installed tools. A folder wins
/// if it happens to use a tool's domain.
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
            let own = settings.sites.get(&name);
            let ssl = own.and_then(|s| s.ssl).unwrap_or(settings.https_by_default);
            let php = own.and_then(|s| s.php.clone());
            let server = site_server(settings, &name);
            let node = crate::node::project_version(&path);
            let modified = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            Some(Site {
                url: site_url(settings, server, &domain, ssl),
                name,
                domain,
                path,
                docroot,
                ssl,
                modified,
                tool: None,
                proxy: None,
                php,
                server,
                node,
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

/// The generated site config for a server: Apache's vhosts file or Nginx's whole config.
pub fn config_path(layout: &Layout, server: Server) -> PathBuf {
    match server {
        Server::Apache => layout.etc("apache").join("vhosts.conf"),
        Server::Nginx => layout.etc("nginx").join("nginx.conf"),
    }
}

pub fn render(settings: &Settings, layout: &Layout, sites: &[Site], plan: &[PoolPlan], server: Server) -> String {
    match server {
        Server::Apache => render_apache(settings, layout, sites, plan),
        Server::Nginx => render_nginx(settings, layout, sites, plan),
    }
}

pub fn apache_php_handler(plan: Option<&PoolPlan>, indent: &str) -> String {
    match plan {
        Some(p) => format!(
            "{indent}<FilesMatch \"\\.php$\">\n{indent}    SetHandler \"proxy:balancer://{}/\"\n{indent}</FilesMatch>\n",
            p.name()
        ),
        None => String::new(),
    }
}

/// Apache virtual hosts for the sites it serves. Pure, so it can be compared with what is
/// on disk to tell whether Apache needs a restart.
fn render_apache(settings: &Settings, layout: &Layout, sites: &[Site], plan: &[PoolPlan]) -> String {
    let (http, https) = settings.server_ports(Server::Apache);
    let sites: Vec<&Site> = sites.iter().filter(|s| s.server == Server::Apache).collect();
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

    if sites.iter().any(|s| s.ssl) {
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
            None => format!(
                "    ServerName {domain}\n    ServerAlias *.{domain}\n    DocumentRoot \"{root}\"\n    <Directory \"{root}\">\n        Options Indexes FollowSymLinks\n        AllowOverride All\n        Require all granted\n    </Directory>\n{handler}",
                domain = site.domain,
                root = slash(&site.docroot),
                handler = apache_php_handler(php::pool_for(plan, site.php.as_deref(), settings), "    "),
            ),
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

/// Nginx's whole config. Unlike Apache it doesn't read .htaccess, so PHP sites get the
/// usual front-controller fallback (`try_files ... /index.php`), which suits Laravel,
/// Symfony and WordPress.
fn render_nginx(settings: &Settings, layout: &Layout, sites: &[Site], plan: &[PoolPlan]) -> String {
    let (http, https) = settings.server_ports(Server::Nginx);
    let home = settings.versions.nginx.as_ref().map(|v| slash(&layout.bin("nginx", v))).unwrap_or_default();
    let logs = slash(&layout.logs());
    let fastcgi_params = format!("{home}/conf/fastcgi_params");
    let sites: Vec<&Site> = sites.iter().filter(|s| s.server == Server::Nginx).collect();

    let upstreams: String = plan
        .iter()
        .map(|p| {
            let servers: String = p.ports().map(|port| format!("        server 127.0.0.1:{port};\n")).collect();
            format!("    upstream {} {{\n{servers}    }}\n", p.name())
        })
        .collect();

    let php_location = |pool: Option<&PoolPlan>, script: &str| match pool {
        Some(p) => format!(
            "        location ~ [^/]\\.php(/|$) {{\n            fastcgi_split_path_info ^(.+?\\.php)(/.*)$;\n            if (!-f $document_root$fastcgi_script_name) {{ return 404; }}\n            include \"{fastcgi_params}\";\n            fastcgi_param SCRIPT_FILENAME {script};\n            fastcgi_param PATH_INFO $fastcgi_path_info;\n            fastcgi_pass {};\n        }}\n",
            p.name()
        ),
        None => String::new(),
    };
    let default_pool = php::pool_for(plan, None, settings);

    // Tools under http://localhost/<tool>/ when Nginx is the primary server.
    let aliases: String = sites
        .iter()
        .filter(|s| s.tool.is_some() && s.proxy.is_none())
        .map(|s| {
            let php = match default_pool {
                Some(p) => format!(
                    "            location ~ \\.php$ {{\n                include \"{fastcgi_params}\";\n                fastcgi_param SCRIPT_FILENAME $request_filename;\n                fastcgi_pass {};\n            }}\n",
                    p.name()
                ),
                None => String::new(),
            };
            format!(
                "        location ^~ /{name}/ {{\n            alias \"{dir}/\";\n            index index.php;\n{php}        }}\n",
                name = s.name,
                dir = slash(&s.docroot)
            )
        })
        .collect();

    let mut servers = format!(
        "    server {{\n        listen {http} default_server;\n        server_name localhost;\n        root \"{www}\";\n        index index.php index.html index.htm;\n        autoindex on;\n{aliases}{php}    }}\n",
        www = slash(&layout.www()),
        php = php_location(default_pool, "$document_root$fastcgi_script_name"),
    );
    for site in sites {
        let mut listen = format!("        listen {http};\n");
        if site.ssl {
            let (crt, key) = ssl::site_cert_paths(layout, &site.domain);
            listen.push_str(&format!(
                "        listen {https} ssl;\n        ssl_certificate \"{}\";\n        ssl_certificate_key \"{}\";\n",
                slash(&crt),
                slash(&key)
            ));
        }
        let body = match &site.proxy {
            Some(target) => format!(
                "        location / {{\n            proxy_pass {target};\n            proxy_http_version 1.1;\n            proxy_set_header Host $host;\n            proxy_set_header Upgrade $http_upgrade;\n            proxy_set_header Connection $connection_upgrade;\n        }}\n"
            ),
            None => format!(
                "        root \"{root}\";\n        index index.php index.html index.htm;\n        location / {{\n            try_files $uri $uri/ /index.php?$query_string;\n        }}\n        location ~ /\\.ht {{ deny all; }}\n{php}",
                root = slash(&site.docroot),
                php = php_location(php::pool_for(plan, site.php.as_deref(), settings), "$document_root$fastcgi_script_name"),
            ),
        };
        servers.push_str(&format!(
            "\n    # {name}\n    server {{\n{listen}        server_name {domain} *.{domain};\n{body}    }}\n",
            name = site.name,
            domain = site.domain,
        ));
    }

    format!(
        r#"# Generated by lampRabit. Changes are overwritten on every start.
worker_processes 1;
error_log "{logs}/nginx-error.log" warn;
pid "{pid}";

events {{
    worker_connections 1024;
}}

http {{
    include "{home}/conf/mime.types";
    default_type application/octet-stream;
    # lampRabit's own health checks would otherwise add a line every few seconds.
    map $http_user_agent $lamprabit_log {{
        ~^lampRabit/ 0;
        default 1;
    }}
    map $http_upgrade $connection_upgrade {{
        default upgrade;
        '' close;
    }}
    access_log "{logs}/nginx-access.log" combined if=$lamprabit_log;
    sendfile off;
    client_max_body_size 128m;
    server_names_hash_bucket_size 128;
    fastcgi_read_timeout 300;

{upstreams}
{servers}}}
"#,
        pid = slash(&layout.tmp().join("nginx").join("nginx.pid")),
    )
}

/// Issues any missing certificates and writes the server's site config.
/// Returns true when a new local CA had to be created.
pub fn write_config(settings: &Settings, layout: &Layout, sites: &[Site], plan: &[PoolPlan], server: Server) -> Result<bool> {
    let mut created_ca = false;
    let ssl_sites: Vec<&Site> = sites.iter().filter(|s| s.ssl && s.server == server).collect();
    if !ssl_sites.is_empty() {
        let (ca, created) = ssl::ensure_ca(layout)?;
        created_ca = created;
        for site in ssl_sites {
            ssl::ensure_site_cert(layout, &ca, &site.domain)?;
        }
    }
    let path = config_path(layout, server);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, render(settings, layout, sites, plan, server))?;
    Ok(created_ca)
}

/// Whether a running server was configured with a different site list.
pub fn config_stale(settings: &Settings, layout: &Layout, sites: &[Site], plan: &[PoolPlan], server: Server) -> bool {
    std::fs::read_to_string(config_path(layout, server)).ok().as_deref()
        != Some(&render(settings, layout, sites, plan, server))
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
    std::fs::read(hosts_file()).is_ok_and(|h| managed_domains(&String::from_utf8_lossy(&h)) == wanted)
}

/// The new hosts file: every byte outside lampRabit's block is kept as it was (the file
/// may not be UTF-8, and other tools' entries must survive untouched).
fn render_hosts(current: &[u8], sites: &[Site]) -> Vec<u8> {
    let nl: &[u8] = if cfg!(windows) { b"\r\n" } else { b"\n" };
    let mut lines: Vec<&[u8]> = Vec::new();
    let mut inside = false;
    for raw in current.split(|&b| b == b'\n') {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        let trimmed = String::from_utf8_lossy(line);
        match trimmed.trim() {
            HOSTS_BEGIN => inside = true,
            HOSTS_END => inside = false,
            _ if !inside => lines.push(line),
            _ => {}
        }
    }
    while lines.last().is_some_and(|l| l.iter().all(u8::is_ascii_whitespace)) {
        lines.pop();
    }
    let mut out = Vec::with_capacity(current.len() + 64 * sites.len());
    for line in lines {
        out.extend_from_slice(line);
        out.extend_from_slice(nl);
    }
    if !sites.is_empty() {
        out.extend_from_slice(nl);
        out.extend_from_slice(HOSTS_BEGIN.as_bytes());
        out.extend_from_slice(nl);
        for s in sites {
            out.extend_from_slice(format!("127.0.0.1\t{}", s.domain).as_bytes());
            out.extend_from_slice(nl);
        }
        out.extend_from_slice(HOSTS_END.as_bytes());
        out.extend_from_slice(nl);
    }
    out
}

/// Rewrites lampRabit's block of the hosts file. This needs administrator rights, so the
/// OS shows its elevation prompt; only the managed block changes, and a copy of the
/// previous file is kept in tmp/.
pub async fn update_hosts(layout: &Layout, sites: &[Site]) -> Result<()> {
    let hosts = hosts_file();
    // Never write a hosts file built from a failed read: that would drop every other entry.
    let current = std::fs::read(&hosts).map_err(|e| Error::msg(format!("Could not read {}: {e}", hosts.display())))?;
    std::fs::create_dir_all(layout.tmp())?;
    std::fs::write(layout.tmp().join("hosts.backup"), &current)?;
    let staged = layout.tmp().join("hosts.lamprabit");
    let error_file = layout.tmp().join("hosts.lamprabit.error");
    let _ = std::fs::remove_file(&error_file);
    std::fs::write(&staged, render_hosts(&current, sites))?;

    let status = if cfg!(windows) {
        use base64::Engine;
        let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
        // Many hosts files are marked read-only, which even administrators can't write
        // through; lift the flag for the write and put it back afterwards.
        let script = format!(
            "$ErrorActionPreference = 'Stop'; try {{ \
               $h = '{hosts}'; $f = Get-Item -LiteralPath $h -Force; $ro = $f.IsReadOnly; \
               if ($ro) {{ $f.IsReadOnly = $false }}; \
               [IO.File]::WriteAllBytes($h, [IO.File]::ReadAllBytes('{staged}')); \
               if ($ro) {{ (Get-Item -LiteralPath $h -Force).IsReadOnly = $true }}; \
               Clear-DnsClientCache -ErrorAction SilentlyContinue; exit 0 \
             }} catch {{ Set-Content -LiteralPath '{error}' -Value $_.Exception.Message; exit 2 }}",
            hosts = quote(&hosts),
            staged = quote(&staged),
            error = quote(&error_file),
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
    let reason = std::fs::read_to_string(&error_file).ok().map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
    let _ = std::fs::remove_file(&error_file);

    match status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(Error::msg("Updating the hosts file was cancelled.")),
        _ => Err(Error::msg(match reason {
            // Windows Defender and other antivirus tools sometimes lock the hosts file.
            Some(r) => format!(
                "Could not update {}: {r} If security software protects the hosts file, allow lampRabit or add the entries by hand.",
                hosts.display()
            ),
            None => format!("Could not update {}.", hosts.display()),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Server;

    fn site(domain: &str) -> Site {
        Site {
            name: domain.into(),
            domain: domain.into(),
            path: PathBuf::new(),
            docroot: PathBuf::new(),
            ssl: false,
            url: String::new(),
            modified: 0,
            tool: None,
            proxy: None,
            php: None,
            server: Server::Apache,
            node: None,
        }
    }

    #[test]
    fn hosts_keeps_foreign_bytes_and_replaces_block() {
        // Latin-1 "é" (0xE9) is not valid UTF-8 and must survive untouched.
        let mut current = b"127.0.0.1 localhost\r\n# caf\xe9\r\n0.0.0.0 blocked.example\r\n\r\n".to_vec();
        current.extend_from_slice(format!("{HOSTS_BEGIN}\r\n127.0.0.1\told.test\r\n{HOSTS_END}\r\n").as_bytes());
        let out = render_hosts(&current, &[site("new.test")]);
        let text = String::from_utf8_lossy(&out);
        // Lines come back with the platform's line ending.
        let expected: &[u8] = if cfg!(windows) { b"caf\xe9\r\n" } else { b"caf\xe9\n" };
        assert!(out.windows(expected.len()).any(|w| w == expected));
        assert!(text.contains("0.0.0.0 blocked.example"));
        assert!(text.contains("127.0.0.1\tnew.test"));
        assert!(!text.contains("old.test"));
        assert_eq!(text.matches(HOSTS_BEGIN).count(), 1);
    }

    #[test]
    fn hosts_block_removed_when_no_sites() {
        let current = format!("127.0.0.1 localhost\n{HOSTS_BEGIN}\n127.0.0.1\tx.test\n{HOSTS_END}\n");
        let out = String::from_utf8(render_hosts(current.as_bytes(), &[])).unwrap();
        assert!(!out.contains(HOSTS_BEGIN));
        assert!(out.contains("127.0.0.1 localhost"));
    }
}
