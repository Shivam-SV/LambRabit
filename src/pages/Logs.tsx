import { useEffect, useMemo, useRef, useState } from "react";
import { Pause, Play, Search, Trash2 } from "lucide-react";
import clsx from "clsx";
import type { LogService } from "../lib/api";
import type { Level, LogEntry } from "../lib/logparse";
import { useAppStore } from "../lib/store";
import { Button, PageHeader, Switch } from "../components/ui";

const services: { id: LogService | "all"; label: string }[] = [
  { id: "all", label: "All services" },
  { id: "apache", label: "Apache" },
  { id: "php", label: "PHP" },
  { id: "mariadb", label: "MariaDB" },
];

const badge: Record<LogService, { label: string; className: string }> = {
  apache: { label: "Apache", className: "bg-rose-500/20 text-rose-300" },
  php: { label: "PHP", className: "bg-indigo-500/25 text-indigo-300" },
  mariadb: { label: "MariaDB", className: "bg-amber-500/20 text-amber-300" },
};

const levels: Record<Level, { dot: string; text: string }> = {
  error: { dot: "bg-red-400", text: "text-red-400" },
  warn: { dot: "bg-amber-400", text: "text-amber-300" },
  notice: { dot: "bg-yellow-300", text: "text-yellow-200" },
  info: { dot: "bg-emerald-400", text: "text-emerald-300" },
  access: { dot: "bg-sky-400", text: "text-sky-300" },
  debug: { dot: "bg-slate-500", text: "text-slate-400" },
};

const pad = (n: number) => String(n).padStart(2, "0");
const stamp = (d: Date) =>
  `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;

export default function Logs() {
  const logs = useAppStore((s) => s.logs);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const [service, setService] = useState<LogService | "all">("all");
  const [query, setQuery] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  /** When paused, the view is frozen at this many lines. */
  const [frozen, setFrozen] = useState<LogEntry[] | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  const source = frozen ?? logs;
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return source.filter(
      (l) => (service === "all" || l.service === service) && (!q || l.message.toLowerCase().includes(q)),
    );
  }, [source, service, query]);
  const newWhilePaused = frozen ? Math.max(0, logs.length - frozen.length) : 0;

  useEffect(() => {
    if (autoScroll && !frozen) scroller.current?.scrollTo({ top: scroller.current.scrollHeight });
  }, [visible.length, autoScroll, frozen]);

  return (
    <>
      <PageHeader
        title="Logs"
        subtitle="Live output from your local services."
        actions={
          <>
            <select
              value={service}
              onChange={(e) => setService(e.target.value as LogService | "all")}
              aria-label="Filter by service"
              className="h-10 rounded-xl border border-line bg-surface px-3 text-sm shadow-card"
            >
              {services.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
            <label className="flex h-10 w-60 items-center gap-2 rounded-xl border border-line bg-surface px-3 shadow-card focus-within:border-accent">
              <Search size={16} className="text-muted" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search logs…"
                className="w-full bg-transparent text-sm outline-none placeholder:text-muted"
              />
            </label>
            <div className="flex rounded-xl border border-line bg-surface p-1 shadow-card">
              <button
                onClick={() => setFrozen(null)}
                className={clsx(
                  "flex items-center gap-1.5 rounded-lg px-3 py-1 text-sm font-medium",
                  !frozen ? "bg-accent text-accent-fg" : "text-muted hover:text-fg",
                )}
              >
                <Play size={14} /> Live
              </button>
              <button
                onClick={() => setFrozen((f) => f ?? logs)}
                className={clsx(
                  "flex items-center gap-1.5 rounded-lg px-3 py-1 text-sm font-medium",
                  frozen ? "bg-warn text-white" : "text-muted hover:text-fg",
                )}
              >
                <Pause size={14} /> Pause
              </button>
            </div>
            <Button
              className="h-10 rounded-xl"
              onClick={() => {
                clearLogs();
                setFrozen(null);
              }}
            >
              <Trash2 size={15} /> Clear logs
            </Button>
          </>
        }
      />

      <div className="overflow-hidden rounded-2xl border border-white/5 bg-[#10151c] text-slate-200 shadow-float">
        <div ref={scroller} className="selectable h-[calc(100vh-21rem)] min-h-64 overflow-auto px-5 py-4 font-mono text-[13px] leading-7">
          {visible.length === 0 ? (
            <div className="flex h-full items-center justify-center font-sans text-sm text-slate-500">
              {logs.length === 0 ? "No log output yet. Start a service to see its logs here." : "No lines match your filter."}
            </div>
          ) : (
            <table className="w-full border-separate border-spacing-0">
              <tbody>
                {visible.map((l) => (
                  <tr key={l.id} className="align-top hover:bg-white/[0.03]">
                    <td className="pr-4 whitespace-nowrap text-slate-500">[{stamp(l.time)}]</td>
                    <td className="pr-4">
                      <span className={clsx("rounded-md px-2 py-0.5 text-xs", badge[l.service].className)}>
                        {badge[l.service].label}
                      </span>
                    </td>
                    <td className={clsx("pr-3 whitespace-nowrap", levels[l.level].text)}>
                      <span className={clsx("mr-2 inline-block size-1.5 rounded-full align-middle", levels[l.level].dot)} />
                      {l.level}
                    </td>
                    <td className="pr-3 text-slate-600">|</td>
                    <td className="w-full break-all whitespace-pre-wrap">{l.message}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t border-white/10 px-5 py-3 text-sm text-slate-400">
          <span className={clsx("size-2.5 rounded-full", frozen ? "bg-amber-400" : "animate-pulse bg-emerald-400")} />
          <span className={clsx("font-semibold", frozen ? "text-amber-300" : "text-emerald-300")}>
            {frozen ? "Paused" : "Live"}
          </span>
          <span>·</span>
          <span>
            {frozen
              ? `${newWhilePaused} new ${newWhilePaused === 1 ? "line" : "lines"} since pausing`
              : "Waiting for service output"}
          </span>
          <span className="flex-1" />
          <span>
            {visible.length} {visible.length === 1 ? "line" : "lines"}
          </span>
          <span className="text-slate-600">|</span>
          <span>Auto-scroll {autoScroll ? "on" : "off"}</span>
          <Switch checked={autoScroll} onChange={setAutoScroll} label="Auto-scroll" />
        </div>
      </div>
    </>
  );
}
