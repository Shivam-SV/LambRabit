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
/// How much of an existing file to show when it is first seen.
const BACKFILL: u64 = 16 * 1024;
/// Upper bound on bytes read from one file per poll.
const MAX_READ: u64 = 1024 * 1024;

/// (file name in logs/, service, stream)
const SOURCES: [(&str, &str, &str); 5] = [
    ("apache-error.log", "apache", "error"),
    ("apache-access.log", "apache", "access"),
    ("mariadb-error.log", "mariadb", "error"),
    ("php-error.log", "php", "error"),
    ("php-fpm.log", "php", "error"),
];

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub id: u64,
    pub service: &'static str,
    pub stream: &'static str,
    pub line: String,
}

#[derive(Default)]
pub struct LogHub {
    inner: Mutex<(u64, VecDeque<LogLine>)>,
}

pub fn push(app: &AppHandle, service: &'static str, stream: &'static str, line: String) {
    let hub = app.state::<LogHub>();
    let entry = {
        let mut guard = hub.inner.lock().unwrap();
        let (seq, lines) = &mut *guard;
        *seq += 1;
        let entry = LogLine { id: *seq, service, stream, line };
        lines.push_back(entry.clone());
        if lines.len() > KEEP {
            lines.pop_front();
        }
        entry
    };
    let _ = app.emit("log-line", entry);
}

pub fn recent(app: &AppHandle) -> Vec<LogLine> {
    app.state::<LogHub>().inner.lock().unwrap().1.iter().cloned().collect()
}

/// Per-file read position and any trailing partial line.
#[derive(Default)]
pub struct Tailer {
    files: HashMap<PathBuf, (u64, Vec<u8>)>,
}

impl Tailer {
    pub fn poll(&mut self, app: &AppHandle, dir: &Path) {
        for (file, service, stream) in SOURCES {
            let path = dir.join(file);
            let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
            let is_new = !self.files.contains_key(&path);
            let entry = self.files.entry(path.clone()).or_insert_with(|| (len.saturating_sub(BACKFILL), Vec::new()));
            // Backfill usually starts mid-line; drop that fragment.
            let skip_partial = is_new && entry.0 > 0;
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
            if skip_partial && !lines.is_empty() {
                lines.remove(0); // started mid-line during backfill
            }
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
