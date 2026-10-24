//! Everyday conveniences: terminal and editor launchers, and the Microsoft Visual C++
//! runtime that Apache and PHP need on Windows.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    config::Settings,
    error::{Error, Result},
    layout::Layout,
    procgroup,
};

/// Folders added to PATH in lampRabit terminals: active PHP, Composer, MariaDB client.
fn tool_paths(settings: &Settings, layout: &Layout) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(v) = &settings.versions.php {
        paths.push(layout.bin("php", v));
    }
    let composer = layout.root().join("bin").join("composer");
    if composer.join("composer.phar").exists() {
        write_composer_shim(&composer);
        paths.push(composer);
    }
    if let Some(v) = &settings.versions.mariadb {
        paths.push(layout.bin("mariadb", v).join("bin"));
    }
    paths.retain(|p| p.exists());
    paths
}

/// `composer` as a command next to composer.phar.
fn write_composer_shim(dir: &Path) {
    if cfg!(windows) {
        let _ = std::fs::write(dir.join("composer.bat"), "@php \"%~dp0composer.phar\" %*\r\n");
    } else {
        let shim = dir.join("composer");
        if std::fs::write(&shim, "#!/bin/sh\nexec php \"$(dirname \"$0\")/composer.phar\" \"$@\"\n").is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755));
            }
        }
    }
}

/// Opens a terminal in `dir` with lampRabit's tools first on PATH.
pub fn open_terminal(settings: &Settings, layout: &Layout, dir: &Path) -> Result<()> {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(tool_paths(settings, layout).into_iter().chain(std::env::split_paths(&existing)))
        .map_err(|e| Error::msg(e.to_string()))?;
    if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "lampRabit", "cmd", "/K", "echo lampRabit terminal: php, composer and mariadb are on PATH"])
            .current_dir(dir)
            .env("PATH", path)
            .spawn()?;
        return Ok(());
    }
    if cfg!(target_os = "macos") {
        let script = format!(
            "tell application \"Terminal\" to do script \"cd '{}'; export PATH='{}'\"",
            dir.display(),
            path.to_string_lossy()
        );
        std::process::Command::new("osascript").args(["-e", &script, "-e", "tell application \"Terminal\" to activate"]).spawn()?;
        return Ok(());
    }
    for term in ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"] {
        if std::process::Command::new(term).current_dir(dir).env("PATH", &path).spawn().is_ok() {
            return Ok(());
        }
    }
    Err(Error::msg("No terminal emulator found."))
}

/// Opens a folder in the configured editor command (VS Code's `code` by default).
pub async fn open_in_editor(settings: &Settings, dir: &Path) -> Result<()> {
    let editor = settings.editor.trim();
    if editor.is_empty() {
        return Err(Error::msg("Set an editor command in Settings → General."));
    }
    let mut cmd = if cfg!(windows) {
        // Editor launchers are usually .cmd shims (code.cmd), which need cmd to run.
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", editor]).arg(dir);
        c
    } else {
        let mut c = tokio::process::Command::new(editor);
        c.arg(dir);
        c
    };
    procgroup::prepare(&mut cmd);
    let out = cmd.output().await.map_err(|_| Error::msg(format!("Couldn't run \"{editor}\". Check Settings → General → Editor.")))?;
    if !out.status.success() {
        return Err(Error::msg(format!(
            "\"{editor}\" didn't start. Make sure it's installed and on your PATH (VS Code: Command Palette → \"Shell Command: Install 'code' command\"), or change it in Settings → General."
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcRuntime {
    /// Only Windows needs it.
    pub applicable: bool,
    pub installed: bool,
    pub version: Option<String>,
}

pub fn vc_runtime() -> VcRuntime {
    if !cfg!(windows) {
        return VcRuntime { applicable: false, installed: true, version: None };
    }
    let sys = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
    let files = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"].iter().all(|f| sys.join(f).exists());
    let mut version = None;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(out) = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64", "/v", "Version"])
            .creation_flags(0x0800_0000)
            .output()
        {
            version = String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.contains("Version"))
                .and_then(|l| l.split_whitespace().last())
                .map(|v| v.trim_start_matches('v').to_string());
        }
    }
    VcRuntime { applicable: true, installed: files, version }
}

/// Downloads Microsoft's Visual C++ redistributable and installs it (one admin prompt).
pub async fn install_vc_runtime(client: &reqwest::Client, layout: &Layout) -> Result<()> {
    let err = |e: reqwest::Error| Error::msg(format!("Download failed: {e}"));
    let bytes = client
        .get("https://aka.ms/vc14/vc_redist.x64.exe")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(err)?
        .bytes()
        .await
        .map_err(err)?;
    std::fs::create_dir_all(layout.tmp())?;
    let setup = layout.tmp().join("vc_redist.x64.exe");
    std::fs::write(&setup, &bytes)?;
    // Microsoft signs the installer; Windows verifies that signature when elevating.
    let launcher = format!(
        "try {{ $p = Start-Process -FilePath '{}' -ArgumentList '/install','/quiet','/norestart' -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        setup.display().to_string().replace('\'', "''")
    );
    let mut cmd = tokio::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &launcher]);
    procgroup::prepare(&mut cmd);
    let status = cmd.status().await?;
    let _ = std::fs::remove_file(&setup);
    match status.code() {
        // 3010: installed, restart recommended; 1638: a newer version is already there.
        Some(0) | Some(3010) | Some(1638) => Ok(()),
        Some(1223) => Err(Error::msg("Installing the Visual C++ runtime was cancelled.")),
        code => Err(Error::msg(format!("The Visual C++ installer exited with code {}.", code.unwrap_or(-1)))),
    }
}
