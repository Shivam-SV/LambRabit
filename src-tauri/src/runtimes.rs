//! Runtime catalog (what can be downloaded) and installer (download, verify, extract).

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::{
    error::{Error, Result},
    layout::{Layout, exe},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Apache,
    Nginx,
    Php,
    Mariadb,
    /// Developer tools: a mail catcher and two database web UIs.
    Mailpit,
    Adminer,
    Phpmyadmin,
    /// Public tunnels for sharing sites.
    Ngrok,
    Cloudflared,
    /// Redis-compatible server (Microsoft Garnet).
    Redis,
    /// Private .NET runtime that Garnet runs on; installed with Redis, not shown on its own.
    Dotnet,
}

impl Runtime {
    pub const ALL: [Runtime; 11] = [
        Runtime::Apache,
        Runtime::Nginx,
        Runtime::Php,
        Runtime::Mariadb,
        Runtime::Mailpit,
        Runtime::Adminer,
        Runtime::Phpmyadmin,
        Runtime::Ngrok,
        Runtime::Cloudflared,
        Runtime::Redis,
        Runtime::Dotnet,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Php => "php",
            Self::Mariadb => "mariadb",
            Self::Mailpit => "mailpit",
            Self::Adminer => "adminer",
            Self::Phpmyadmin => "phpmyadmin",
            Self::Ngrok => "ngrok",
            Self::Cloudflared => "cloudflared",
            Self::Redis => "redis",
            Self::Dotnet => "dotnet",
        }
    }

    /// File that must exist inside an installed version's directory.
    pub fn marker(self) -> PathBuf {
        match self {
            Self::Apache => Path::new("bin").join(exe("httpd")),
            Self::Nginx => PathBuf::from(exe("nginx")),
            Self::Php if cfg!(windows) => PathBuf::from(exe("php")),
            Self::Php => PathBuf::from("php-fpm"),
            Self::Mariadb => Path::new("bin").join(exe("mariadbd")),
            Self::Mailpit => PathBuf::from(exe("mailpit")),
            Self::Adminer => PathBuf::from("adminer.php"),
            Self::Phpmyadmin => PathBuf::from("index.php"),
            Self::Ngrok => PathBuf::from(exe("ngrok")),
            Self::Cloudflared => PathBuf::from(exe("cloudflared")),
            Self::Redis => PathBuf::from(exe("GarnetServer")),
            Self::Dotnet => PathBuf::from(exe("dotnet")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Archive {
    Zip,
    TarGz,
    TarXz,
    /// A single file, saved as the runtime's marker file.
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub runtime: Runtime,
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
    /// Some publishers (Microsoft .NET) only give SHA-512.
    #[serde(default)]
    pub sha512: Option<String>,
    pub archive: Archive,
    /// Short note shown next to the version, e.g. "LTS".
    pub label: Option<String>,
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("lampRabit/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .expect("http client")
}

fn unsupported(what: &str) -> Error {
    Error::msg(format!("{what} downloads are not available for this OS yet"))
}

fn http(e: reqwest::Error) -> Error {
    Error::msg(format!("network error: {e}"))
}

fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

pub async fn available(client: &reqwest::Client, runtime: Runtime) -> Result<Vec<Release>> {
    let mut releases = match runtime {
        Runtime::Php => php_releases(client).await?,
        Runtime::Apache => apache_releases(client).await?,
        Runtime::Nginx => nginx_releases(client).await?,
        Runtime::Mariadb => mariadb_releases(client).await?,
        Runtime::Mailpit => mailpit_releases(client).await?,
        Runtime::Adminer => adminer_releases(client).await?,
        Runtime::Phpmyadmin => phpmyadmin_releases(client).await?,
        Runtime::Ngrok => ngrok_releases(),
        Runtime::Cloudflared => cloudflared_releases(client).await?,
        Runtime::Redis => garnet_releases(client).await?,
        Runtime::Dotnet => dotnet_releases(client).await?,
    };
    releases.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    Ok(releases)
}

/// windows.php.net lists the newest patch release of every branch, with checksums.
async fn php_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return Err(unsupported("PHP"));
    }
    const BASE: &str = "https://windows.php.net/downloads/releases/";
    let json: Value = client
        .get(format!("{BASE}releases.json"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .json()
        .await
        .map_err(http)?;

    let mut out = Vec::new();
    for branch in json.as_object().into_iter().flat_map(|o| o.values()) {
        let Some(version) = branch["version"].as_str() else { continue };
        // Older branches are end-of-life and have known incompatibilities with current Apache builds.
        if version_key(version) < vec![8, 1] {
            continue;
        }
        // Thread-safe x64 build, required by mod_php.
        let Some(build) = branch
            .as_object()
            .and_then(|o| o.iter().find(|(k, _)| k.starts_with("ts-") && k.ends_with("-x64")))
            .map(|(_, v)| v)
        else {
            continue;
        };
        let Some(path) = build["zip"]["path"].as_str() else { continue };
        out.push(Release {
            runtime: Runtime::Php,
            version: version.to_string(),
            url: format!("{BASE}{path}"),
            sha256: build["zip"]["sha256"].as_str().map(str::to_string),
            sha512: None,
            archive: Archive::Zip,
            label: None,
        });
    }
    Ok(out)
}

/// Apache Lounge only hosts its latest build, so the download page is scraped for it.
async fn apache_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return Err(unsupported("Apache"));
    }
    const SITE: &str = "https://www.apachelounge.com";
    let page = client
        .get(format!("{SITE}/download/"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .text()
        .await
        .map_err(http)?;

    let href = page
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .find(|h| h.contains("/httpd-2.4.") && h.contains("Win64") && h.ends_with(".zip"))
        .ok_or_else(|| Error::msg("could not find an Apache build on apachelounge.com"))?;
    let url = if href.starts_with("http") { href.to_string() } else { format!("{SITE}{href}") };
    let file = url.rsplit('/').next().unwrap_or_default();
    // httpd-2.4.69-261002-Win64-VS18.zip -> 2.4.69
    let version = file
        .strip_prefix("httpd-")
        .and_then(|s| s.split('-').next())
        .ok_or_else(|| Error::msg(format!("unexpected Apache file name {file}")))?
        .to_string();

    // Checksums live next to the archive as "<file>.txt".
    let sha256 = match client.get(format!("{url}.txt")).send().await {
        Ok(r) if r.status().is_success() => r.text().await.ok().and_then(|t| {
            let mut lines = t.lines();
            lines.find(|l| l.starts_with("SHA256"))?;
            lines.next().map(|l| l.trim().to_ascii_lowercase())
        }),
        _ => None,
    };

    let label = file.split('-').find(|p| p.starts_with("VS")).map(|vs| vs.trim_end_matches(".zip").to_string());
    Ok(vec![Release { runtime: Runtime::Apache, version, url, sha256, sha512: None, archive: Archive::Zip, label }])
}

/// The MariaDB REST API lists stable series and the files of their latest release.
async fn mariadb_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let (suffix, archive) = if cfg!(windows) {
        ("winx64.zip", Archive::Zip)
    } else if cfg!(target_os = "linux") {
        ("linux-systemd-x86_64.tar.gz", Archive::TarGz)
    } else {
        return Err(unsupported("MariaDB"));
    };
    const API: &str = "https://downloads.mariadb.org/rest-api/mariadb";

    let index: Value = client
        .get(format!("{API}/"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .json()
        .await
        .map_err(http)?;
    let series: Vec<(String, Option<String>)> = index["major_releases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["release_status"] == "Stable")
        .filter_map(|r| {
            let id = r["release_id"].as_str()?.to_string();
            let label = match r["release_support_type"].as_str() {
                Some("Long Term Support") => Some("LTS".to_string()),
                Some(other) => Some(other.to_string()),
                None => None,
            };
            Some((id, label))
        })
        .collect();

    let fetches = series.into_iter().map(|(id, label)| async move {
        let latest: Value = client.get(format!("{API}/{id}/latest/")).send().await.ok()?.json().await.ok()?;
        let release = latest["releases"].as_object()?.values().next()?;
        let file = release["files"]
            .as_array()?
            .iter()
            .find(|f| f["file_name"].as_str().is_some_and(|n| n.ends_with(suffix) && !n.contains("debug")))?;
        Some(Release {
            runtime: Runtime::Mariadb,
            version: release["release_id"].as_str()?.to_string(),
            url: file["file_download_url"].as_str()?.replacen("http://", "https://", 1),
            sha256: file["checksum"]["sha256sum"].as_str().map(str::to_string),
            sha512: None,
            archive,
            label,
        })
    });
    Ok(futures_util::future::join_all(fetches).await.into_iter().flatten().collect())
}

/// nginx.org lists the mainline and stable Windows builds on its download page. It
/// publishes PGP signatures rather than checksums, so these downloads rely on HTTPS alone.
async fn nginx_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    if !cfg!(windows) {
        return Err(Error::msg(
            "nginx.org only publishes Windows builds. On macOS and Linux, Nginx support will use the system package manager in a later version.",
        ));
    }
    const SITE: &str = "https://nginx.org";
    let page = client
        .get(format!("{SITE}/en/download.html"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .text()
        .await
        .map_err(http)?;
    // In page order: mainline first, then stable, then legacy.
    let mut releases: Vec<Release> = Vec::new();
    for href in page.split("href=\"").skip(1).filter_map(|s| s.split('"').next()) {
        let Some(version) = href.strip_prefix("/download/nginx-").and_then(|v| v.strip_suffix(".zip")) else {
            continue;
        };
        if releases.iter().any(|r| r.version == version) {
            continue;
        }
        let label = match releases.len() {
            0 => Some("Mainline".to_string()),
            1 => Some("Stable".to_string()),
            _ => None,
        };
        releases.push(Release {
            runtime: Runtime::Nginx,
            version: version.to_string(),
            url: format!("{SITE}{href}"),
            sha256: None,
            sha512: None,
            archive: Archive::Zip,
            label,
        });
        if releases.len() == 4 {
            break;
        }
    }
    if releases.is_empty() {
        return Err(Error::msg("could not find Nginx builds on nginx.org"));
    }
    Ok(releases)
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value> {
    client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(http)?
        .json()
        .await
        .map_err(http)
}

/// Latest GitHub release of `repo`, picking the asset `pick` accepts. GitHub reports a
/// sha256 digest for each asset.
async fn github_latest(
    client: &reqwest::Client,
    runtime: Runtime,
    repo: &str,
    archive: Archive,
    pick: impl Fn(&str, &str) -> bool,
) -> Result<Vec<Release>> {
    let latest = get_json(client, &format!("https://api.github.com/repos/{repo}/releases/latest")).await?;
    let version = latest["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    let asset = latest["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str().is_some_and(|n| pick(n, &version)))
        .ok_or_else(|| Error::msg(format!("no download for this platform in {repo} {version}")))?;
    Ok(vec![Release {
        runtime,
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
        sha512: None,
        version,
        archive,
        label: None,
    }])
}

async fn mailpit_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "darwin",
        "linux" => "linux",
        _ => return Err(unsupported("Mailpit")),
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (ext, archive) = if cfg!(windows) { ("zip", Archive::Zip) } else { ("tar.gz", Archive::TarGz) };
    let name = format!("mailpit-{os}-{arch}.{ext}");
    github_latest(client, Runtime::Mailpit, "axllent/mailpit", archive, |n, _| n == name).await
}

/// ngrok publishes a "v3 stable" download per platform, without a version number in the
/// URL or a checksum, so this download relies on HTTPS.
fn ngrok_releases() -> Vec<Release> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (os, ext, archive) = match std::env::consts::OS {
        "windows" => ("windows", "zip", Archive::Zip),
        "macos" => ("darwin", "zip", Archive::Zip),
        _ => ("linux", "tgz", Archive::TarGz),
    };
    vec![Release {
        runtime: Runtime::Ngrok,
        version: "3-stable".into(),
        url: format!("https://bin.equinox.io/c/bNyj1mQVY4c/ngrok-v3-stable-{os}-{arch}.{ext}"),
        sha256: None,
        sha512: None,
        archive,
        label: Some("Stable channel".into()),
    }]
}

async fn cloudflared_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let (name, archive) = match std::env::consts::OS {
        "windows" => (format!("cloudflared-windows-{arch}.exe"), Archive::File),
        "macos" => (format!("cloudflared-darwin-{arch}.tgz"), Archive::TarGz),
        _ => (format!("cloudflared-linux-{arch}"), Archive::File),
    };
    github_latest(client, Runtime::Cloudflared, "cloudflare/cloudflared", archive, |n, _| n == name).await
}

/// Garnet: Microsoft's Redis-compatible server. Its builds need the .NET runtime, which
/// lampRabit installs privately next to it (see `dotnet_releases`).
async fn garnet_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let (name, archive) = match std::env::consts::OS {
        "windows" => (format!("win-{arch}-based-readytorun.zip"), Archive::Zip),
        "macos" => (format!("osx-{arch}-based.tar.xz"), Archive::TarXz),
        _ => (format!("linux-{arch}-based.tar.xz"), Archive::TarXz),
    };
    let mut releases = github_latest(client, Runtime::Redis, "microsoft/garnet", archive, |n, _| n == name).await?;
    for r in &mut releases {
        r.label = Some("Garnet".into());
    }
    Ok(releases)
}

/// The .NET release line Garnet's current builds target (their folder is `net10.0`).
pub const DOTNET_CHANNEL: &str = "10.0";

async fn dotnet_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let meta = get_json(client, &format!("https://builds.dotnet.microsoft.com/dotnet/release-metadata/{DOTNET_CHANNEL}/releases.json")).await?;
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let (rid, ext, archive) = match std::env::consts::OS {
        "windows" => (format!("win-{arch}"), ".zip", Archive::Zip),
        "macos" => (format!("osx-{arch}"), ".tar.gz", Archive::TarGz),
        _ => (format!("linux-{arch}"), ".tar.gz", Archive::TarGz),
    };
    let release = &meta["releases"][0];
    let file = release["runtime"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| f["rid"] == rid.as_str() && f["name"].as_str().is_some_and(|n| n.contains("dotnet-runtime") && n.ends_with(ext)))
        .ok_or_else(|| Error::msg(format!("no .NET runtime download for {rid}")))?;
    Ok(vec![Release {
        runtime: Runtime::Dotnet,
        version: release["runtime"]["version"].as_str().unwrap_or_default().to_string(),
        url: file["url"].as_str().unwrap_or_default().to_string(),
        sha256: None,
        sha512: file["hash"].as_str().map(str::to_string),
        archive,
        label: None,
    }])
}

async fn adminer_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    // The all-drivers, all-languages single file.
    github_latest(client, Runtime::Adminer, "vrana/adminer", Archive::File, |n, v| n == format!("adminer-{v}.php"))
        .await
}

async fn phpmyadmin_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let info = get_json(client, "https://www.phpmyadmin.net/home_page/version.json").await?;
    let version = info["version"].as_str().ok_or_else(|| Error::msg("unexpected phpMyAdmin version data"))?;
    let php = info["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["version"] == version)
        .and_then(|r| r["php_versions"].as_str())
        // ">=7.2,<8.4" -> "PHP <8.4": the upper bound is what users run into.
        .map(|p| format!("PHP {}", p.rsplit(',').next().unwrap_or(p).trim()));
    let url = format!("https://files.phpmyadmin.net/phpMyAdmin/{version}/phpMyAdmin-{version}-all-languages.zip");
    let sha256 = match client.get(format!("{url}.sha256")).send().await {
        Ok(r) if r.status().is_success() => {
            r.text().await.ok().and_then(|t| t.split_whitespace().next().map(str::to_string))
        }
        _ => None,
    };
    Ok(vec![Release {
        runtime: Runtime::Phpmyadmin,
        version: version.to_string(),
        url,
        sha256,
        sha512: None,
        archive: Archive::Zip,
        label: php,
    }])
}

pub fn installed(layout: &Layout, runtime: Runtime) -> Vec<String> {
    let dir = layout.root().join("bin").join(runtime.id());
    let mut versions: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join(runtime.marker()).exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    versions.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    versions
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub runtime: Runtime,
    pub version: String,
    pub stage: &'static str,
    pub downloaded: u64,
    pub total: Option<u64>,
}

pub async fn install(app: &AppHandle, client: &reqwest::Client, layout: &Layout, release: &Release) -> Result<()> {
    let runtime = release.runtime;
    let dest = layout.bin(runtime.id(), &release.version);
    if dest.join(runtime.marker()).exists() {
        return Ok(());
    }
    let progress = |stage, downloaded, total| {
        let _ = app.emit(
            "install-progress",
            InstallProgress { runtime, version: release.version.clone(), stage, downloaded, total },
        );
    };

    std::fs::create_dir_all(layout.tmp())?;
    let name = format!("{}-{}", runtime.id(), release.version);
    let archive_path = layout.tmp().join(format!("{name}.download"));
    let extract_dir = layout.tmp().join(format!("{name}.extract"));

    let result = async {
        let response = client.get(&release.url).send().await.and_then(|r| r.error_for_status()).map_err(http)?;
        let total = response.content_length();
        let mut file = tokio::fs::File::create(&archive_path).await?;
        let mut hasher = Sha256::new();
        let mut hasher512 = sha2::Sha512::new();
        let mut downloaded = 0u64;
        let mut last_emit = Instant::now();
        let mut stream = response.bytes_stream();
        progress("downloading", 0, total);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(http)?;
            hasher.update(&chunk);
            hasher512.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() > Duration::from_millis(150) {
                progress("downloading", downloaded, total);
                last_emit = Instant::now();
            }
        }
        file.flush().await?;
        drop(file);

        progress("verifying", downloaded, total);
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let checks = [(&release.sha256, hex(&hasher.finalize())), (&release.sha512, hex(&hasher512.finalize()))];
        for (expected, actual) in checks {
            if let Some(expected) = expected {
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(Error::msg(format!("checksum mismatch for {name}: download is corrupt")));
                }
            }
        }

        progress("extracting", downloaded, total);
        let (archive_path, extract_dir, dest, marker, kind) =
            (archive_path.clone(), extract_dir.clone(), dest.clone(), runtime.marker(), release.archive);
        tokio::task::spawn_blocking(move || extract_into(&archive_path, kind, &extract_dir, &dest, &marker))
            .await
            .map_err(|e| Error::msg(e.to_string()))??;
        progress("done", downloaded, total);
        Ok(())
    }
    .await;

    let _ = std::fs::remove_file(&archive_path);
    let _ = std::fs::remove_dir_all(&extract_dir);
    if result.is_err() {
        progress("error", 0, None);
    }
    result
}

fn extract_into(archive: &Path, kind: Archive, scratch: &Path, dest: &Path, marker: &Path) -> Result<()> {
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)?;
    }
    if let Archive::File = kind {
        std::fs::create_dir_all(dest)?;
        let target = dest.join(marker);
        std::fs::copy(archive, &target)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
        }
        return Ok(());
    }
    let file = std::fs::File::open(archive)?;
    match kind {
        Archive::File => unreachable!(),
        Archive::Zip => zip::ZipArchive::new(file)
            .and_then(|mut z| z.extract(scratch))
            .map_err(|e| Error::msg(format!("could not extract archive: {e}")))?,
        Archive::TarGz => tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(scratch)?,
        Archive::TarXz => {
            let mut tar_bytes = Vec::new();
            lzma_rs::xz_decompress(&mut std::io::BufReader::new(file), &mut tar_bytes)
                .map_err(|e| Error::msg(format!("could not decompress archive: {e}")))?;
            tar::Archive::new(std::io::Cursor::new(tar_bytes)).unpack(scratch)?
        }
    }

    // Archives wrap their content differently (Apache24/, mariadb-x.y-winx64/, or nothing);
    // the runtime root is whichever directory contains the marker file.
    let root = find_root(scratch, marker, 2)
        .ok_or_else(|| Error::msg(format!("archive does not contain {}", marker.display())))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::rename(root, dest)?;
    Ok(())
}

fn find_root(dir: &Path, marker: &Path, depth: u32) -> Option<PathBuf> {
    if dir.join(marker).exists() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .find_map(|e| find_root(&e.path(), marker, depth - 1))
}

pub fn remove(layout: &Layout, runtime: Runtime, version: &str) -> Result<()> {
    let dir = layout.bin(runtime.id(), version);
    if !dir.starts_with(layout.root().join("bin")) || version.contains("..") {
        return Err(Error::msg("invalid version"));
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotice {
    pub runtime: Runtime,
    pub installed: String,
    pub release: Release,
}

/// Newer patch releases for installed versions: same "major.minor" line (for example
/// PHP 8.4.27 → 8.4.30), never a jump to a new minor that could change behaviour.
pub async fn updates(client: &reqwest::Client, layout: &Layout) -> Vec<UpdateNotice> {
    let checks = [Runtime::Apache, Runtime::Nginx, Runtime::Php, Runtime::Mariadb, Runtime::Mailpit, Runtime::Adminer, Runtime::Phpmyadmin, Runtime::Redis]
        .into_iter()
        .map(|runtime| async move {
            let installed = installed(layout, runtime);
            if installed.is_empty() {
                return Vec::new();
            }
            let Ok(catalog) = available(client, runtime).await else { return Vec::new() };
            let line = |v: &str| v.split('.').take(2).collect::<Vec<_>>().join(".");
            let mut out = Vec::new();
            for have in &installed {
                if let Some(newer) = catalog.iter().find(|r| {
                    line(&r.version) == line(have) && version_key(&r.version) > version_key(have) && !installed.contains(&r.version)
                }) {
                    out.push(UpdateNotice { runtime, installed: have.clone(), release: newer.clone() });
                }
            }
            out
        });
    futures_util::future::join_all(checks).await.into_iter().flatten().collect()
}
