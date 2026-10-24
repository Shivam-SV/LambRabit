use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Ports {
    pub http: u16,
    pub https: u16,
    pub mysql: u16,
    /// Only used on platforms where PHP runs as php-fpm (macOS/Linux).
    pub php_fpm: u16,
}

/// Active version per runtime; `None` means not installed yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Versions {
    pub apache: Option<String>,
    pub php: Option<String>,
    pub mariadb: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root_dir: default_root(),
            tld: "test".into(),
            ports: Ports::default(),
            versions: Versions::default(),
        }
    }
}

impl Default for Ports {
    fn default() -> Self {
        Self { http: 80, https: 443, mysql: 3306, php_fpm: 9000 }
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
