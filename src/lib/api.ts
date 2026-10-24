import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "apache" | "mariadb" | "php-fpm";
export type ServiceState = "stopped" | "running" | "stopping" | "error";

export interface ServiceInfo {
  kind: ServiceKind;
  label: string;
  state: ServiceState;
  pid: number | null;
  port: number;
  version: string | null;
  installed: boolean;
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
  startAll: () => invoke<string[]>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  checkPort: (port: number) => invoke<boolean>("check_port", { port }),
};
