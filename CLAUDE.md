# lampRabit: project context

A local PHP development environment (Laragon/XAMPP-style) built with **Tauri v2 (Rust) + React 19 + TypeScript + Tailwind v4**. It downloads and runs Apache or Nginx, several PHP versions side by side, MariaDB, MySQL, Mailpit, Redis, Memcached, and serves every folder in `www/` as `https://<folder>.test`. Windows is the reference platform; Linux has full parity; macOS is in preview.

Read [PLAN.md](PLAN.md) for the roadmap and decisions, [RELEASING.md](RELEASING.md) for releases and the Linux runtime builds.

## Layout

- `src-tauri/src/`: the backend. `lib.rs` holds app state, every Tauri command and app setup. Key modules:
  - `services.rs`: the supervisor (start/stop, health probes, port checks, launch specs per `ServiceKind`) and `reap_orphans`.
  - `runtimes.rs`: download catalogs per `Runtime`, install (download, checksum, extract), remove.
  - `configs.rs`: generates `httpd.conf`, `my.ini` (MariaDB and MySQL), `php.ini`, `php-fpm.conf`, and initialises database data folders.
  - `php.rs`: PHP FastCGI pools (`php-cgi` workers on Windows, `php-fpm` elsewhere). `sites.rs`: vhosts and the hosts file. `ssl.rs`: local CA and trust. `shims.rs`: `php`/`composer`/`mariadb` command shims. `procgroup.rs`: process trees (Windows Job Objects, Unix process groups).
  - `config.rs`: `Settings` (saved as `settings.toml` in the app config dir), including ports, active versions and stack profiles.
- `src/`: the frontend. `lib/api.ts` mirrors every command and type, so update it alongside `lib.rs`. Pages are in `pages/`, shared pieces in `components/`.
- `build/linux/` and `.github/workflows/binaries.yml`: lampRabit's own Linux and macOS builds of Apache, Nginx, PHP, Redis and Memcached, plus the repackaged Windows Memcached (see below). The folder name predates macOS.

## Decisions to keep

- **Linux and macOS runtimes are lampRabit's own CI builds** (the user's choice, 2026-10-28, extended to macOS 2026-10-29; distro packages and Homebrew were considered and rejected, as were static-php-cli's and FlyEnv's prebuilt downloads, which lag behind). Apache and Nginx are built in Debian 11 (glibc 2.31) with OpenSSL, PCRE2, zlib and expat compiled in, and natively on macOS runners (APR gets `@rpath` names, binaries are re-signed ad hoc); PHP is built static with static-php-cli and includes `redis`, `memcached` and `igbinary`, since static PHP can't load extensions later. Versions and source checksums are pinned in `build/linux/versions.env`. The workflow publishes to the `binaries` pre-release (never "latest", so the app updater is unaffected) with a `manifest.json`; `runtimes::own_builds` reads it. `LAMPRABIT_BINARIES_URL` points the app at another manifest (local testing).
- **MySQL** (Oracle) runs next to MariaDB on port 3307 (`ports.mysqlServer`; `ports.mysql` is MariaDB's, the name predates MySQL). Versions come from docker-library's `versions.json`, archives from cdn.mysql.com (Linux: the `-minimal` tarball), checked by MD5. Each release line has its own data folder (`data/mysql-8.4`…) because MySQL can't downgrade. `mysql_native_password=ON` is written only for 8.x (9.0 removed it). On Linux, `configs::mysql_libraries` links `libaio.so.1t64` to `libaio.so.1` (Ubuntu 24.04+) and otherwise names the packages to install.
- **Ports below 1024 on Linux**: the web server start asks once (pkexec) to write `/etc/sysctl.d/50-lamprabit.conf` lowering `ip_unprivileged_port_start`. `port_free` treats EACCES as free.
- **Single instance**: `tauri-plugin-single-instance` is registered first; a second launch shows the existing window.
- **Composer** is a normal runtime (`bin/composer/<version>/composer.phar`); presets install the newest one if none is active.
- **Default database** (`Settings.default_database`, MariaDB or MySQL; falls back to whichever is installed): drives the `mysql`/`mysqldump`/`mysqladmin` shims (`mariadb*` always mean MariaDB), new projects' databases and `.env`, and the order of servers in phpMyAdmin (`?server=N`) and Adminer (a server picker via `loginFormField`). Tools connect through `etc/<db>/client.cnf` (host, port, user), which exists before the server first starts.
- **Real Redis everywhere** (the user's choice, 2026-10-29, replacing Garnet): Windows uses the community redis-windows MSYS2 build (`redis-server.exe` + `redis-cli.exe` + msys DLLs, GitHub SHA-256 checked, newest patch of the last four lines); Linux and macOS use lampRabit's builds (`build/linux/build-redis.sh`, core only, no TLS). The `redis-cli` shim runs the active Redis's client. 0.1.x installs had Garnet (with `bin/dotnet`, `bin/rediscli`): `runtimes::remove_garnet` deletes them on start and the user is told to reinstall Redis. A custom redis-cli and an in-app Redis browser were tried and removed: don't bring them back.
- **Memcached is a runtime** (2026-10-29): lampRabit's builds on Linux and macOS (libevent compiled in); on Windows nono303's Cygwin build (`libevent-2.1/x64/memcached.exe`), repackaged by the CI with a pinned checksum, so all three come from the `binaries` manifest. The PHP `memcached` extension is in the Windows extension manager (PECL builds include libmemcached's DLL).
- **phpRedisAdmin** is a tool runtime like phpMyAdmin (`phpredisadmin.test`, `localhost/phpredisadmin/`). GitHub ships it as source only: after install, `runtimes::add_predis` fetches the Predis version its composer.json names and writes `vendor/autoload.php` (no Composer or PHP needed). `configs::write_phpredisadmin` writes its config on every web server start with `allow_no_auth` (local only).
- **Product roadmap** (the user's choices, 2026-10-29): phases 10+ in PLAN.md, in order (macOS parity first, all services before CLI/MCP and Git). No in-app database browser for now (Adminer/phpMyAdmin/phpRedisAdmin stay); uninstall keeps only `www/` and `data/`; the tab bar is movable but there are no dockable panels; FrankenPHP and Caddy are back.
- **Stack profiles**: `Settings.profiles` (name + services). The active profile is a per-device view preference (localStorage). `apply_services(start, stop)` backs profiles, selection actions and Start/Stop all.

## Conventions

- Comments explain *why*, in plain sentences; match the density of the surrounding code. Error messages tell the user what to do next.
- UI colors come from the theme tokens in `src/index.css` (`bg-surface`, `text-muted`, `accent`…), never fixed hex, so dark mode works. Check both themes.
- **Keyboard**: every shortcut is listed in `SHORTCUTS` in `src/lib/shortcuts.ts` (shown by the `?` dialog); handlers use `useHotkeys`, which pauses while typing or while a dialog or menu is open. Add new shortcuts to both.
- Runtime cards on Services use the shared pieces in `components/versions.tsx` (in-use tile, installed tiles, download list); Node.js uses them too.
- Dialogs use `components/Dialog.tsx` (focus in, Tab trapped, Escape closes, focus restored). Mark the field to focus with `data-autofocus`; mount stateful dialogs only while open.
- OS-specific behaviour sits behind `cfg!`/`#[cfg]`; where a feature can't work on a platform, the UI says why instead of hiding it.
- Version bumps touch `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` together.

## Checks

- `npx tsc --noEmit -p .` and `npm run build` (frontend); `cargo check` and `cargo test --lib` in `src-tauri/` (backend). CI runs these on Windows, macOS and Linux.
- **Linux testing on the maintainer's Windows machine** uses the WSL distro `Ubuntu-24.04` (user `dev`, passwordless sudo) with Rust, Node and the Tauri build libraries installed. Build with `npx tauri build --bundles appimage` there, run the AppImage under `Xvfb :99` with `GDK_BACKEND=x11`, drive it with `xdotool` and capture screenshots with `import -window root`. Build the runtimes with `build/linux/build-*.sh`, then serve `out/` with a manifest and set `LAMPRABIT_BINARIES_URL` (steps in RELEASING.md).

## Pitfalls already hit

- `sysinfo` lists every thread as a process on Linux; filter with `thread_kind().is_none()` or memory is counted many times over.
- `httpd` and `nginx` daemonize by default outside Windows; they're started with `-D FOREGROUND` / `daemon off;` so the supervisor tracks the real process.
- The AppImage can't bundle GL libraries; `main.rs` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Linux, or WebKitGTK aborts without `libGLESv2` (and draws blank windows on some GPUs).
- GNOME's terminals don't inherit the spawning process's environment; the terminal launcher passes `PATH` through `env` in the command.
- A full-width fixed wrapper (like the bottom nav's) swallows clicks; give it `pointer-events-none` and the visible part `pointer-events-auto`.
- The redis-windows build is MSYS2-based: pass paths with forward slashes (`--dir C:/…`); it accepts them, spaces included.
- Adminer 6 rejects a login post without its CSRF `token` field; scripted tests must send it (browsers do).
- Settings inputs: `inputClass` contains `w-full`; strip it for narrow fields (`numberInput`), or the width class loses.
- Pass callbacks that change every render (like `onClose`) into effects through a ref, or the effect re-runs, and the Overview re-renders every second.
