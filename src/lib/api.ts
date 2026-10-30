import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "apache" | "nginx" | "mariadb" | "mailpit" | "redis" | "memcached" | "mysql";
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
  | "cloudflared"
  | "redis"
  | "composer"
  | "mysql"
  | "phpredisadmin"
  | "memcached";
export const coreRuntimes: Runtime[] = ["apache", "nginx", "php", "mariadb", "mysql"];
export const toolRuntimes: Runtime[] = ["composer", "mailpit", "adminer", "phpmyadmin", "redis", "phpredisadmin", "memcached", "ngrok", "cloudflared"];

export const serviceLabels: Record<ServiceKind, string> = {
  apache: "Apache",
  nginx: "Nginx",
  mariadb: "MariaDB",
  mysql: "MySQL",
  mailpit: "Mailpit",
  redis: "Redis",
  memcached: "Memcached",
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
  redis: "Redis",
  composer: "Composer",
  mysql: "MySQL",
  phpredisadmin: "phpRedisAdmin",
  memcached: "Memcached",
};

export interface UpdateNotice {
  runtime: Runtime;
  installed: string;
  release: Release;
}

export interface ExtInfo {
  name: string;
  kind: "bundled" | "extra";
  description: string | null;
  enabled: boolean;
  installed: boolean;
  available: string | null;
  note: string | null;
}

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
  tool: "adminer" | "phpmyadmin" | "phpredisadmin" | "mailpit" | null;
  proxy: string | null;
  /** PHP version chosen for the site; null follows the active version. */
  php: string | null;
  server: Server;
  /** Node version from the project's .nvmrc / .node-version. */
  node: string | null;
  /** "Laravel", "WordPress", "Symfony", "PHP", "Static"…, from the project's files. */
  framework: string | null;
  /** The checked-out git branch (or short commit). */
  gitBranch: string | null;
  /** The database the project's config names. */
  database: string | null;
}

export interface NodeStatus {
  manager: "nvm-windows" | "nvm" | null;
  managerVersion: string | null;
  current: string | null;
  installed: string[];
  needsElevation: boolean;
}

export interface NodeRelease {
  version: string;
  major: number;
  lts: string | null;
}

export interface NewProject {
  name: string;
  /** A template id from listTemplates. */
  template: string;
  php: string | null;
  server: Server | null;
  ssl: boolean;
  database: string | null;
  /** Values for the template's variables, by id. */
  variables: Record<string, string>;
  /** Optional steps by id: run or not. */
  optional: Record<string, boolean>;
  /** The user accepted the risky steps of a template that didn't ship with lampRabit. */
  approveRisky: boolean;
}

export interface TemplateVariable {
  id: string;
  label: string;
  help: string | null;
  type: "text" | "choice" | "bool";
  options: string[];
  default: string | null;
}

export interface ProjectTemplate {
  id: string;
  name: string;
  description: string;
  version: number;
  icon: string | null;
  color: string | null;
  duration: string | null;
  source: "builtin" | "user";
  path: string | null;
  usesPhp: boolean;
  minPhp: string | null;
  extensions: string[];
  database: "none" | "optional" | "required";
  variables: TemplateVariable[];
  optionalSteps: { id: string; label: string; default: boolean }[];
  /** Steps needing approval (templates you added yourself). */
  risks: { step: string; heading: string; command: string; reasons: string[] }[];
  done: string | null;
}

export interface TemplateCatalog {
  templates: ProjectTemplate[];
  broken: { file: string; error: string }[];
  folder: string;
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
  /** Folders in www that can't be served, and why. */
  unserved: { name: string; reason: string }[];
  /** Tool domains (adminer.test…) that a folder of the same name has taken. */
  shadowedTools: string[];
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

export type LogService = "apache" | "nginx" | "php" | "mariadb" | "mysql" | "mailpit" | "redis" | "memcached" | "system";

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
    /** MariaDB's port (the name predates MySQL support). */
    mysql: number;
    /** Oracle MySQL, next to MariaDB. */
    mysqlServer: number;
    phpFpm: number;
    nginxHttp: number;
    nginxHttps: number;
    mailSmtp: number;
    mailUi: number;
    redis: number;
    memcached: number;
  };
  versions: Record<Runtime, string | null>;
  webServer: WebServer;
  phpWorkers: number;
  httpsByDefault: boolean;
  manageHosts: boolean;
  routeMail: boolean;
  checkUpdates: boolean;
  closeToTray: boolean;
  addToPath: boolean;
  startServicesOnLaunch: boolean;
  editor: string;
  sites: Record<string, { ssl: boolean | null; php: string | null; server: Server | null }>;
  profiles: StackProfile[];
  /** What `mysql`, new projects and the database tools use first. */
  defaultDatabase: Database;
}

export type Database = "mariadb" | "mysql";

/** A named set of services started and stopped together. */
export interface StackProfile {
  name: string;
  services: ServiceKind[];
}

/** CPU (like `top`: 100 is one full core) and memory (bytes) of one service, or "php". */
export interface Usage {
  id: ServiceKind | "php";
  cpu: number;
  memory: number;
  processes: number;
}

/** Removing lampRabit: folders deleted and kept (Settings → Troubleshooting). */
export interface UninstallPlan {
  remove: string[];
  keep: string[];
}

export interface UninstallReport {
  kept: string[];
  /** What couldn't be undone, each saying what to do. */
  problems: string[];
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
  takeStartupWarnings: () => invoke<string[]>("take_startup_warnings"),
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
  createProject: (request: NewProject) => invoke<{ url: string; note: string | null }>("create_project", { request }),
  listTemplates: () => invoke<TemplateCatalog>("list_templates"),
  openTemplatesFolder: () => invoke<void>("open_templates_folder"),
  tunnelStart: (provider: TunnelProvider, site: string | null, port: number | null) =>
    invoke<Tunnel>("tunnel_start", { provider, site, port }),
  tunnelStop: (id: string) => invoke<void>("tunnel_stop", { id }),
  tunnelList: () => invoke<Tunnel[]>("tunnel_list"),
  ngrokTokenSaved: () => invoke<boolean>("ngrok_token_saved"),
  nodeStatus: () => invoke<NodeStatus>("node_status"),
  nodeAvailable: () => invoke<NodeRelease[]>("node_available"),
  nodeInstall: (version: string) => invoke<void>("node_install", { version }),
  nodeUse: (version: string) => invoke<void>("node_use", { version }),
  nodeUninstall: (version: string) => invoke<void>("node_uninstall", { version }),
  nvmInstall: () => invoke<void>("nvm_install"),
  runtimeUpdates: () => invoke<UpdateNotice[]>("runtime_updates"),
  phpExtensions: (version: string) => invoke<ExtInfo[]>("php_extensions", { version }),
  setPhpExtension: (version: string, name: string, enabled: boolean) =>
    invoke<void>("set_php_extension", { version, name, enabled }),
  installPhpExtension: (version: string, name: string) => invoke<void>("install_php_extension", { version, name }),
  openTerminal: (dir: string | null) => invoke<void>("open_terminal", { dir }),
  openInEditor: (dir: string) => invoke<void>("open_in_editor", { dir }),
  vcRuntimeStatus: () => invoke<{ applicable: boolean; installed: boolean; version: string | null }>("vc_runtime_status"),
  vcRuntimeInstall: () => invoke<void>("vc_runtime_install"),
  setNgrokToken: (token: string | null) => invoke<void>("set_ngrok_token", { token }),
  /** Resolves to a note when browsers couldn't be covered (Linux without certutil). */
  trustCa: () => invoke<string | null>("trust_ca"),
  openDbClient: (database?: Database) => invoke<void>("open_db_client", { database: database ?? null }),
  startAll: () => invoke<string[]>("start_all"),
  /** Stops `stop`, then starts `start`; resolves to one message per failed start. */
  applyServices: (start: ServiceKind[], stop: ServiceKind[]) => invoke<string[]>("apply_services", { start, stop }),
  serviceUsage: () => invoke<Usage[]>("service_usage"),
  stopAll: () => invoke<void>("stop_all"),
  uninstallPlan: () => invoke<UninstallPlan>("uninstall_plan"),
  uninstall: () => invoke<UninstallReport>("uninstall"),
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
