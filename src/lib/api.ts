import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "apache" | "mariadb" | "php-fpm";
export type ServiceState = "stopped" | "starting" | "running" | "stopping" | "error";
export type Runtime = "apache" | "php" | "mariadb";

export const serviceLabels: Record<ServiceKind, string> = {
  apache: "Apache",
  mariadb: "MariaDB",
  "php-fpm": "PHP-FPM",
};

export const runtimeLabels: Record<Runtime, string> = { apache: "Apache", php: "PHP", mariadb: "MariaDB" };

export interface Release {
  runtime: Runtime;
  version: string;
  url: string;
  sha256: string | null;
  archive: "zip" | "targz";
  label: string | null;
}

export interface InstalledRuntime {
  runtime: Runtime;
  active: string | null;
  versions: string[];
}

export interface InstallProgress {
  runtime: Runtime;
  version: string;
  stage: "downloading" | "verifying" | "extracting" | "done" | "error";
  downloaded: number;
  total: number | null;
}

export interface ServiceInfo {
  kind: ServiceKind;
  label: string;
  state: ServiceState;
  pid: number | null;
  port: number;
  version: string | null;
  installed: boolean;
  startedAt: number | null;
  configPath: string;
  logPath: string;
}

export interface Site {
  name: string;
  path: string;
  url: string;
  modified: number;
}

export interface StatusEvent {
  kind: ServiceKind;
  state: ServiceState;
  message: string | null;
}

export interface LogEvent {
  kind: ServiceKind;
  line: string;
}

export interface Settings {
  rootDir: string;
  tld: string;
  ports: { http: number; https: number; mysql: number; phpFpm: number };
  versions: { apache: string | null; php: string | null; mariadb: string | null };
}

export interface EnvPaths {
  root: string;
  www: string;
  logs: string;
  settingsFile: string;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  envPaths: () => invoke<EnvPaths>("env_paths"),
  listServices: () => invoke<ServiceInfo[]>("list_services"),
  startService: (kind: ServiceKind) => invoke<void>("start_service", { kind }),
  stopService: (kind: ServiceKind) => invoke<void>("stop_service", { kind }),
  restartService: (kind: ServiceKind) => invoke<void>("restart_service", { kind }),
  listSites: () => invoke<Site[]>("list_sites"),
  openDbClient: () => invoke<void>("open_db_client"),
  startAll: () => invoke<string[]>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  checkPort: (port: number) => invoke<boolean>("check_port", { port }),
  runtimeCatalog: (runtime: Runtime) => invoke<Release[]>("runtime_catalog", { runtime }),
  installedRuntimes: () => invoke<InstalledRuntime[]>("installed_runtimes"),
  installRuntime: (release: Release) => invoke<void>("install_runtime", { release }),
  removeRuntime: (runtime: Runtime, version: string) => invoke<void>("remove_runtime", { runtime, version }),
  setActiveVersion: (runtime: Runtime, version: string) =>
    invoke<void>("set_active_version", { runtime, version }),
};
