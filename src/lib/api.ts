import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "apache" | "mariadb" | "php-fpm" | "mailpit";
export type ServiceState = "stopped" | "starting" | "running" | "stopping" | "error";
export type Runtime = "apache" | "php" | "mariadb" | "mailpit" | "adminer" | "phpmyadmin";
export const coreRuntimes: Runtime[] = ["apache", "php", "mariadb"];
export const toolRuntimes: Runtime[] = ["mailpit", "adminer", "phpmyadmin"];

export const serviceLabels: Record<ServiceKind, string> = {
  apache: "Apache",
  mariadb: "MariaDB",
  "php-fpm": "PHP-FPM",
  mailpit: "Mailpit",
};

export const runtimeLabels: Record<Runtime, string> = {
  apache: "Apache",
  php: "PHP",
  mariadb: "MariaDB",
  mailpit: "Mailpit",
  adminer: "Adminer",
  phpmyadmin: "phpMyAdmin",
};

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
  domain: string;
  path: string;
  /** Directory Apache serves: the project's public/ folder when it has one. */
  docroot: string;
  ssl: boolean;
  url: string;
  modified: number;
  /** Set for built-in tools rather than www/ folders. */
  tool: "adminer" | "phpmyadmin" | "mailpit" | null;
  proxy: string | null;
}

export interface SitesInfo {
  sites: Site[];
  tools: Site[];
  tld: string;
  needsApply: boolean;
  hostsOk: boolean;
  manageHosts: boolean;
  /** null while no site uses HTTPS. */
  caTrusted: boolean | null;
  caPath: string;
}

export interface LogHistory {
  lines: RawLogLine[];
  cursor: number;
}

export interface StatusEvent {
  kind: ServiceKind;
  state: ServiceState;
  message: string | null;
}

export type LogService = "apache" | "php" | "mariadb" | "mailpit" | "system";

/** A raw log line from a service's output or one of its log files. */
export interface RawLogLine {
  id: number;
  service: LogService;
  stream: "output" | "error" | "access";
  line: string;
}

export interface Settings {
  rootDir: string;
  tld: string;
  ports: { http: number; https: number; mysql: number; phpFpm: number; mailSmtp: number; mailUi: number };
  versions: Record<Runtime, string | null>;
  httpsByDefault: boolean;
  manageHosts: boolean;
  routeMail: boolean;
  sites: Record<string, { ssl: boolean | null }>;
}

export interface EnvPaths {
  root: string;
  www: string;
  logs: string;
  settingsFile: string;
  os: "windows" | "macos" | "linux";
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  envPaths: () => invoke<EnvPaths>("env_paths"),
  listServices: () => invoke<ServiceInfo[]>("list_services"),
  startService: (kind: ServiceKind) => invoke<void>("start_service", { kind }),
  stopService: (kind: ServiceKind) => invoke<void>("stop_service", { kind }),
  restartService: (kind: ServiceKind) => invoke<void>("restart_service", { kind }),
  listSites: () => invoke<SitesInfo>("list_sites"),
  applySites: () => invoke<void>("apply_sites"),
  setSiteSsl: (name: string, ssl: boolean) => invoke<void>("set_site_ssl", { name, ssl }),
  trustCa: () => invoke<void>("trust_ca"),
  openDbClient: () => invoke<void>("open_db_client"),
  startAll: () => invoke<string[]>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  checkPort: (port: number) => invoke<boolean>("check_port", { port }),
  logHistory: () => invoke<LogHistory>("log_history"),
  clearLogs: () => invoke<void>("clear_logs"),
  runtimeCatalog: (runtime: Runtime) => invoke<Release[]>("runtime_catalog", { runtime }),
  installedRuntimes: () => invoke<InstalledRuntime[]>("installed_runtimes"),
  installRuntime: (release: Release) => invoke<void>("install_runtime", { release }),
  removeRuntime: (runtime: Runtime, version: string) => invoke<void>("remove_runtime", { runtime, version }),
  setActiveVersion: (runtime: Runtime, version: string) =>
    invoke<void>("set_active_version", { runtime, version }),
};
