//! Command shims in `<root>/shims` (php, composer, mariadb, mysql…) that always run the
//! active versions with lampRabit's config, and optionally that folder on the user's PATH
//! so they work from any terminal.

use std::path::{Path, PathBuf};

use crate::{
    config::{Database, Settings},
    configs,
    error::{Error, Result},
    layout::{exe, Layout},
};

/// (shim name, MariaDB program, MySQL program). `mariadb*` always mean MariaDB; the
/// `mysql*` names follow Settings → Default database.
const DB_TOOLS: &[(&str, &str, &str)] = &[
    ("mysql", "mariadb", "mysql"),
    ("mysqldump", "mariadb-dump", "mysqldump"),
    ("mysqladmin", "mariadb-admin", "mysqladmin"),
];
const MARIADB_TOOLS: &[&str] = &["mariadb", "mariadb-dump", "mariadb-admin"];

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
    // Sites with their own PHP version get it when the command runs inside their folder.
    let mut php_cases: Vec<Case> = settings
        .sites
        .iter()
        .filter_map(|(name, site)| Some((layout.www().join(name), site.php.as_ref()?)))
        .filter(|(_, v)| Some(*v) != settings.versions.php.as_ref())
        .filter_map(|(folder, v)| php_case(settings, layout, v, Some(folder)))
        .collect();
    php_cases.extend(settings.versions.php.as_ref().and_then(|v| php_case(settings, layout, v, None)));
    if php_cases.iter().any(|c| c.folder.is_none()) {
        shim(&dir, "php", &php_cases)?;
        let phar = settings.versions.composer.as_ref().map(|v| layout.bin("composer", v).join("composer.phar"));
        if let Some(phar) = phar.filter(|p| p.exists()) {
            for case in &mut php_cases {
                case.args.push(path(&phar));
            }
            shim(&dir, "composer", &php_cases)?;
        }
    }
    // The tools read host, port and user from client.cnf, so they reach lampRabit's
    // server as root without any options.
    let db_tool = |db: Database, program: &str| -> Option<Case> {
        let target = configs::database_home(settings, layout, db).ok()?.join("bin").join(exe(program));
        let cnf = configs::write_client_config(settings, layout, db).ok()?;
        target.exists().then(|| Case { folder: None, target, args: vec![format!("--defaults-extra-file={}", path(&cnf))] })
    };
    for name in MARIADB_TOOLS {
        if let Some(case) = db_tool(Database::Mariadb, name) {
            shim(&dir, name, &[case])?;
        }
    }
    if let Some(db) = settings.database() {
        for (name, mariadb, mysql) in DB_TOOLS {
            if let Some(case) = db_tool(db, if db == Database::Mariadb { mariadb } else { mysql }) {
                shim(&dir, name, &[case])?;
            }
        }
    }
    // redis-cli from the active Redis, pointed at lampRabit's port.
    let cli = settings.versions.redis.as_ref().map(|v| layout.bin("redis", v).join(exe("redis-cli")));
    if let Some(target) = cli.filter(|t| t.exists()) {
        let args = vec!["-p".to_string(), settings.ports.redis.to_string()];
        shim(&dir, "redis-cli", &[Case { folder: None, target, args }])?;
    }
    Ok(dir)
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Run `target args…` when the working directory is inside `folder` (or anywhere, for None).
struct Case {
    folder: Option<PathBuf>,
    target: PathBuf,
    args: Vec<String>,
}

fn php_case(settings: &Settings, layout: &Layout, version: &str, folder: Option<PathBuf>) -> Option<Case> {
    let target = layout.bin("php", version).join(exe("php"));
    if !target.exists() {
        return None;
    }
    let args = match configs::write_php(settings, layout, version, settings.ports.php_fpm) {
        Ok(ini) => vec!["-c".to_string(), path(&ini)],
        Err(_) => Vec::new(),
    };
    Some(Case { folder, target, args })
}

/// Writes a shim that picks the first case whose folder contains the working directory;
/// the case without a folder is the fallback.
fn shim(dir: &Path, name: &str, cases: &[Case]) -> Result<()> {
    let fallback = cases.iter().find(|c| c.folder.is_none());
    if cfg!(windows) {
        let run = |c: &Case| {
            let args: String = c.args.iter().map(|a| format!(" \"{a}\"")).collect();
            format!("\"{}\"{args} %*\r\nexit /b %ERRORLEVEL%\r\n", c.target.display())
        };
        let mut script = String::from("@echo off\r\nsetlocal\r\nset \"LR_HERE=%CD%\\\"\r\n");
        let mut labels = String::new();
        for (i, case) in cases.iter().enumerate() {
            if let Some(folder) = &case.folder {
                // Compare the start of %CD%\ with the folder, case-insensitively.
                let prefix = format!("{}\\", folder.display());
                let n = prefix.chars().count();
                script.push_str(&format!("if /I \"%LR_HERE:~0,{n}%\"==\"{prefix}\" goto site{i}\r\n"));
                labels.push_str(&format!(":site{i}\r\n{}", run(case)));
            }
        }
        if let Some(c) = fallback {
            script.push_str(&run(c));
        }
        script.push_str(&labels);
        std::fs::write(dir.join(format!("{name}.cmd")), script)?;
    } else {
        let quote = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
        let run = |c: &Case| {
            let args: String = c.args.iter().map(|a| format!(" {}", quote(a))).collect();
            format!("exec {}{args} \"$@\"", quote(&c.target.to_string_lossy()))
        };
        let mut script = String::from("#!/bin/sh\ncase \"$PWD/\" in\n");
        for case in cases {
            if let Some(folder) = &case.folder {
                script.push_str(&format!("  {}*) {} ;;\n", quote(&format!("{}/", folder.display())), run(case)));
            }
        }
        script.push_str("esac\n");
        if let Some(c) = fallback {
            script.push_str(&run(c));
            script.push('\n');
        }
        let file = dir.join(name);
        std::fs::write(&file, script)?;
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
