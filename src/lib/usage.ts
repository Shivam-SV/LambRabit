import { useEffect } from "react";
import { create } from "zustand";
import { api, type Usage } from "./api";

/** One reading every 2 seconds; 30 of them is the last minute. */
const INTERVAL_MS = 2000;
const SAMPLES = 30;

interface Series {
  cpu: number[];
  memory: number[];
}

interface UsageStore {
  series: Record<string, Series>;
  latest: Record<string, Usage>;
  push: (readings: Usage[]) => void;
}

export const useUsage = create<UsageStore>((set) => ({
  series: {},
  latest: {},
  push: (readings) =>
    set((s) => {
      const series: Record<string, Series> = {};
      const latest: Record<string, Usage> = {};
      // Services that stopped drop out, so a restart starts a fresh line.
      for (const r of readings) {
        const prev = s.series[r.id] ?? { cpu: [], memory: [] };
        series[r.id] = { cpu: [...prev.cpu, r.cpu].slice(-SAMPLES), memory: [...prev.memory, r.memory].slice(-SAMPLES) };
        latest[r.id] = r;
      }
      return { series, latest };
    }),
}));

/** Polls CPU and memory while the window is visible and something is running. */
export function useUsageSampler(active: boolean) {
  useEffect(() => {
    if (!active) {
      useUsage.getState().push([]);
      return;
    }
    let cancelled = false;
    const tick = () => {
      if (document.hidden) return;
      api
        .serviceUsage()
        .then((r) => !cancelled && useUsage.getState().push(r))
        .catch(() => {});
    };
    tick();
    const timer = setInterval(tick, INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [active]);
}

export function formatBytes(bytes: number) {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

export const formatCpu = (cpu: number) => (cpu < 10 ? `${cpu.toFixed(1)}%` : `${Math.round(cpu)}%`);
