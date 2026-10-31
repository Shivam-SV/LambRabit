//! Collects log output for the Logs page: service stdout/stderr plus the log files the
//! services write themselves (Apache error/access, MariaDB, PHP), tailed by polling.

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Lines kept in memory for pages that open later.
const KEEP: usize = 2000;
/// Upper bound on bytes read from one file per poll.
const MAX_READ: u64 = 1024 * 1024;

/// (file name in logs/, service, stream)
const SOURCES: [(&str, &str, &str); 8] = [
    ("apache-error.log", "apache", "error"),
    ("apache-access.log", "apache", "access"),
    ("nginx-error.log", "nginx", "error"),
    ("nginx-access.log", "nginx", "access"),
    ("mariadb-error.log", "mariadb", "error"),
    ("mysql-error.log", "mysql", "error"),
    ("php-error.log", "php", "error"),
    ("php-fpm.log", "php", "error"),
];

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub id: u64,
    pub service: &'static str,
    pub stream: &'static str,
    pub line: String,
    /// For service "project": which project (its Node app and the commands run for it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Default)]
pub struct LogHub {
    inner: Mutex<(u64, VecDeque<LogLine>)>,
    /// Where system events are persisted (`logs/lampRabit.log`).
    system_file: Mutex<Option<PathBuf>>,
}

pub fn set_system_file(app: &AppHandle, path: PathBuf) {
    *app.state::<LogHub>().system_file.lock().unwrap() = Some(path);
}

#[derive(Debug, Clone, Copy)]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// Records something LampRabit did (service lifecycle, installs, sites, ...)
/// in the System log stream and in `logs/lampRabit.log`.
/// Local time as log files write it: `2026-10-31 12:00:00`.
pub fn stamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]")).unwrap_or_default()
}

pub fn system(app: &AppHandle, level: Level, message: impl Into<String>) {
    let stamp = stamp();
    let level = match level {
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    };
    let line = format!("[{stamp}] {level}: {}", message.into());

    let file = app.state::<LogHub>().system_file.lock().unwrap().clone();
    if let Some(file) = file {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(f, "{line}");
        }
    }
    push(app, "system", "output", line);
}

pub fn push(app: &AppHandle, service: &'static str, stream: &'static str, line: String) {
    let hub = app.state::<LogHub>();
    let entry = {
        let mut guard = hub.inner.lock().unwrap();
        let (seq, lines) = &mut *guard;
        *seq += 1;
        let entry = LogLine { id: *seq, service, stream, line, project: None };
        lines.push_back(entry.clone());
        if lines.len() > KEEP {
            lines.pop_front();
        }
        entry
    };
    let _ = app.emit("log-line", entry);
}

/// A line of a project's output (its Node app or a command run for it), for the Logs page.
/// `stream` is "output", "error" or "command" (the command line itself).
pub fn push_project(app: &AppHandle, project: &str, stream: &'static str, line: String) {
    let hub = app.state::<LogHub>();
    let entry = {
        let mut guard = hub.inner.lock().unwrap();
        let (seq, lines) = &mut *guard;
        *seq += 1;
        let entry = LogLine { id: *seq, service: "project", stream, line, project: Some(project.to_string()) };
        lines.push_back(entry.clone());
        if lines.len() > KEEP {
            lines.pop_front();
        }
        entry
    };
    let _ = app.emit("log-line", entry);
}

/// Projects' log files (logs/apps/<project>.log), written by apps.rs. Each line starts with
/// its time and stream: `[2026-10-31 12:00:00] error: …`.
pub fn project_dir(dir: &Path) -> PathBuf {
    dir.join("apps")
}

/// Files written by LampRabit itself: the system log and captured service console output.
const OWN_FILES: [(&str, &str, &str); 5] = [
    ("lampRabit.log", "system", "output"),
    ("apache-output.log", "apache", "output"),
    ("mariadb-output.log", "mariadb", "output"),
    ("mysql-output.log", "mysql", "output"),
    ("nginx-output.log", "nginx", "output"),
];
/// How much of each file the Logs page loads.
const HISTORY_BYTES: u64 = 96 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct History {
    pub lines: Vec<LogLine>,
    /// Live lines with an id above this arrived after the snapshot.
    pub cursor: u64,
}

/// Reads the recent part of every log file from disk, so history survives restarts.
pub fn history(app: &AppHandle, dir: &Path) -> History {
    let cursor = app.state::<LogHub>().inner.lock().unwrap().0;
    let mut lines = Vec::new();
    for (file, service, stream) in SOURCES.iter().chain(OWN_FILES.iter()) {
        let path = dir.join(file);
        let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
        let from = len.saturating_sub(HISTORY_BYTES);
        let Ok(bytes) = read_range(&path, from, len - from) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut iter = text.split('\n');
        if from > 0 {
            iter.next(); // partial first line
        }
        lines.extend(
            iter.map(|l| l.trim_end_matches('\r'))
                .filter(|l| !l.is_empty())
                .map(|l| LogLine { id: 0, service, stream, line: l.to_string(), project: None }),
        );
    }
    // Projects' output, one file each.
    for entry in std::fs::read_dir(project_dir(dir)).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(project) = path.file_stem().and_then(|n| n.to_str()).filter(|_| path.extension().is_some_and(|e| e == "log")) else { continue };
        let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
        let from = len.saturating_sub(HISTORY_BYTES);
        let Ok(bytes) = read_range(&path, from, len - from) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut iter = text.split('\n');
        if from > 0 {
            iter.next();
        }
        for l in iter.map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty()) {
            let stream = if l.contains("] error: ") { "error" } else if l.contains("] command: ") { "command" } else { "output" };
            lines.push(LogLine { id: 0, service: "project", stream, line: l.to_string(), project: Some(project.to_string()) });
        }
    }
    History { lines, cursor }
}

/// Empties every log file. Returns the files that could not be cleared.
pub fn clear(app: &AppHandle, dir: &Path) -> Vec<String> {
    app.state::<LogHub>().inner.lock().unwrap().1.clear();
    let mut failed = Vec::new();
    for (file, _, _) in SOURCES.iter().chain(OWN_FILES.iter()) {
        let path = dir.join(file);
        if !path.exists() {
            continue;
        }
        if std::fs::OpenOptions::new().write(true).open(&path).and_then(|f| f.set_len(0)).is_err() {
            failed.push(file.to_string());
        }
    }
    for entry in std::fs::read_dir(project_dir(dir)).into_iter().flatten().flatten() {
        if std::fs::OpenOptions::new().write(true).open(entry.path()).and_then(|f| f.set_len(0)).is_err() {
            failed.push(format!("apps/{}", entry.file_name().to_string_lossy()));
        }
    }
    failed
}

/// Per-file read position and any trailing partial line. Existing content is read by
/// `history`, so files present at the first poll are followed from their end.
#[derive(Default)]
pub struct Tailer {
    files: HashMap<PathBuf, (u64, Vec<u8>)>,
    /// The logs directory of the first poll; changes when the environment root moves.
    dir: Option<PathBuf>,
}

impl Tailer {
    pub fn poll(&mut self, app: &AppHandle, dir: &Path) {
        let first_poll = self.dir.as_deref() != Some(dir);
        if first_poll {
            self.files.clear();
            self.dir = Some(dir.to_path_buf());
        }
        for (file, service, stream) in SOURCES {
            let path = dir.join(file);
            let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
            // Files that appear later are new, so they are read from the start.
            let entry = self
                .files
                .entry(path.clone())
                .or_insert_with(|| (if first_poll { len } else { 0 }, Vec::new()));
            if len < entry.0 {
                *entry = (0, Vec::new()); // truncated or replaced
            }
            if len == entry.0 {
                continue;
            }
            let Ok(chunk) = read_range(&path, entry.0, (len - entry.0).min(MAX_READ)) else { continue };
            entry.0 += chunk.len() as u64;
            let mut bytes = std::mem::take(&mut entry.1);
            bytes.extend_from_slice(&chunk);

            let mut lines: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
            let rest = lines.pop().unwrap_or_default().to_vec();
            for line in lines {
                let text = String::from_utf8_lossy(line).trim_end_matches('\r').to_string();
                if !text.is_empty() {
                    push(app, service, stream, text);
                }
            }
            entry.1 = rest;
        }
    }
}

fn read_range(path: &Path, from: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}
