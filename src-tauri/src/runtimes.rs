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
    Php,
    Mariadb,
}

impl Runtime {
    pub const ALL: [Runtime; 3] = [Runtime::Apache, Runtime::Php, Runtime::Mariadb];

    pub fn id(self) -> &'static str {
        match self {
            Self::Apache => "apache",
            Self::Php => "php",
            Self::Mariadb => "mariadb",
        }
    }

    /// File that must exist inside an installed version's directory.
    fn marker(self) -> PathBuf {
        match self {
            Self::Apache => Path::new("bin").join(exe("httpd")),
            Self::Php if cfg!(windows) => PathBuf::from(exe("php")),
            Self::Php => PathBuf::from("php-fpm"),
            Self::Mariadb => Path::new("bin").join(exe("mariadbd")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Archive {
    Zip,
    TarGz,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub runtime: Runtime,
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
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
        Runtime::Mariadb => mariadb_releases(client).await?,
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
    Ok(vec![Release { runtime: Runtime::Apache, version, url, sha256, archive: Archive::Zip, label }])
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
            archive,
            label,
        })
    });
    Ok(futures_util::future::join_all(fetches).await.into_iter().flatten().collect())
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
        let mut downloaded = 0u64;
        let mut last_emit = Instant::now();
        let mut stream = response.bytes_stream();
        progress("downloading", 0, total);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(http)?;
            hasher.update(&chunk);
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
        if let Some(expected) = &release.sha256 {
            let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(Error::msg(format!("checksum mismatch for {name}: download is corrupt")));
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
    let file = std::fs::File::open(archive)?;
    match kind {
        Archive::Zip => zip::ZipArchive::new(file)
            .and_then(|mut z| z.extract(scratch))
            .map_err(|e| Error::msg(format!("could not extract archive: {e}")))?,
        Archive::TarGz => tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(scratch)?,
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
