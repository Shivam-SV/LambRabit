import { create } from "zustand";
import type { InstallProgress, ServiceKind } from "./api";

const MAX_LOG_LINES = 2000;

export interface LogLine {
  id: number;
  kind: ServiceKind;
  line: string;
  at: Date;
}

export interface Notice {
  id: number;
  tone: "error" | "info";
  text: string;
}

export const installKey = (runtime: string, version: string) => `${runtime}@${version}`;

interface AppStore {
  logs: LogLine[];
  notices: Notice[];
  /** In-flight installs keyed by installKey(). */
  installs: Record<string, InstallProgress>;
  setInstall: (p: InstallProgress) => void;
  pushLog: (kind: ServiceKind, line: string) => void;
  clearLogs: () => void;
  notify: (tone: Notice["tone"], text: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;

export const useAppStore = create<AppStore>((set) => ({
  logs: [],
  notices: [],
  installs: {},
  setInstall: (p) =>
    set((s) => {
      const installs = { ...s.installs };
      const key = installKey(p.runtime, p.version);
      if (p.stage === "done" || p.stage === "error") delete installs[key];
      else installs[key] = p;
      return { installs };
    }),
  pushLog: (kind, line) =>
    set((s) => ({
      logs: [...s.logs.slice(-(MAX_LOG_LINES - 1)), { id: nextId++, kind, line, at: new Date() }],
    })),
  clearLogs: () => set({ logs: [] }),
  notify: (tone, text) => {
    const id = nextId++;
    set((s) => ({ notices: [...s.notices, { id, tone, text }] }));
    setTimeout(() => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })), 6000);
  },
  dismiss: (id) => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })),
}));
