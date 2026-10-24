import { create } from "zustand";
import type { InstallProgress, RawLogLine } from "./api";
import { parseLog, type LogEntry } from "./logparse";

const MAX_LOG_LINES = 2000;
const MAX_ACTIVITY = 100;

export type PageId = "overview" | "sites" | "services" | "logs" | "settings";

export interface Notice {
  id: number;
  tone: "error" | "info";
  text: string;
}

export interface Activity {
  id: number;
  at: Date;
  text: string;
  source: string;
  tone: "ok" | "info" | "muted" | "error";
}

export const installKey = (runtime: string, version: string) => `${runtime}@${version}`;

interface AppStore {
  page: PageId;
  navigate: (page: PageId) => void;
  logs: LogEntry[];
  notices: Notice[];
  activity: Activity[];
  /** In-flight installs keyed by installKey(). */
  installs: Record<string, InstallProgress>;
  setInstall: (p: InstallProgress) => void;
  pushLog: (line: RawLogLine) => void;
  /** Merges history fetched from the backend, skipping lines already received live. */
  mergeLogs: (lines: RawLogLine[]) => void;
  clearLogs: () => void;
  record: (text: string, source: string, tone: Activity["tone"]) => void;
  notify: (tone: Notice["tone"], text: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;
/** Log lines with an id at or below this were cleared by the user. */
let clearedThrough = 0;

export const useAppStore = create<AppStore>((set) => ({
  page: "overview",
  navigate: (page) => set({ page }),
  logs: [],
  notices: [],
  activity: [{ id: nextId++, at: new Date(), text: "Environment ready", source: "System", tone: "info" }],
  installs: {},
  setInstall: (p) =>
    set((s) => {
      const installs = { ...s.installs };
      const key = installKey(p.runtime, p.version);
      if (p.stage === "done" || p.stage === "error") delete installs[key];
      else installs[key] = p;
      return { installs };
    }),
  pushLog: (line) => set((s) => ({ logs: [...s.logs.slice(-(MAX_LOG_LINES - 1)), parseLog(line)] })),
  mergeLogs: (lines) =>
    set((s) => {
      const seen = new Set(s.logs.map((l) => l.id));
      const fresh = lines.filter((l) => l.id > clearedThrough && !seen.has(l.id)).map((l) => parseLog(l));
      // History arrives grouped by file; order everything by the time in the line.
      const merged = [...fresh, ...s.logs].sort((a, b) => a.time.getTime() - b.time.getTime());
      return { logs: merged.slice(-MAX_LOG_LINES) };
    }),
  clearLogs: () =>
    set((s) => {
      clearedThrough = Math.max(clearedThrough, ...s.logs.map((l) => l.id));
      return { logs: [] };
    }),
  record: (text, source, tone) =>
    set((s) => ({
      activity: [{ id: nextId++, at: new Date(), text, source, tone }, ...s.activity].slice(0, MAX_ACTIVITY),
    })),
  notify: (tone, text) => {
    const id = nextId++;
    set((s) => ({ notices: [...s.notices, { id, tone, text }] }));
    setTimeout(() => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })), 6000);
  },
  dismiss: (id) => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })),
}));
