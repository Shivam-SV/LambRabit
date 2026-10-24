import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "apache" | "nginx" | "mariadb" | "mailpit";
export type WebServer = "apache" | "nginx" | "both";
export type Server = "apache" | "nginx";
export type ServiceState = "stopped" | "starting" | "running" | "stopping" | "error";
export type Runtime =
  | "apache"
  | "nginx"
  | "php"
  | "mariadb"
  | "mailpit"
  | "adminer"
  | "phpmyadmin"
  | "ngrok"
  | "cloudflared";
export const coreRuntimes: Runtime[] = ["apache", "nginx", "php", "mariadb"];
export const toolRuntimes: Runtime[] = ["mailpit", "adminer", "phpmyadmin", "ngrok", "cloudflared"];

export const serviceLabels: Record<ServiceKind, string> = {
  apache: "Apache",
  nginx: "Nginx",
  mariadb: "MariaDB",
  mailpit: "Mailpit",
};

export const runtimeLabels: Record<Runtime, string> = {
  apache: "Apache",
  nginx: "Nginx",
  php: "PHP",
  mariadb: "MariaDB",
  mailpit: "Mailpit",
  adminer: "Adminer",
  phpmyadmin: "phpMyAdmin",
  ngrok: "Ngrok",
  cloudflared: "Cloudflare Tunnel",
};

export type TunnelProvider = "ngrok" | "cloudflare";

export interface Tunnel {
  id: string;
  provider: TunnelProvider;
  site: string | null;
  target: string;
  url: string | null;
  status: "starting" | "online" | "error";
  error: string | null;
}

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
  health: Health | null;
  problem: Problem | null;
}

export interface Health {
  status: "checking" | "healthy" | "degraded";
  detail: string | null;
  checkedAt: number;
}

export interface PortOwner {
  pid: number;
  name: string;
  note: string | null;
  stoppable: boolean;
}

export interface Problem {
  title: string;
  detail: string;
  fixes: string[];
  port: number | null;
  owner: PortOwner | null;
  at: number;
}

export interface PortStatus {
  key: keyof Settings["ports"];
  port: number;
  free: boolean;
  usedByService: string | null;
  owner: PortOwner | null;
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
  /** PHP version chosen for the site; null follows the active version. */
  php: string | null;
  server: Server;
}

export type Preset = "blank" | "static" | "laravel" | "symfony" | "wordpress";

export interface NewProject {
  name: string;
  preset: Preset;
  php: string | null;
  server: Server | null;
  ssl: boolean;
  database: string | null;
}

export interface ProjectProgress {
  name: string;
  step: string | null;
  line: string | null;
}

export interface PoolStatus {
  version: string;
  port: number;
  workers: number;
  alive: number;
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

export type LogService = "apache" | "nginx" | "php" | "mariadb" | "mailpit" | "system";

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
  ports: {
    http: number;
    https: number;
    mysql: number;
    phpFpm: number;
    nginxHttp: number;
    nginxHttps: number;
    mailSmtp: number;
    mailUi: number;
  };
  versions: Record<Runtime, string | null>;
  webServer: WebServer;
  phpWorkers: number;
  httpsByDefault: boolean;
  manageHosts: boolean;
  routeMail: boolean;
  sites: Record<string, { ssl: boolean | null; php: string | null; server: Server | null }>;
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
  setSitePhp: (name: string, version: string | null) => invoke<void>("set_site_php", { name, version }),
  setSiteServer: (name: string, server: Server | null) => invoke<void>("set_site_server", { name, server }),
  phpStatus: () => invoke<PoolStatus[]>("php_status"),
  createProject: (request: NewProject) => invoke<string>("create_project", { request }),
  tunnelStart: (provider: TunnelProvider, site: string | null, port: number | null) =>
    invoke<Tunnel>("tunnel_start", { provider, site, port }),
  tunnelStop: (id: string) => invoke<void>("tunnel_stop", { id }),
  tunnelList: () => invoke<Tunnel[]>("tunnel_list"),
  ngrokTokenSaved: () => invoke<boolean>("ngrok_token_saved"),
  setNgrokToken: (token: string | null) => invoke<void>("set_ngrok_token", { token }),
  trustCa: () => invoke<void>("trust_ca"),
  openDbClient: () => invoke<void>("open_db_client"),
  startAll: () => invoke<string[]>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  checkPort: (port: number) => invoke<boolean>("check_port", { port }),
  portStatus: () => invoke<PortStatus[]>("port_status"),
  stopProcess: (pid: number) => invoke<void>("stop_process", { pid }),
  clearProblem: (kind: ServiceKind) => invoke<void>("clear_problem", { kind }),
  diagnostics: (redact: boolean) => invoke<string>("diagnostics", { redact }),
  saveText: (path: string, contents: string) => invoke<void>("save_text", { path, contents }),
  logHistory: () => invoke<LogHistory>("log_history"),
  clearLogs: () => invoke<void>("clear_logs"),
  runtimeCatalog: (runtime: Runtime) => invoke<Release[]>("runtime_catalog", { runtime }),
  installedRuntimes: () => invoke<InstalledRuntime[]>("installed_runtimes"),
  installRuntime: (release: Release) => invoke<void>("install_runtime", { release }),
  removeRuntime: (runtime: Runtime, version: string) => invoke<void>("remove_runtime", { runtime, version }),
  setActiveVersion: (runtime: Runtime, version: string) =>
    invoke<void>("set_active_version", { runtime, version }),
};
