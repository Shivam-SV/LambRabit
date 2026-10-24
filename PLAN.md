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
- [x] **6. Feature set** (see below)
- [ ] **7. Comfort and PHP tooling** (see below)
- [ ] **8. Release**: installers (NSIS/MSI, dmg, AppImage/deb), signed auto-updates, CI

## Phase 6: feature set

Platform rule for everything below: a setting or tool is shown only on platforms where it works; where it doesn't, say why instead of hiding it silently.

Order: 6.1 → 6.2 → 6.3 → 6.4 → 6.5 (reliability first; per-project PHP and presets build on the web-server work).

### 6.1 Service reliability ✅ (plus custom title bar)
- **Health checks**: beyond "process alive": Apache answers an HTTP probe, MariaDB answers `mariadb-admin ping`, configs pass `httpd -t` before start. States: running / stopped / degraded / failed, each with a plain-language cause and fix (known patterns: port in use, missing VC++ runtime, bad config line, InnoDB recovery).
- **Port conflicts**: identify the owning process (Windows: TCP table → PID → name; macOS/Linux: `lsof`), explain common culprits (IIS/`http.sys` as PID 4, Skype, another XAMPP/Laragon), offer "use another port" or "stop that process" when it's safe.
- **Activity logs**: existing System stream plus filters by severity and time range, and export (save dialog → `.log`/`.txt`).
- **Diagnostics report**: OS and app versions, runtimes, ports and owners, config test output, hosts/CA status, recent errors; copy or save as a file, with user paths optionally redacted.

### 6.2 Web servers ✅
- **Apache, Nginx or both**: Nginx from nginx.org (Windows zips; macOS/Linux via own builds or package manager). PHP for Nginx runs as FastCGI (`php-cgi` pool on Windows, `php-fpm` elsewhere). With both enabled, each site picks its server; the second server gets its own ports (default 8080/8443) and the UI blocks overlapping ports before start.

### 6.3 Projects ✅
- **Create project** wizard: name → folder in `www/` → preset → options (database, HTTPS) → done, with live progress.
- **Presets**: Blank PHP, Laravel, WordPress, Symfony, static. A preset can create a database and user, pick the PHP version, and enable tools (Mailpit, Redis). Needs Composer (downloaded `composer.phar`) and WP-CLI or the WordPress zip.
- **Per-project PHP version** (decided: yes): PHP runs as FastCGI per installed version (`php-cgi` pool on Windows, `php-fpm` elsewhere) instead of a single `mod_php`; each site picks its version, default is the global active one. Shared with Nginx support in 6.2, so 6.2 lands first.

### 6.4 Tools and tunnels ✅
- **Node.js via NVM** (decided): not installed by default. When the user asks, lampRabit installs NVM and from then on manages Node versions through it (list, install, use, `.nvmrc` per project).
  - Windows: nvm-windows. Its `nvm use` creates a symlink, which needs admin rights or Developer Mode; the UI says so before installing.
  - macOS/Linux: nvm's official install script into `~/.nvm`; commands run through a login shell that sources `nvm.sh`.
  - An existing NVM install is detected and used instead of installing a second copy.
- **Tunnels: Ngrok + Cloudflare** (decided): app-level (expose `localhost`) and per-project (expose `myapp.test` with a host-header rewrite, or a manual port).
  - Ngrok (default): binary downloaded on demand, auth token stored in the OS keychain, free-plan limits explained in the UI.
  - Cloudflare quick tunnels (`cloudflared`): no account needed, random `trycloudflare.com` URL.
- **Redis / Memcached**: no official Windows builds for either. Windows: Redis-compatible alternative (Garnet) or Memurai, or "not available" with explanation; macOS/Linux: manage a package-manager install (brew/apt) or a downloaded build. PHP extensions (`redis`, `memcached`) offered only where builds exist for the active PHP.

### 6.5 Updates ✅
- **App updates**: Tauri updater with signed releases on GitHub; check on start (opt-out) and from Settings. Depends on the release pipeline in phase 8.
- **Runtime updates**: badge when a newer patch of an installed Apache/PHP/MariaDB is available.

## Phase 7: comfort and PHP tooling

### 7.1 Comfort
- Tray menu (start/stop all, open sites, quit), start with the OS, optionally start services on launch.
- "Open terminal" with PHP, Composer, MariaDB and Node on PATH; "Open in editor" for projects.
- Microsoft Visual C++ runtime check on Windows, with a link to (or download of) the redistributable.
- MariaDB data directory per major version, or an automatic `mariadb-upgrade` when moving up.

### 7.2 PHP extension manager
Browse, enable, disable and add PHP extensions per installed PHP version, with compatibility shown for each.

- **Bundled extensions**: list what each PHP build ships (Windows: `ext/php_*.dll`; elsewhere: `php -m` plus shared objects in the extension dir). Toggle them on or off per version; the choice is stored in settings and written into that version's generated php.ini, then the PHP pool restarts.
- **Extra extensions** (PECL and vendor builds): Xdebug, Redis, APCu, Imagick, MongoDB, PCOV, Memcached and similar.
  - Windows: download prebuilt DLLs matching the PHP minor version, thread safety (lampRabit's PHP builds are thread-safe) and compiler (vs16 / vs17) from downloads.php.net/~windows/pecl and xdebug.org. Show "available for PHP 8.3, not yet for 8.5" style compatibility from what actually exists for download.
  - macOS / Linux: depends on how PHP itself is provided there (static builds can't load shared extensions); show "built in" / "not available on this platform" accordingly.
- Extensions with extra requirements are flagged (Imagick needs ImageMagick DLLs; Xdebug gets a ready-made `xdebug.mode=debug` and IDE port setting).
- Status per extension: built in, enabled, disabled, downloadable, unavailable (with the reason).

### 7.3 FrankenPHP: dropped for now

Decision (2026-10-24): not planned. The assessment below is kept for reference if it is revisited.

#### Assessment
FrankenPHP is a PHP application server built on Caddy: the web server and PHP run in one process, with automatic HTTPS, HTTP/2 and HTTP/3, and a "worker mode" that keeps the application booted between requests (Laravel Octane, Symfony Runtime).

**Availability (checked against v1.13.3 releases):** Linux x86_64 and aarch64, macOS Apple Silicon (no Intel Mac build), and a Windows x86_64 zip. All three platforms can be offered; Intel Macs get "not available" with the reason.

**How it fits lampRabit**
- It is a third web server, not a PHP version: each FrankenPHP release embeds its own PHP, so it doesn't use the PHP pools, php.ini per version, or the extension manager. Its PHP version is shown read-only on the site card, and its bundled extensions are listed (static builds include the common ones).
- Per-site choice: Settings → Web server gains "FrankenPHP" as an option next to Apache and Nginx; sites can pick it like they pick Nginx today. It gets its own ports when another server holds 80/443.
- Config: lampRabit generates a Caddyfile with one `php_server` block per site, using lampRabit's own certificates (`tls <cert> <key>`) so the existing trusted CA keeps working, and Caddy's automatic HTTPS turned off to avoid a second CA.
- Worker mode as a per-site toggle (off by default) for projects that support it; a restart button on the site card reloads workers after code changes.
- Mail: `sendmail_path`/SMTP settings through FrankenPHP's php.ini (`PHP_INI_SCAN_DIR`) so Mailpit keeps catching mail.
- Logs, health checks (HTTP probe), port-conflict detection and the diagnostics report extend to it like the other servers.

**Risks / open questions**
- Windows build is the newest platform; needs a spike (start, PHP extensions loaded, our certificates, `.user.ini`) before committing.
- Extensions are fixed at build time; projects needing an extension that isn't in the build must use Apache/Nginx.
- Worker mode changes app behaviour (state persists between requests); keep it opt-in with a clear explanation.

**Recommendation at the time:** an optional third server after the extension manager, starting with a spike on Windows and Linux.
