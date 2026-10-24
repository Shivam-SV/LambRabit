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
- [ ] **5. Developer utilities**: Adminer / phpMyAdmin at `adminer.test`, Mailpit as a managed service with PHP `mail()` routed to it
- [ ] **6. Comfort**: tray menu, autostart, "open terminal" with PATH, open in editor, VC++ runtime check, MariaDB data dir per major version
- [ ] **7. Extras**: quick-create Laravel/WordPress, Nginx, Redis, Node
- [ ] **8. Release**: installers (NSIS/MSI, dmg, AppImage/deb), auto-updater, CI
