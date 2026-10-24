import { create } from "zustand";
import type { InstallProgress, LogHistory, RawLogLine } from "./api";
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
  diagnosticsOpen: boolean;
  setDiagnosticsOpen: (open: boolean) => void;
  newProjectOpen: boolean;
  setNewProjectOpen: (open: boolean) => void;
  logs: LogEntry[];
  notices: Notice[];
  activity: Activity[];
  /** In-flight installs keyed by installKey(). */
  installs: Record<string, InstallProgress>;
  setInstall: (p: InstallProgress) => void;
  pushLog: (line: RawLogLine) => void;
  /** Replaces the view with history read from the log files on disk. */
  loadHistory: (history: LogHistory) => void;
  clearLogs: () => void;
  record: (text: string, source: string, tone: Activity["tone"]) => void;
  notify: (tone: Notice["tone"], text: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;
/** Live lines at or below this id are already part of the loaded history. */
let logCursor = 0;

export const useAppStore = create<AppStore>((set) => ({
  page: "overview",
  navigate: (page) => set({ page }),
  diagnosticsOpen: false,
  setDiagnosticsOpen: (diagnosticsOpen) => set({ diagnosticsOpen }),
  newProjectOpen: false,
  setNewProjectOpen: (newProjectOpen) => set({ newProjectOpen }),
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
  pushLog: (line) =>
    set((s) => (line.id <= logCursor ? s : { logs: [...s.logs.slice(-(MAX_LOG_LINES - 1)), parseLog(line)] })),
  loadHistory: ({ lines, cursor }) =>
    set((s) => {
      logCursor = cursor;
      // History ids are 0; give them unique negative ids so they never collide with live ones.
      // Lines without their own timestamp (stack traces, MariaDB's "Version:" line) take the
      // time of the line before them in the same file.
      const lastTime: Record<string, Date> = {};
      const history = lines.map((l, i) => {
        const entry = parseLog({ ...l, id: -(i + 1) });
        const file = `${l.service}/${l.stream}`;
        if (entry.timed) lastTime[file] = entry.time;
        else if (lastTime[file]) entry.time = lastTime[file];
        return entry;
      });
      const live = s.logs.filter((l) => l.id > cursor);
      const merged = [...history, ...live].sort((a, b) => a.time.getTime() - b.time.getTime());
      return { logs: merged.slice(-MAX_LOG_LINES) };
    }),
  clearLogs: () => set({ logs: [] }),
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
