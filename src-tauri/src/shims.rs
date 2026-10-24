//! Command shims in `<root>/shims` (php, composer, mariadb, mysql…) that always run the
//! active versions with lampRabit's config, and optionally that folder on the user's PATH
//! so they work from any terminal.

use std::path::{Path, PathBuf};

use crate::{
    config::Settings,
    configs,
    error::{Error, Result},
    layout::{exe, Layout},
};

const MARIADB_TOOLS: &[&str] = &["mariadb", "mysql", "mariadb-dump", "mysqldump", "mariadb-admin", "mysqladmin"];

pub fn dir(layout: &Layout) -> PathBuf {
    layout.root().join("shims")
}

/// Rewrites every shim for the current active versions; tools that aren't installed get none.
pub fn write(settings: &Settings, layout: &Layout) -> Result<PathBuf> {
    let dir = dir(layout);
    std::fs::create_dir_all(&dir)?;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    if let Some(v) = &settings.versions.php {
        let php = layout.bin("php", v).join(exe("php"));
        if php.exists() {
            let mut args = Vec::new();
            if let Ok(ini) = configs::write_php(settings, layout, v, settings.ports.php_fpm) {
                args = vec!["-c".to_string(), path(&ini)];
            }
            shim(&dir, "php", &php, &args)?;
            let phar = layout.root().join("bin").join("composer").join("composer.phar");
            if phar.exists() {
                args.push(path(&phar));
                shim(&dir, "composer", &php, &args)?;
            }
        }
    }
    if let Some(v) = &settings.versions.mariadb {
        let bin = layout.bin("mariadb", v).join("bin");
        // Picks up the [client] port and socket, so the tools reach lampRabit's server.
        let my_ini = layout.etc("mariadb").join("my.ini");
        let args: Vec<String> =
            if my_ini.exists() { vec![format!("--defaults-extra-file={}", path(&my_ini))] } else { Vec::new() };
        for name in MARIADB_TOOLS {
            let target = bin.join(exe(name));
            if target.exists() {
                shim(&dir, name, &target, &args)?;
            }
        }
    }
    Ok(dir)
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn shim(dir: &Path, name: &str, target: &Path, args: &[String]) -> Result<()> {
    if cfg!(windows) {
        let args: String = args.iter().map(|a| format!(" \"{a}\"")).collect();
        std::fs::write(dir.join(format!("{name}.cmd")), format!("@\"{}\"{args} %*\r\n", target.display()))?;
    } else {
        let args: String = args.iter().map(|a| format!(" '{}'", a.replace('\'', r"'\''"))).collect();
        let file = dir.join(name);
        let target = target.to_string_lossy().replace('\'', r"'\''");
        std::fs::write(&file, format!("#!/bin/sh\nexec '{target}'{args} \"$@\"\n"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

/// Adds or removes the shims folder on the user's PATH. New terminals see the change;
/// ones already open keep their old PATH.
pub fn set_on_path(layout: &Layout, on: bool) -> Result<()> {
    let dir = dir(layout);
    #[cfg(windows)]
    return windows_path::set(&dir, on);
    #[cfg(not(windows))]
    return profile::set(&dir, on);
}

#[cfg(windows)]
mod windows_path {
    use super::*;
    use winreg::{enums::*, RegKey, RegValue};

    /// Edits HKCU\Environment\Path directly (no elevation), keeping it REG_EXPAND_SZ so
    /// entries like %USERPROFILE%\bin survive, then tells running programs it changed.
    pub fn set(dir: &Path, on: bool) -> Result<()> {
        let env = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .map_err(|e| Error::msg(format!("Couldn't open your user environment: {e}")))?;
        let current: String = match env.get_raw_value("Path") {
            Ok(v) => decode(&v.bytes),
            Err(_) => String::new(),
        };
        let mine = dir.to_string_lossy();
        let mut parts: Vec<&str> =
            current.split(';').filter(|p| !p.is_empty() && !p.trim_end_matches('\\').eq_ignore_ascii_case(&mine)).collect();
        if on {
            parts.insert(0, &mine);
        }
        let value = parts.join(";");
        if value == current {
            return Ok(());
        }
        env.set_raw_value("Path", &RegValue { bytes: encode(&value), vtype: REG_EXPAND_SZ })
            .map_err(|e| Error::msg(format!("Couldn't update your PATH: {e}")))?;
        broadcast();
        Ok(())
    }

    fn decode(bytes: &[u8]) -> String {
        let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&wide).trim_end_matches('\0').to_string()
    }

    fn encode(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
    }

    fn broadcast() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let name: Vec<u16> = "Environment".encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, name.as_ptr() as isize, SMTO_ABORTIFHUNG, 2000, std::ptr::null_mut());
        }
    }
}

#[cfg(not(windows))]
mod profile {
    use super::*;

    const START: &str = "# >>> lampRabit >>>";
    const END: &str = "# <<< lampRabit <<<";

    /// A managed block in the shell start-up files. zsh is macOS's default shell, so
    /// .zshrc is created there if missing; elsewhere only existing files are touched,
    /// falling back to .profile.
    pub fn set(dir: &Path, on: bool) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| Error::msg("Couldn't find your home folder"))?;
        let mut files: Vec<PathBuf> =
            [".zshrc", ".bashrc", ".profile"].iter().map(|f| home.join(f)).filter(|f| f.exists()).collect();
        if on && cfg!(target_os = "macos") && !files.iter().any(|f| f.ends_with(".zshrc")) {
            files.push(home.join(".zshrc"));
        }
        if on && files.is_empty() {
            files.push(home.join(".profile"));
        }
        let block = format!("{START}\nexport PATH=\"{}:$PATH\"\n{END}\n", dir.display());
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let mut kept = strip(&text);
            if on {
                if !kept.is_empty() && !kept.ends_with('\n') {
                    kept.push('\n');
                }
                kept.push_str(&block);
            }
            if kept != text {
                std::fs::write(&file, kept)?;
            }
        }
        Ok(())
    }

    fn strip(text: &str) -> String {
        let mut out = String::new();
        let mut inside = false;
        for line in text.split_inclusive('\n') {
            match line.trim_end() {
                START => inside = true,
                END => inside = false,
                _ if !inside => out.push_str(line),
                _ => {}
            }
        }
        out
    }
}
