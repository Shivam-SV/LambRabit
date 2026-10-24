# lampRabit — implementation plan

A Laragon/XAMPP-style local dev environment built with Tauri v2 (Rust) and React.

## Decisions

| Topic | Decision |
|---|---|
| Platforms | Windows, macOS and Linux from day one; OS-specific code is isolated behind `cfg` |
| Binaries | Downloaded on demand from a per-OS version manifest, sha256-verified |
| MVP stack | Apache + PHP + MariaDB |
| PHP integration | Windows: thread-safe PHP loaded into Apache as `mod_php`. macOS/Linux: `php-fpm` + `mod_proxy_fcgi` |
| Binary sources | Windows: Apache Lounge, windows.php.net, mariadb.org. macOS/Linux: static-php-cli builds for PHP, official MariaDB tarballs, Apache from our own builds (fallback: system package manager) |
| Frontend | React 19, TypeScript, Vite, Tailwind v4, TanStack Query, Zustand |

## Environment layout (default `C:\lampRabit` / `~/lampRabit`)

```
bin/<runtime>/<version>/   etc/<runtime>/   www/   data/mariadb/   logs/   tmp/   ssl/
```

## Phases

- [x] **0. Scaffold**: Tauri v2 + React + TS + Tailwind
- [x] **1. Service supervisor**: start/stop/status, port-conflict check, log streaming, cleanup on exit
- [x] **2. Runtime manager**: manifest, download with progress, verify, extract, switch active version
- [x] **3. Config templating**: httpd.conf / php.ini / my.ini / php-fpm.conf from templates; MariaDB data-dir init
- [x] **4. Sites**: `www/foo` → `foo.test` (+ sub-domains, `public/` detection), hosts file kept in sync when Apache starts (one elevation prompt, managed block only), local CA + per-site HTTPS with a global default, System log stream persisted to `logs/lampRabit.log`, log history read from disk with clear, OS-specific settings hidden elsewhere
- [x] **5. Developer utilities**: Adminer / phpMyAdmin at `adminer.test`; Mailpit as a managed service (SMTP 1025, UI 8025) with PHP `mail()` routed to it (Windows: `SMTP`/`smtp_port`; macOS/Linux: `sendmail_path = mailpit sendmail`) and a panel explaining SMTP vs `mail()` settings for frameworks
- [ ] **6. Feature set** (see below)
- [ ] **7. Comfort**: tray menu, autostart, "open terminal" with PATH, open in editor, VC++ runtime check, MariaDB data dir per major version
- [ ] **8. Release**: installers (NSIS/MSI, dmg, AppImage/deb), signed auto-updates, CI

## Phase 6: feature set

Platform rule for everything below: a setting or tool is shown only on platforms where it works; where it doesn't, say why instead of hiding it silently.

Order: 6.1 → 6.2 → 6.3 → 6.4 → 6.5 (reliability first; per-project PHP and presets build on the web-server work).

### 6.1 Service reliability
- **Health checks**: beyond "process alive": Apache answers an HTTP probe, MariaDB answers `mariadb-admin ping`, configs pass `httpd -t` before start. States: running / stopped / degraded / failed, each with a plain-language cause and fix (known patterns: port in use, missing VC++ runtime, bad config line, InnoDB recovery).
- **Port conflicts**: identify the owning process (Windows: TCP table → PID → name; macOS/Linux: `lsof`), explain common culprits (IIS/`http.sys` as PID 4, Skype, another XAMPP/Laragon), offer "use another port" or "stop that process" when it's safe.
- **Activity logs**: existing System stream plus filters by severity and time range, and export (save dialog → `.log`/`.txt`).
- **Diagnostics report**: OS and app versions, runtimes, ports and owners, config test output, hosts/CA status, recent errors; copy or save as a file, with user paths optionally redacted.

### 6.2 Web servers
- **Apache, Nginx or both**: Nginx from nginx.org (Windows zips; macOS/Linux via own builds or package manager). PHP for Nginx runs as FastCGI (`php-cgi` pool on Windows, `php-fpm` elsewhere). With both enabled, each site picks its server; the second server gets its own ports (default 8080/8443) and the UI blocks overlapping ports before start.

### 6.3 Projects
- **Create project** wizard: name → folder in `www/` → preset → options (database, HTTPS) → done, with live progress.
- **Presets**: Blank PHP, Laravel, WordPress, Symfony, static. A preset can create a database and user, pick the PHP version, and enable tools (Mailpit, Redis). Needs Composer (downloaded `composer.phar`) and WP-CLI or the WordPress zip.
- **Per-project PHP version** (decided: yes): PHP runs as FastCGI per installed version (`php-cgi` pool on Windows, `php-fpm` elsewhere) instead of a single `mod_php`; each site picks its version, default is the global active one. Shared with Nginx support in 6.2, so 6.2 lands first.

### 6.4 Tools and tunnels
- **Node.js via NVM** (decided): not installed by default. When the user asks, lampRabit installs NVM and from then on manages Node versions through it (list, install, use, `.nvmrc` per project).
  - Windows: nvm-windows. Its `nvm use` creates a symlink, which needs admin rights or Developer Mode; the UI says so before installing.
  - macOS/Linux: nvm's official install script into `~/.nvm`; commands run through a login shell that sources `nvm.sh`.
  - An existing NVM install is detected and used instead of installing a second copy.
- **Tunnels: Ngrok + Cloudflare** (decided): app-level (expose `localhost`) and per-project (expose `myapp.test` with a host-header rewrite, or a manual port).
  - Ngrok (default): binary downloaded on demand, auth token stored in the OS keychain, free-plan limits explained in the UI.
  - Cloudflare quick tunnels (`cloudflared`): no account needed, random `trycloudflare.com` URL.
- **Redis / Memcached**: no official Windows builds for either. Windows: Redis-compatible alternative (Garnet) or Memurai, or "not available" with explanation; macOS/Linux: manage a package-manager install (brew/apt) or a downloaded build. PHP extensions (`redis`, `memcached`) offered only where builds exist for the active PHP.

### 6.5 Updates
- **App updates**: Tauri updater with signed releases on GitHub; check on start (opt-out) and from Settings. Depends on the release pipeline in phase 8.
- **Runtime updates**: badge when a newer patch of an installed Apache/PHP/MariaDB is available.
