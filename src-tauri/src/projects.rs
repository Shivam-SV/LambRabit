//! "New project": creates a folder in www/ from a preset (blank, static, Laravel, Symfony,
//! WordPress), optionally with a database, and reports progress as it goes.

use std::{
    hash::{BuildHasher, Hasher},
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::{Deserialize, Serialize};
use sha1::Digest as _;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{
    config::{Server, Settings},
    configs,
    error::{Error, Result},
    layout::{Layout, exe},
    procgroup,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Blank,
    Static,
    Laravel,
    Symfony,
    Wordpress,
}

impl Preset {
    fn uses_php(self) -> bool {
        self != Preset::Static
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProject {
    pub name: String,
    pub preset: Preset,
    /// PHP version for the site; None follows the active version.
    pub php: Option<String>,
    pub server: Option<Server>,
    pub ssl: bool,
    /// Database to create; None for no database.
    pub database: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub name: String,
    /// A step heading ("Downloading WordPress") or None for a line of tool output.
    pub step: Option<String>,
    pub line: Option<String>,
}

pub struct Ctx<'a> {
    pub app: &'a AppHandle,
    pub http: &'a reqwest::Client,
    pub settings: &'a Settings,
    pub layout: &'a Layout,
    pub name: &'a str,
}

impl Ctx<'_> {
    pub fn step(&self, step: impl Into<String>) {
        let _ = self.app.emit("project-progress", Progress { name: self.name.into(), step: Some(step.into()), line: None });
    }

    fn line(&self, line: impl Into<String>) {
        let _ = self.app.emit("project-progress", Progress { name: self.name.into(), step: None, line: Some(line.into()) });
    }

    fn dir(&self) -> PathBuf {
        self.layout.www().join(self.name)
    }
}

/// Folder names become DNS labels, so keep them to letters, digits, dashes and underscores.
pub fn validate(req: &NewProject, layout: &Layout) -> Result<()> {
    let name = req.name.trim();
    let ok = !name.is_empty()
        && name.len() <= 63
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with(['-', '_', '.']);
    if !ok {
        return Err(Error::msg("Use letters, numbers, dashes and underscores for the project name (it becomes the domain)."));
    }
    if layout.www().join(name).exists() {
        return Err(Error::msg(format!("www/{name} already exists.")));
    }
    if let Some(db) = &req.database {
        if db.is_empty() || db.len() > 64 || !db.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::msg("Database names can use letters, numbers and underscores (up to 64)."));
        }
    }
    Ok(())
}

/// The PHP binary for CLI work (Composer, artisan), with a php.ini to go with it.
fn php_cli(ctx: &Ctx, version: &str) -> Result<(PathBuf, PathBuf)> {
    let ini = configs::write_php(ctx.settings, ctx.layout, version, ctx.settings.ports.php_fpm)?;
    Ok((ctx.layout.bin("php", version).join(exe("php")), ini))
}

/// Runs a command, streaming its output as progress lines. On failure the error carries
/// the last few lines, which usually say what went wrong.
async fn run(ctx: &Ctx<'_>, program: &Path, args: &[String], cwd: &Path, php_dir: Option<&Path>) -> Result<()> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("COMPOSER_HOME", ctx.layout.data("composer"))
        .env("COMPOSER_NO_INTERACTION", "1");
    if let Some(dir) = php_dir {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&existing));
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
    }
    procgroup::prepare(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| Error::msg(format!("Couldn't run {}: {e}", program.display())))?;

    let tail = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut readers = Vec::new();
    for reader in [child.stdout.take().map(|r| Box::new(r) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|r| Box::new(r) as _)]
        .into_iter()
        .flatten()
    {
        let app = ctx.app.clone();
        let name = ctx.name.to_string();
        let tail = tail.clone();
        readers.push(tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let _ = app.emit("project-progress", Progress { name: name.clone(), step: None, line: Some(line.clone()) });
                let mut t = tail.lock().unwrap();
                t.push(line);
                if t.len() > 6 {
                    t.remove(0);
                }
            }
        }));
    }
    let status = child.wait().await?;
    for r in readers {
        let _ = r.await;
    }
    if status.success() {
        Ok(())
    } else {
        let tail = tail.lock().unwrap().join("\n");
        Err(Error::msg(format!("{} failed:\n{tail}", program.file_name().unwrap_or_default().to_string_lossy())))
    }
}

/// Downloads a file into memory and checks it against a published digest.
async fn download(ctx: &Ctx<'_>, url: &str, digest_url: &str, sha1_digest: bool) -> Result<Vec<u8>> {
    let err = |e: reqwest::Error| Error::msg(format!("Download failed: {e}"));
    let expected = ctx.http.get(digest_url).send().await.and_then(|r| r.error_for_status()).map_err(err)?.text().await.map_err(err)?;
    let expected = expected.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    let bytes = ctx.http.get(url).send().await.and_then(|r| r.error_for_status()).map_err(err)?.bytes().await.map_err(err)?;
    let actual: String = if sha1_digest {
        sha1::Sha1::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
    } else {
        sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
    };
    if actual != expected {
        return Err(Error::msg(format!("{url} failed its checksum check; the download may be corrupt.")));
    }
    Ok(bytes.to_vec())
}

/// Composer lives in bin/composer and is downloaded the first time a preset needs it.
async fn ensure_composer(ctx: &Ctx<'_>) -> Result<PathBuf> {
    let phar = ctx.layout.root().join("bin").join("composer").join("composer.phar");
    if phar.exists() {
        return Ok(phar);
    }
    ctx.step("Downloading Composer");
    const URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar";
    let bytes = download(ctx, URL, &format!("{URL}.sha256"), false).await?;
    std::fs::create_dir_all(phar.parent().unwrap())?;
    std::fs::write(&phar, bytes)?;
    Ok(phar)
}

fn random_string(len: usize) -> String {
    // RandomState is seeded from the OS RNG; good enough for local-only secrets.
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%^&*()-_=+[]{}<>~";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let n = std::collections::hash_map::RandomState::new().build_hasher().finish();
        for i in 0..8 {
            if out.len() < len {
                out.push(CHARS[((n >> (i * 8)) & 0xff) as usize % CHARS.len()] as char);
            }
        }
    }
    out
}

/// Creates the database (MariaDB must be running).
pub async fn create_database(ctx: &Ctx<'_>, database: &str) -> Result<()> {
    ctx.step(format!("Creating database {database}"));
    let home = configs::mariadb_home(ctx.settings, ctx.layout)?;
    let client = home.join("bin").join(exe("mariadb"));
    let args = vec![
        format!("--defaults-file={}", ctx.layout.etc("mariadb").join("my.ini").display()),
        "--user=root".into(),
        "-e".into(),
        format!("CREATE DATABASE IF NOT EXISTS `{database}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"),
    ];
    run(ctx, &client, &args, &home, None).await
}

/// Replaces `KEY=...` lines in a .env file, adding the keys that are missing.
fn set_env(path: &Path, values: &[(&str, String)]) -> Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for (key, value) in values {
        let prefix = format!("{key}=");
        // Laravel ships some DB_ keys commented out ("# DB_HOST=...").
        let commented = format!("# {key}=");
        match lines.iter_mut().find(|l| l.starts_with(&prefix) || l.starts_with(&commented)) {
            Some(line) => *line = format!("{key}={value}"),
            None => lines.push(format!("{key}={value}")),
        }
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

pub async fn build(ctx: &Ctx<'_>, req: &NewProject, url: &str) -> Result<()> {
    let dir = ctx.dir();
    let php_version = req
        .php
        .clone()
        .or_else(|| ctx.settings.versions.php.clone())
        .filter(|_| req.preset.uses_php());
    if req.preset.uses_php() && php_version.is_none() {
        return Err(Error::msg("Install PHP first (Services → PHP)."));
    }
    let db = req.database.as_deref();
    let db_port = ctx.settings.ports.mysql;

    match req.preset {
        Preset::Static => {
            ctx.step("Creating files");
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("index.html"),
                format!("<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{n}</title>\n</head>\n<body>\n  <h1>{n}</h1>\n  <p>Edit www/{n}/index.html to get started.</p>\n</body>\n</html>\n", n = req.name),
            )?;
        }
        Preset::Blank => {
            ctx.step("Creating files");
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("index.php"),
                format!("<?php\n// {n}: served at {url}\n?>\n<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{n}</title>\n</head>\n<body>\n  <h1>{n}</h1>\n  <p>Running PHP <?= PHP_VERSION ?>. Edit www/{n}/index.php to get started.</p>\n</body>\n</html>\n", n = req.name),
            )?;
        }
        Preset::Laravel | Preset::Symfony => {
            let version = php_version.unwrap();
            let composer = ensure_composer(ctx).await?;
            let (php, ini) = php_cli(ctx, &version)?;
            let php_dir = ctx.layout.bin("php", &version);
            let package = if req.preset == Preset::Laravel { "laravel/laravel" } else { "symfony/skeleton" };
            ctx.step(format!("Creating the project with Composer ({package})"));
            let args: Vec<String> = vec![
                "-c".into(),
                ini.to_string_lossy().into(),
                composer.to_string_lossy().into(),
                "create-project".into(),
                package.into(),
                req.name.clone(),
                "--prefer-dist".into(),
                "--no-interaction".into(),
                "--no-progress".into(),
            ];
            run(ctx, &php, &args, &ctx.layout.www(), Some(&php_dir)).await?;

            ctx.step("Configuring the environment");
            let mail_port = ctx.settings.ports.mail_smtp;
            if req.preset == Preset::Laravel {
                let mut values = vec![
                    ("APP_URL", url.trim_end_matches('/').to_string()),
                    ("MAIL_MAILER", "smtp".into()),
                    ("MAIL_HOST", "127.0.0.1".into()),
                    ("MAIL_PORT", mail_port.to_string()),
                ];
                if let Some(db) = db {
                    values.extend([
                        ("DB_CONNECTION", "mariadb".to_string()),
                        ("DB_HOST", "127.0.0.1".into()),
                        ("DB_PORT", db_port.to_string()),
                        ("DB_DATABASE", db.to_string()),
                        ("DB_USERNAME", "root".into()),
                        ("DB_PASSWORD", String::new()),
                    ]);
                }
                set_env(&dir.join(".env"), &values)?;
                if db.is_some() {
                    ctx.step("Running database migrations");
                    let args: Vec<String> = vec!["-c".into(), ini.to_string_lossy().into(), "artisan".into(), "migrate".into(), "--force".into()];
                    run(ctx, &php, &args, &dir, Some(&php_dir)).await?;
                }
            } else {
                let mariadb = ctx.settings.versions.mariadb.clone().unwrap_or_else(|| "11.8".into());
                let mut values = vec![("MAILER_DSN", format!("smtp://127.0.0.1:{mail_port}"))];
                if let Some(db) = db {
                    values.push((
                        "DATABASE_URL",
                        format!("\"mysql://root:@127.0.0.1:{db_port}/{db}?serverVersion={mariadb}-MariaDB&charset=utf8mb4\""),
                    ));
                }
                set_env(&dir.join(".env.local"), &values)?;
            }
        }
        Preset::Wordpress => {
            ctx.step("Downloading WordPress");
            const URL: &str = "https://wordpress.org/latest.zip";
            let bytes = download(ctx, URL, &format!("{URL}.sha1"), true).await?;
            ctx.step("Unpacking WordPress");
            let scratch = ctx.layout.tmp().join(format!("wordpress-{}", req.name));
            let _ = std::fs::remove_dir_all(&scratch);
            zip::ZipArchive::new(std::io::Cursor::new(bytes))
                .and_then(|mut z| z.extract(&scratch))
                .map_err(|e| Error::msg(format!("Couldn't unpack WordPress: {e}")))?;
            std::fs::rename(scratch.join("wordpress"), &dir)?;
            let _ = std::fs::remove_dir_all(&scratch);

            ctx.step("Writing wp-config.php");
            let sample = std::fs::read_to_string(dir.join("wp-config-sample.php"))?;
            let mut config = sample
                .replace("database_name_here", db.unwrap_or("wordpress"))
                .replace("username_here", "root")
                .replace("'password_here'", "''")
                .replace("'localhost'", &format!("'127.0.0.1:{db_port}'"));
            // Fresh authentication keys and salts.
            while let Some(i) = config.find("put your unique phrase here") {
                config.replace_range(i..i + "put your unique phrase here".len(), &random_string(64).replace('\'', "-"));
            }
            std::fs::write(dir.join("wp-config.php"), config)?;
            ctx.line(if db.is_some() {
                "Open the site to finish the WordPress installation (site title and admin account)."
            } else {
                "No database was created; WordPress will ask for one when you open the site."
            });
        }
    }
    Ok(())
}
