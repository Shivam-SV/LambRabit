//! Port discovery and routing. A project's domain goes to whatever its own processes are
//! listening on (a dev server started from LampRabit or from your own terminal), and to
//! its files when nothing is. A watcher in lib.rs calls `discover` every few seconds,
//! decides each project's route with `choose`, and restarts the web server when a route
//! changes. Routes live here rather than in Settings because they follow running processes,
//! not choices; `sites::scan` reads them when it renders the vhosts.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::sites::Language;

/// A port a project's process listens on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub port: u16,
    /// Where the proxy reaches it: `127.0.0.1`, or `[::1]` for servers that only listen on
    /// IPv6 (Vite and others on "localhost" with newer Node versions).
    pub host: String,
    pub pid: u32,
    /// The program's name (`node`, `php`…).
    pub process: String,
    /// Its command line, shortened.
    pub command: String,
}

/// Where a domain is forwarded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub port: u16,
    pub host: String,
}

static ROUTES: LazyLock<RwLock<BTreeMap<String, Route>>> = LazyLock::new(Default::default);
static DETECTED: LazyLock<RwLock<BTreeMap<String, Vec<Detected>>>> = LazyLock::new(Default::default);
/// Wakes the watcher early, e.g. when a project's app starts or stops.
pub static WAKE: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);

pub fn poke() {
    WAKE.notify_one();
}

pub fn route_of(name: &str) -> Option<Route> {
    ROUTES.read().ok()?.get(name).cloned()
}

pub fn routes() -> BTreeMap<String, Route> {
    ROUTES.read().map(|r| r.clone()).unwrap_or_default()
}

pub fn set_routes(routes: BTreeMap<String, Route>) {
    if let Ok(mut r) = ROUTES.write() {
        *r = routes;
    }
}

pub fn detected_of(name: &str) -> Vec<Detected> {
    DETECTED.read().ok().and_then(|d| d.get(name).cloned()).unwrap_or_default()
}

/// Stores what the last scan found; true when it differs from before.
pub fn set_detected(found: BTreeMap<String, Vec<Detected>>) -> bool {
    let Ok(mut d) = DETECTED.write() else { return false };
    let changed = *d != found;
    *d = found;
    changed
}

/// Where a project's domain should go.
///
/// - "Serve its files" (a Node project's choice) wins: no route.
/// - A port you picked, while it's listening.
/// - The app LampRabit runs for the project, once its port listens.
/// - Otherwise the lowest port its processes listen on (a dev server before its HMR or
///   Storybook ports), except for PHP projects: their package.json dev server (Vite for
///   assets) must not take over a site PHP serves, so they're only routed to a port you pick.
pub fn choose(language: Language, files_only: bool, pinned: Option<u16>, managed: Option<u16>, detected: &[Detected]) -> Option<Route> {
    if files_only {
        return None;
    }
    let route = |port: u16| detected.iter().find(|d| d.port == port).map(|d| Route { port, host: d.host.clone() });
    if let Some(r) = pinned.and_then(route) {
        return Some(r);
    }
    if language == Language::Php {
        return None;
    }
    managed.and_then(route).or_else(|| detected.iter().min_by_key(|d| d.port).map(|d| Route { port: d.port, host: d.host.clone() }))
}

/// A listening socket and its process.
struct Listener {
    port: u16,
    pid: u32,
    /// The proxy's address for it; None when it listens on a specific outside address only.
    host: Option<String>,
}

/// The ports each project's processes listen on. A process belongs to a project when its
/// working folder (or, if that can't be read, a path in its command line) is inside it.
pub fn discover(system: &mut System, projects: &[(String, PathBuf)]) -> BTreeMap<String, Vec<Detected>> {
    let own = std::process::id();
    // One entry per (process, port); IPv4 wins over IPv6 for the proxy's address.
    let mut sockets: BTreeMap<(u32, u16), String> = BTreeMap::new();
    for l in listeners() {
        let Some(host) = l.host else { continue };
        if l.pid <= 4 || l.pid == own {
            continue;
        }
        let entry = sockets.entry((l.pid, l.port)).or_insert_with(|| host.clone());
        if host == "127.0.0.1" {
            *entry = host;
        }
    }
    if sockets.is_empty() {
        return BTreeMap::new();
    }
    let pids: Vec<Pid> = sockets.keys().map(|(pid, _)| Pid::from_u32(*pid)).collect();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always).with_cmd(UpdateKind::Always).with_exe(UpdateKind::OnlyIfNotSet),
    );
    let roots: Vec<(String, String)> = projects.iter().map(|(name, path)| (name.clone(), comparable(path))).collect();
    let inside = |p: &Path| {
        let p = comparable(p);
        roots.iter().find(|(_, root)| p == *root || p.starts_with(&format!("{root}/"))).map(|(name, _)| name.clone())
    };

    let mut owners: HashMap<u32, Option<String>> = HashMap::new();
    let mut found: BTreeMap<String, Vec<Detected>> = BTreeMap::new();
    for ((pid, port), host) in sockets {
        let Some(process) = system.process(Pid::from_u32(pid)) else { continue };
        let project = owners
            .entry(pid)
            .or_insert_with(|| match process.cwd() {
                Some(cwd) if !cwd.as_os_str().is_empty() => inside(cwd),
                _ => process.cmd().iter().find_map(|arg| inside(Path::new(arg))),
            })
            .clone();
        let Some(project) = project else { continue };
        let command: String = process.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
        let command = if command.chars().count() > 120 { format!("{}…", command.chars().take(119).collect::<String>()) } else { command };
        // The executable's name: Node renames its main thread ("MainThread"), which is what
        // the process name shows on Linux.
        let name = process
            .exe()
            .and_then(|e| e.file_stem())
            .or_else(|| process.cmd().first().and_then(|c| Path::new(c).file_stem()))
            .map_or_else(|| process.name().to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
        found.entry(project).or_default().push(Detected { port, host, pid, process: name, command });
    }
    for list in found.values_mut() {
        list.sort_by_key(|d| d.port);
    }
    found
}

/// A path for prefix comparison: forward slashes, no trailing slash, and lowercase on
/// Windows, where paths ignore case.
fn comparable(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// The proxy's address for a listener bound to `addr`.
fn proxy_host(addr: std::net::IpAddr) -> Option<String> {
    match addr {
        std::net::IpAddr::V4(v4) if v4.is_unspecified() || v4.is_loopback() => Some("127.0.0.1".into()),
        std::net::IpAddr::V6(v6) if v6.is_unspecified() || v6.is_loopback() => Some("[::1]".into()),
        _ => None,
    }
}

#[cfg(windows)]
fn listeners() -> Vec<Listener> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };
    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

    // The table is a count followed by rows; a u32 buffer keeps the rows aligned.
    fn table(af: u32) -> Vec<u32> {
        let mut size = 0u32;
        // SAFETY: a null buffer asks for the size needed.
        unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, af, TCP_TABLE_OWNER_PID_LISTENER, 0) };
        for _ in 0..4 {
            let mut buf = vec![0u32; (size as usize).div_ceil(4) + 1];
            let mut bytes = (buf.len() * 4) as u32;
            // SAFETY: buf holds `bytes` writable bytes.
            let result = unsafe { GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut bytes, 0, af, TCP_TABLE_OWNER_PID_LISTENER, 0) };
            match result {
                0 => return buf,
                ERROR_INSUFFICIENT_BUFFER => size = bytes,
                _ => break,
            }
        }
        Vec::new()
    }
    let port_of = |raw: u32| u16::from_be((raw & 0xffff) as u16);
    let mut out = Vec::new();

    let v4 = table(AF_INET);
    if let Some(&count) = v4.first() {
        let rows = (v4.len() - 1) * 4 / std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
        // SAFETY: the rows follow the count, and `count` is capped by what the buffer holds.
        let rows = unsafe { std::slice::from_raw_parts(v4[1..].as_ptr().cast::<MIB_TCPROW_OWNER_PID>(), (count as usize).min(rows)) };
        for r in rows {
            let addr = std::net::Ipv4Addr::from(u32::from_be(r.dwLocalAddr));
            out.push(Listener { port: port_of(r.dwLocalPort), pid: r.dwOwningPid, host: proxy_host(addr.into()) });
        }
    }
    let v6 = table(AF_INET6);
    if let Some(&count) = v6.first() {
        let rows = (v6.len() - 1) * 4 / std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>();
        // SAFETY: as above.
        let rows = unsafe { std::slice::from_raw_parts(v6[1..].as_ptr().cast::<MIB_TCP6ROW_OWNER_PID>(), (count as usize).min(rows)) };
        for r in rows {
            let addr = std::net::Ipv6Addr::from(r.ucLocalAddr);
            out.push(Listener { port: port_of(r.dwLocalPort), pid: r.dwOwningPid, host: proxy_host(addr.into()) });
        }
    }
    out
}

/// /proc/net/tcp{,6} for listening sockets, then /proc/<pid>/fd to find whose they are.
#[cfg(target_os = "linux")]
fn listeners() -> Vec<Listener> {
    let mut sockets: HashMap<u64, (u16, Option<String>)> = HashMap::new();
    for (file, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for line in text.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // 0A is LISTEN.
            if cols.len() < 10 || cols[3] != "0A" {
                continue;
            }
            let Some((addr, port)) = cols[1].split_once(':') else { continue };
            let (Ok(port), Ok(inode)) = (u16::from_str_radix(port, 16), cols[9].parse::<u64>()) else { continue };
            sockets.insert(inode, (port, parse_proc_addr(addr, v6).and_then(proxy_host)));
        }
    }
    let mut out = Vec::new();
    let Ok(procs) = std::fs::read_dir("/proc") else { return out };
    for entry in procs.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else { continue };
            let target = target.to_string_lossy();
            let Some(inode) = target.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')).and_then(|s| s.parse::<u64>().ok()) else { continue };
            if let Some((port, host)) = sockets.get(&inode) {
                out.push(Listener { port: *port, pid, host: host.clone() });
            }
        }
    }
    out
}

/// /proc/net/tcp addresses: hex in host byte order, per 32-bit word.
#[cfg(target_os = "linux")]
fn parse_proc_addr(hex: &str, v6: bool) -> Option<std::net::IpAddr> {
    if !v6 {
        return Some(std::net::Ipv4Addr::from(u32::from_str_radix(hex, 16).ok()?.swap_bytes()).into());
    }
    let mut bytes = [0u8; 16];
    for i in 0..4 {
        let word = u32::from_str_radix(hex.get(i * 8..i * 8 + 8)?, 16).ok()?.swap_bytes();
        bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    Some(std::net::Ipv6Addr::from(bytes).into())
}

/// `lsof` lists listening sockets with their processes.
#[cfg(target_os = "macos")]
fn listeners() -> Vec<Listener> {
    let Ok(out) = std::process::Command::new("lsof").args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"]).output() else { return Vec::new() };
    let mut list = Vec::new();
    let mut pid = 0u32;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().unwrap_or(0);
        } else if let Some(name) = line.strip_prefix('n') {
            let Some((addr, port)) = name.rsplit_once(':') else { continue };
            let Ok(port) = port.parse() else { continue };
            let host = match addr {
                "*" => Some("127.0.0.1".to_string()),
                a => a.trim_matches(['[', ']']).parse().ok().and_then(proxy_host),
            };
            list.push(Listener { port, pid, host });
        }
    }
    list
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn listeners() -> Vec<Listener> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(port: u16) -> Detected {
        Detected { port, host: "127.0.0.1".into(), pid: 1, process: "node".into(), command: String::new() }
    }

    #[test]
    fn routes_prefer_your_pick_then_lamprabits_app_then_the_lowest_port() {
        let ports = [port(24678), port(5173), port(6006)];
        assert_eq!(choose(Language::Node, false, None, None, &ports).map(|r| r.port), Some(5173));
        assert_eq!(choose(Language::Node, false, None, Some(6006), &ports).map(|r| r.port), Some(6006));
        assert_eq!(choose(Language::Node, false, Some(24678), Some(6006), &ports).map(|r| r.port), Some(24678));
        // A picked port that isn't listening any more falls back.
        assert_eq!(choose(Language::Node, false, Some(9999), None, &ports).map(|r| r.port), Some(5173));
        assert_eq!(choose(Language::Node, true, Some(5173), None, &ports), None, "serving files wins");
        assert_eq!(choose(Language::Node, false, None, None, &[]), None, "nothing listening: files");
    }

    #[test]
    fn php_projects_route_only_to_a_picked_port() {
        let ports = [port(5173), port(8000)];
        assert_eq!(choose(Language::Php, false, None, None, &ports), None, "Laravel's Vite stays an asset server");
        assert_eq!(choose(Language::Php, false, Some(8000), None, &ports).map(|r| r.port), Some(8000));
    }

    #[test]
    fn proxy_addresses_cover_loopback_and_wildcards_only() {
        assert_eq!(proxy_host("0.0.0.0".parse().unwrap()).as_deref(), Some("127.0.0.1"));
        assert_eq!(proxy_host("::1".parse().unwrap()).as_deref(), Some("[::1]"));
        assert_eq!(proxy_host("192.168.1.4".parse().unwrap()), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_proc_net_addresses() {
        assert_eq!(parse_proc_addr("0100007F", false), Some("127.0.0.1".parse().unwrap()));
        assert_eq!(parse_proc_addr("00000000000000000000000001000000", true), Some("::1".parse().unwrap()));
    }

    #[test]
    fn finds_this_test_listener_in_its_folder() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // The test process itself is skipped (it's LampRabit in the app), so check the raw table.
        assert!(listeners().iter().any(|l| l.port == port && l.pid == std::process::id()), "port {port} is listed");
    }
}

