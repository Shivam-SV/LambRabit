//! Sites are the folders in `www/`. Each gets a virtual host at `<name>.<tld>`
//! (plus sub-domains) on Apache or Nginx, optionally over HTTPS, with its own PHP
//! version, and an entry in the system hosts file.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    config::{ServeMode, Server, Settings},
    error::{Error, Result},
    layout::Layout,
    php::{self, PoolPlan},
    ssl,
};

/// What a project is written in, which decides the runtime it gets (PHP version, Node
/// version) and how it's served. New languages (Go, Python...) add a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Php,
    Node,
    /// HTML and assets only.
    Static,
}

const HOSTS_BEGIN: &str = "# lampRabit begin (managed automatically, do not edit)";
const HOSTS_END: &str = "# lampRabit end";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub name: String,
    pub domain: String,
    pub path: PathBuf,
    /// Served directory: `public/` when the project has one (Laravel, Symfony...), `web/`
    /// (Drupal), `webroot/` (CakePHP) or `pub/` (Magento) when they hold an index.php.
    pub docroot: PathBuf,
    pub ssl: bool,
    pub url: String,
    /// Unix time in milliseconds of the folder's last modification.
    pub modified: u64,
    /// Set for built-in tools (adminer, phpmyadmin, phpredisadmin, mailpit) rather than www/ folders.
    pub tool: Option<&'static str>,
    /// Forward requests to this local URL instead of serving files.
    pub proxy: Option<String>,
    /// Pass the browser's Host header on to `proxy` (tools); Node dev servers get their
    /// own address instead, since Vite refuses hosts it doesn't know.
    pub proxy_keep_host: bool,
    /// How the project's process runs, once set up (or started once).
    pub app: Option<crate::config::NodeApp>,
    /// Has a package.json, so it has scripts and can run a dev server.
    pub node_project: bool,
    pub language: Language,
    /// Dynamic: the project runs a dev server (Node projects that have one), so LampRabit
    /// offers to start it. Static: the web server serves the files (PHP, HTML, a build folder).
    pub serve_mode: ServeMode,
    /// Where the domain goes right now: a port the project listens on (ports.rs), or None
    /// for its files.
    pub route: Option<crate::ports::Route>,
    /// Every port the project's processes listen on, whoever started them.
    pub ports: Vec<crate::ports::Detected>,
    /// The port picked for the domain, when there are several.
    pub route_port: Option<u16>,
    /// PHP version chosen for this site; None means the active version.
    pub php: Option<String>,
    pub server: Server,
    /// Node version from the project's .nvmrc / .node-version.
    pub node: Option<String>,
    /// What the project is built with ("Laravel", "WordPress"…), from its files.
    pub framework: Option<&'static str>,
    /// The checked-out git branch (or short commit when detached).
    pub git_branch: Option<String>,
    /// The database the project's config names (Laravel/Symfony .env, wp-config.php).
    pub database: Option<String>,
}

/// Framework, git branch and database name of a project, read from a few small files.
fn project_info(dir: &Path) -> (Option<&'static str>, Option<String>, Option<String>) {
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
    let composer = read("composer.json").unwrap_or_default();
    let requires = |pkg: &str| composer.contains(&format!("\"{pkg}\""));
    // Statamic is a Laravel app, so it's checked first.
    let framework = if requires("statamic/cms") {
        Some("Statamic")
    } else if dir.join("artisan").exists() || requires("laravel/framework") {
        Some("Laravel")
    } else if dir.join("wp-config.php").exists() || dir.join("wp-login.php").exists() {
        Some("WordPress")
    } else if requires("symfony/framework-bundle") || dir.join("symfony.lock").exists() {
        Some("Symfony")
    } else if dir.join("bin/magento").exists() || requires("magento/product-community-edition") || requires("mage-os/product-community-edition") {
        Some("Magento")
    } else if requires("drupal/core") || requires("drupal/core-recommended") {
        Some("Drupal")
    } else if requires("codeigniter4/framework") {
        Some("CodeIgniter")
    } else if requires("cakephp/cakephp") {
        Some("CakePHP")
    } else if requires("yiisoft/yii2") {
        Some("Yii")
    } else if requires("slim/slim") {
        Some("Slim")
    } else if !composer.is_empty() || dir.join("index.php").exists() || dir.join("public/index.php").exists() {
        Some("PHP")
    } else if dir.join("index.html").exists() || dir.join("public/index.html").exists() {
        Some("Static")
    } else {
        None
    };

    let git_branch = read(".git/HEAD").and_then(|head| {
        let head = head.trim();
        match head.strip_prefix("ref: refs/heads/") {
            Some(branch) => Some(branch.to_string()),
            None => head.get(..7).map(str::to_string),
        }
    });

    // `KEY=value` from a .env file, without quotes.
    let env_value = |text: &str, key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(&format!("{key}=")))
            .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
            .filter(|v| !v.is_empty())
    };
    let database = match framework {
        Some("Laravel") => read(".env").and_then(|t| env_value(&t, "DB_DATABASE")),
        Some("WordPress") => read("wp-config.php").and_then(|t| {
            let at = t.find("'DB_NAME'").or_else(|| t.find("\"DB_NAME\""))?;
            let rest = &t[at + 9..];
            let start = rest.find(['\'', '"'])? + 1;
            let end = rest[start..].find(['\'', '"'])?;
            Some(rest[start..start + end].to_string()).filter(|d| !d.is_empty() && d != "database_name_here")
        }),
        _ => [".env.local", ".env"]
            .iter()
            .filter_map(|f| read(f))
            .find_map(|t| env_value(&t, "DATABASE_URL"))
            .and_then(|url| url.split('?').next().and_then(|u| u.rsplit('/').next()).map(str::to_string))
            .filter(|d| !d.is_empty() && !d.contains(':')),
    };
    (framework, git_branch, database)
}

/// A project's language from its files: PHP when it's a PHP project (even with a
/// package.json for its assets), Node with a package.json, otherwise static files.
pub fn language(dir: &Path) -> Language {
    language_of(project_info(dir).0, dir)
}

fn language_of(framework: Option<&str>, dir: &Path) -> Language {
    match framework {
        Some(f) if f != "Static" => Language::Php,
        _ if dir.join("package.json").exists() => Language::Node,
        _ => Language::Static,
    }
}

/// Folder name → DNS label: lowercase letters, digits and dashes. Common accented letters
/// keep their base letter ("Café" → "cafe") instead of turning into dashes.
fn label(name: &str) -> Option<String> {
    let label: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c.to_string(),
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a".into(),
            'æ' => "ae".into(),
            'ç' | 'č' | 'ć' => "c".into(),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => "e".into(),
            'ì' | 'í' | 'î' | 'ï' | 'ī' => "i".into(),
            'ñ' | 'ń' => "n".into(),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o".into(),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u".into(),
            'ý' | 'ÿ' => "y".into(),
            'ß' => "ss".into(),
            'š' | 'ś' => "s".into(),
            'ž' | 'ź' | 'ż' => "z".into(),
            'ł' => "l".into(),
            _ => "-".into(),
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    (!label.is_empty() && label.len() <= 63).then_some(label)
}

/// A folder in www/ that isn't served, and why, so the Sites page can say so.
#[derive(Debug, Clone, Serialize)]
pub struct Unserved {
    pub name: String,
    pub reason: String,
}

/// Folders that scan() leaves out: names with no usable letters, names that are too long,
/// and folders whose domain another folder already uses.
pub fn unserved(settings: &Settings, layout: &Layout) -> Vec<Unserved> {
    let Ok(entries) = std::fs::read_dir(layout.www()) else { return Vec::new() };
    let served: Vec<Site> = scan(settings, layout);
    let mut out = Vec::new();
    for entry in entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if name.starts_with('.') || served.iter().any(|s| s.name == name) {
            continue;
        }
        let reason = match label(&name) {
            None if name.chars().filter(|c| c.is_ascii_alphanumeric()).count() > 63 => {
                "The name is too long for a domain (63 characters at most). Rename the folder.".to_string()
            }
            None => "The name has no letters or digits a domain can use. Rename the folder, for example to plain English letters.".to_string(),
            Some(l) => {
                let domain = format!("{l}.{}", settings.tld);
                match served.iter().find(|s| s.domain == domain) {
                    Some(other) => format!("“{}” already uses {domain}. Rename one of the two folders.", other.name),
                    None => continue,
                }
            }
        };
        out.push(Unserved { name, reason });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Folders that took a developer tool's domain (e.g. a folder called "adminer"), hiding the tool.
pub fn shadowed_tools(settings: &Settings, layout: &Layout) -> Vec<String> {
    let folders = scan(settings, layout);
    tools(settings, layout)
        .into_iter()
        .filter(|t| folders.iter().any(|f| f.domain == t.domain))
        .map(|t| t.domain)
        .collect()
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
pub fn site_server(settings: &Settings, name: &str) -> Server {
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
            proxy_keep_host: true,
            app: None,
            node_project: false,
            language: Language::Static,
            serve_mode: ServeMode::Static,
            route: None,
            ports: Vec::new(),
            route_port: None,
            php: None,
            server,
            node: None,
            framework: None,
            git_branch: None,
            database: None,
        });
    };
    if let Some(ver) = &v.adminer {
        add("adminer", "adminer", Some(layout.bin("adminer", ver)), None);
    }
    if let Some(ver) = &v.phpmyadmin {
        add("phpmyadmin", "phpmyadmin", Some(layout.bin("phpmyadmin", ver)), None);
    }
    if let Some(ver) = &v.phpredisadmin {
        add("phpredisadmin", "phpredisadmin", Some(layout.bin("phpredisadmin", ver)), None);
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
            let mut docroot = docroot(&path);
            let own = settings.sites.get(&name);
            let ssl = own.and_then(|s| s.ssl).unwrap_or(settings.https_by_default);
            let php = own.and_then(|s| s.php.clone());
            let server = site_server(settings, &name);
            let node = crate::node::project_version(&path);
            let (mut framework, git_branch, database) = project_info(&path);
            let node_project = path.join("package.json").exists();
            let language = language_of(framework, &path);
            if language == Language::Node {
                framework = crate::apps::framework(&path);
            }
            let app = own.and_then(|s| s.app.clone());
            // Dynamic is preferred: a Node project with a dev server (from its template or
            // package.json) is served through it, unless it was switched to its files.
            let serve_mode = if language == Language::Node && app.as_ref().map_or(crate::apps::runnable(&path), |a| a.serve) {
                ServeMode::Dynamic
            } else {
                ServeMode::Static
            };
            // The domain follows what's listening: the dev server while it runs (whoever
            // started it), the files otherwise.
            let route = crate::ports::route_of(&name);
            let ports = crate::ports::detected_of(&name);
            let proxy = route.as_ref().map(|r| format!("http://{}:{}/", r.host, r.port));
            // Served as files, a JavaScript project shows its build output, else its own
            // index.html; public/ there holds assets, not a site.
            if language == Language::Node && route.is_none() {
                docroot = ["dist", "build", "out", "public"].iter().map(|d| path.join(d)).find(|d| d.join("index.html").is_file()).unwrap_or_else(|| path.clone());
            }
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
                proxy,
                // PHP servers (artisan serve, php -S) accept any host and build links from it;
                // Node dev servers get their own address, since Vite refuses hosts it doesn't know.
                proxy_keep_host: language == Language::Php,
                app,
                node_project,
                language,
                serve_mode,
                ports,
                route_port: own.and_then(|s| s.route_port),
                route,
                php,
                server,
                node,
                framework,
                git_branch,
                database,
            })
        })
        .collect();
    sites.sort_by(|a, b| b.modified.cmp(&a.modified));
    // Two folders can map to the same domain (e.g. "My App" and "my-app"); keep the first.
    let mut seen = BTreeSet::new();
    sites.retain(|s| seen.insert(s.domain.clone()));
    sites
}

/// What a project's domain shows while its dev server isn't running. It goes inside a
/// quoted Apache string and a single-quoted Nginx one, so it has neither quote; the domain
/// is a DNS name, so it's safe in both. It reloads itself until the app answers.
fn stopped_page(domain: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>{domain} is not running</title>\
         <body style=font-family:system-ui,sans-serif;max-width:36rem;margin:15vh auto;padding:0 1rem;color:#334155>\
         <h1 style=font-size:1.4rem>{domain} is not running</h1>\
         <p>Its dev server just stopped. LampRabit switches the domain to the project files in a few seconds; this page reloads by itself.</p>\
         <script>setTimeout(function(){{location.reload()}},4000)</script>"
    )
}

/// With both web servers enabled, the sites the other server serves (on its own ports).
/// Each server answers their domains with a redirect, so an address without the port
/// (an old bookmark, or one from before the site moved) still arrives, with its own
/// certificate instead of another site's.
fn elsewhere<'a>(settings: &Settings, sites: &'a [Site], server: Server) -> Vec<&'a Site> {
    if settings.web_server != crate::config::WebServer::Both {
        return Vec::new();
    }
    sites.iter().filter(|s| s.server != server && s.tool.is_none()).collect()
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// A path for Nginx directives that expand variables (`root`, `alias`). Nginx has no escape
/// for `$`, so a folder like `price$tag` would read as a variable and stop the whole
/// server; `${dollar}` (defined in the http block) expands to a literal `$`.
fn ngx_path(p: &Path) -> String {
    slash(p).replace('$', "${dollar}")
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
fn render_apache(settings: &Settings, layout: &Layout, all: &[Site], plan: &[PoolPlan]) -> String {
    let (http, https) = settings.server_ports(Server::Apache);
    let sites: Vec<&Site> = all.iter().filter(|s| s.server == Server::Apache).collect();
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
        "# Generated by LampRabit from the folders in www/. Regenerated on every Apache start.\n\n\
         <VirtualHost *:{http}>\n    ServerName localhost\n    DocumentRoot \"{}\"\n{aliases}</VirtualHost>\n",
        slash(&layout.www())
    );

    let moved = elsewhere(settings, all, Server::Apache);
    if sites.iter().chain(moved.iter()).any(|s| s.ssl) {
        out.push_str(&format!(
            "\nLoadModule ssl_module modules/mod_ssl.so\nListen {https}\nSSLSessionCache \"shmcb:{}/ssl_scache(512000)\"\nSSLProtocol all -SSLv3 -TLSv1 -TLSv1.1\n",
            slash(&layout.tmp())
        ));
        // The first HTTPS host answers names nothing else claims; without this one, that
        // would be whichever project comes first, under its certificate.
        let (crt, key) = ssl::site_cert_paths(layout, "localhost");
        out.push_str(&format!(
            "<VirtualHost *:{https}>\n    ServerName localhost\n    DocumentRoot \"{}\"\n{aliases}    SSLEngine on\n    SSLCertificateFile \"{}\"\n    SSLCertificateKeyFile \"{}\"\n</VirtualHost>\n",
            slash(&layout.www()),
            slash(&crt),
            slash(&key),
        ));
    }
    for site in &moved {
        let body = format!("    ServerName {domain}\n    ServerAlias *.{domain}\n    Redirect 302 / {url}\n", domain = site.domain, url = site.url);
        out.push_str(&format!("\n# {} (served by Nginx)\n<VirtualHost *:{http}>\n{body}</VirtualHost>\n", site.name));
        if site.ssl {
            let (crt, key) = ssl::site_cert_paths(layout, &site.domain);
            out.push_str(&format!(
                "<VirtualHost *:{https}>\n{body}    SSLEngine on\n    SSLCertificateFile \"{}\"\n    SSLCertificateKeyFile \"{}\"\n</VirtualHost>\n",
                slash(&crt),
                slash(&key),
            ));
        }
    }

    for site in sites {
        let body = match &site.proxy {
            // `upgrade` lets the proxied app use WebSockets (Mailpit's live inbox).
            Some(target) => format!(
                "    ServerName {domain}\n    ProxyPreserveHost {keep}\n    ProxyPass / {target} upgrade=websocket\n    ProxyPassReverse / {target}\n{stopped}",
                domain = site.domain,
                keep = if site.proxy_keep_host { "On" } else { "Off" },
                // mod_proxy answers 503 when the app isn't listening.
                stopped = if site.tool.is_none() { format!("    ErrorDocument 503 \"{}\"\n", stopped_page(&site.domain)) } else { String::new() },
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
fn render_nginx(settings: &Settings, layout: &Layout, all: &[Site], plan: &[PoolPlan]) -> String {
    let (http, https) = settings.server_ports(Server::Nginx);
    let home = settings.versions.nginx.as_ref().map(|v| slash(&layout.bin("nginx", v))).unwrap_or_default();
    let logs = slash(&layout.logs());
    let fastcgi_params = format!("{home}/conf/fastcgi_params");
    let sites: Vec<&Site> = all.iter().filter(|s| s.server == Server::Nginx).collect();
    let moved = elsewhere(settings, all, Server::Nginx);

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
                dir = ngx_path(&s.docroot)
            )
        })
        .collect();

    // localhost also takes HTTPS names nothing else claims, rather than the first project.
    let default_tls = if sites.iter().chain(moved.iter()).any(|s| s.ssl) {
        let (crt, key) = ssl::site_cert_paths(layout, "localhost");
        format!("        listen {https} ssl default_server;\n        ssl_certificate \"{}\";\n        ssl_certificate_key \"{}\";\n", slash(&crt), slash(&key))
    } else {
        String::new()
    };
    let mut servers = format!(
        "    server {{\n        listen {http} default_server;\n{default_tls}        server_name localhost;\n        root \"{www}\";\n        index index.php index.html index.htm;\n        autoindex on;\n{aliases}{php}    }}\n",
        www = ngx_path(&layout.www()),
        php = php_location(default_pool, "$document_root$fastcgi_script_name"),
    );
    for site in &moved {
        let mut listen = format!("        listen {http};\n");
        if site.ssl {
            let (crt, key) = ssl::site_cert_paths(layout, &site.domain);
            listen.push_str(&format!("        listen {https} ssl;\n        ssl_certificate \"{}\";\n        ssl_certificate_key \"{}\";\n", slash(&crt), slash(&key)));
        }
        servers.push_str(&format!(
            "\n    # {name} (served by Apache)\n    server {{\n{listen}        server_name {domain} *.{domain};\n        return 302 {url}$request_uri;\n    }}\n",
            name = site.name,
            domain = site.domain,
            url = site.url.trim_end_matches('/'),
        ));
    }
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
                "        location / {{\n            proxy_pass {target};\n            proxy_http_version 1.1;\n            proxy_set_header Host {host};\n            proxy_set_header X-Forwarded-Host $host;\n            proxy_set_header X-Forwarded-Proto $scheme;\n            proxy_set_header Upgrade $http_upgrade;\n            proxy_set_header Connection $connection_upgrade;\n        }}\n{stopped}",
                host = if site.proxy_keep_host { "$host" } else { "$proxy_host" },
                // Nginx answers 502 when the app isn't listening; the code stays 502.
                stopped = if site.tool.is_none() {
                    format!(
                        "        error_page 502 504 /__lamprabit_stopped;\n        location = /__lamprabit_stopped {{\n            internal;\n            default_type text/html;\n            return 200 '{}';\n        }}\n",
                        stopped_page(&site.domain)
                    )
                } else {
                    String::new()
                },
            ),
            None => format!(
                "        root \"{root}\";\n        index index.php index.html index.htm;\n        location / {{\n            try_files $uri $uri/ /index.php?$query_string;\n        }}\n        location ~ /\\.ht {{ deny all; }}\n{php}",
                root = ngx_path(&site.docroot),
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
        r#"# Generated by LampRabit. Changes are overwritten on every start.
worker_processes 1;
error_log "{logs}/nginx-error.log" warn;
pid "{pid}";

events {{
    worker_connections 1024;
}}

http {{
    include "{home}/conf/mime.types";
    default_type application/octet-stream;
    # LampRabit's own health checks would otherwise add a line every few seconds.
    map $http_user_agent $lamprabit_log {{
        ~^lampRabit/ 0;
        default 1;
    }}
    map $http_upgrade $connection_upgrade {{
        default upgrade;
        '' close;
    }}
    # A literal "$" for paths that contain one (see ngx_path).
    geo $dollar {{
        default "$";
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
    let moved = elsewhere(settings, sites, server);
    let ssl_sites: Vec<&Site> = sites.iter().filter(|s| s.ssl && s.server == server).chain(moved.into_iter().filter(|s| s.ssl)).collect();
    if !ssl_sites.is_empty() {
        let (ca, created) = ssl::ensure_ca(layout)?;
        created_ca = created;
        ssl::ensure_site_cert(layout, &ca, "localhost")?;
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

/// The folder a project serves from.
fn docroot(dir: &Path) -> PathBuf {
    if dir.join("public").is_dir() {
        return dir.join("public");
    }
    ["web", "webroot", "pub"].iter().map(|d| dir.join(d)).find(|d| d.join("index.php").exists()).unwrap_or_else(|| dir.to_path_buf())
}

pub fn hosts_file() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join(r"System32\drivers\etc\hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

/// Domains currently in LampRabit's block of the hosts file.
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

/// Whether the hosts file has LampRabit's block (so removing it is worth a prompt).
pub fn hosts_has_block() -> bool {
    std::fs::read(hosts_file()).is_ok_and(|h| String::from_utf8_lossy(&h).lines().any(|l| l.trim() == HOSTS_BEGIN))
}

/// The hosts file without LampRabit's block, everything else byte for byte.
pub fn hosts_without_block(current: &[u8]) -> Vec<u8> {
    render_hosts(current, &[])
}

pub fn hosts_in_sync(sites: &[Site]) -> bool {
    let wanted: BTreeSet<String> = sites.iter().map(|s| s.domain.clone()).collect();
    std::fs::read(hosts_file()).is_ok_and(|h| managed_domains(&String::from_utf8_lossy(&h)) == wanted)
}

/// The new hosts file: every byte outside LampRabit's block is kept as it was (the file
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

/// Rewrites LampRabit's block of the hosts file. This needs administrator rights, so the
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
                "Could not update {}: {r} If security software protects the hosts file, allow LampRabit or add the entries by hand.",
                hosts.display()
            ),
            None => format!("Could not update {}.", hosts.display()),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docroot_follows_the_framework_layout() {
        let base = std::env::temp_dir().join(format!("lamprabit-docroot-{}", std::process::id()));
        for (project, folder) in [("laravel", "public"), ("drupal", "web"), ("cake", "webroot"), ("magento", "pub")] {
            std::fs::create_dir_all(base.join(project).join(folder)).unwrap();
            std::fs::write(base.join(project).join(folder).join("index.php"), "").unwrap();
            assert_eq!(docroot(&base.join(project)), base.join(project).join(folder));
        }
        // A folder named web without an index.php is just a folder.
        std::fs::create_dir_all(base.join("plain/web")).unwrap();
        assert_eq!(docroot(&base.join("plain")), base.join("plain"));
        let _ = std::fs::remove_dir_all(base);
    }
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
            proxy_keep_host: false,
            app: None,
            node_project: false,
            language: Language::Static,
            serve_mode: ServeMode::Static,
            route: None,
            ports: Vec::new(),
            route_port: None,
            php: None,
            server: Server::Apache,
            node: None,
            framework: None,
            git_branch: None,
            database: None,
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

    #[test]
    fn folder_names_become_labels() {
        assert_eq!(label("My App").as_deref(), Some("my-app"));
        assert_eq!(label("Café").as_deref(), Some("cafe"));
        assert_eq!(label("Straße").as_deref(), Some("strasse"));
        assert_eq!(label("price$tag").as_deref(), Some("price-tag"));
        assert_eq!(label("日本語"), None);
        assert_eq!(label(&"a".repeat(64)), None);
    }

    #[test]
    fn sites_on_the_other_server_redirect_and_unknown_names_reach_localhost() {
        let mut settings = Settings::default();
        settings.web_server = crate::config::WebServer::Both;
        let layout = Layout::new(PathBuf::from("/lr"));
        let mut wp = site("blog.test");
        wp.ssl = true;
        let mut app = site("app.test");
        app.ssl = true;
        app.server = Server::Nginx;
        app.url = "https://app.test:8443/".into();
        let sites = [wp, app];

        let apache = render(&settings, &layout, &sites, &[], Server::Apache);
        assert_eq!(apache.matches("ServerName localhost").count(), 2, "localhost answers on both ports");
        let tls_default = apache.find("SSLEngine on").unwrap();
        assert!(apache[..tls_default].contains("ServerName localhost"), "the first HTTPS host is localhost, not a project");
        assert!(apache.contains("ServerName app.test
    ServerAlias *.app.test
    Redirect 302 / https://app.test:8443/"));
        let (crt, _) = ssl::site_cert_paths(&layout, "app.test");
        assert!(apache.contains(&slash(&crt)), "the redirect uses the site's own certificate");

        let nginx = render(&settings, &layout, &sites, &[], Server::Nginx);
        assert!(nginx.contains("ssl default_server"));
        assert!(nginx.contains("server_name blog.test *.blog.test;
        return 302"));

        // With one web server there's nothing to redirect.
        settings.web_server = crate::config::WebServer::Apache;
        assert!(!render(&settings, &layout, &sites[..1], &[], Server::Apache).contains("Redirect 302"));
    }

    #[test]
    fn stopped_page_fits_inside_both_servers_quotes() {
        let page = stopped_page("shop.test");
        assert!(!page.contains('\'') && !page.contains('"'), "Nginx quotes it with ' and Apache with \"");
    }

    #[test]
    fn nginx_paths_keep_dollar_signs_literal() {
        assert_eq!(ngx_path(Path::new("C:/www/price$tag")), "C:/www/price${dollar}tag");
        assert_eq!(ngx_path(Path::new("C:/www/a${x}b")), "C:/www/a${dollar}{x}b");
    }
}
