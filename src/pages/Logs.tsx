import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, Copy, Download, Pause, Play, Search, Trash2, WrapText, X } from "lucide-react";
import clsx from "clsx";
import { save } from "@tauri-apps/plugin-dialog";
import { api, type LogService } from "../lib/api";
import type { Level, LogEntry } from "../lib/logparse";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader, Switch } from "../components/ui";

const services: { id: LogService; label: string }[] = [
  { id: "apache", label: "Apache" },
  { id: "nginx", label: "Nginx" },
  { id: "php", label: "PHP" },
  { id: "mariadb", label: "MariaDB" },
  { id: "mysql", label: "MySQL" },
  { id: "mailpit", label: "Mailpit" },
  { id: "redis", label: "Redis" },
  { id: "memcached", label: "Memcached" },
  { id: "system", label: "System" },
];

const badge: Record<LogService, { label: string; className: string }> = {
  apache: { label: "Apache", className: "bg-rose-500/20 text-rose-300" },
  nginx: { label: "Nginx", className: "bg-emerald-500/20 text-emerald-300" },
  php: { label: "PHP", className: "bg-indigo-500/25 text-indigo-300" },
  mariadb: { label: "MariaDB", className: "bg-amber-500/20 text-amber-300" },
  mysql: { label: "MySQL", className: "bg-blue-500/20 text-blue-300" },
  mailpit: { label: "Mailpit", className: "bg-sky-500/20 text-sky-300" },
  redis: { label: "Redis", className: "bg-red-500/20 text-red-300" },
  memcached: { label: "Memcached", className: "bg-teal-500/20 text-teal-300" },
  system: { label: "System", className: "bg-emerald-500/15 text-emerald-300" },
};

const levels: Record<Level, { dot: string; text: string }> = {
  error: { dot: "bg-red-400", text: "text-red-400" },
  warn: { dot: "bg-amber-400", text: "text-amber-300" },
  notice: { dot: "bg-yellow-300", text: "text-yellow-200" },
  info: { dot: "bg-sky-400", text: "text-sky-300" },
  access: { dot: "bg-sky-400", text: "text-sky-300" },
  debug: { dot: "bg-slate-500", text: "text-slate-400" },
};

const levelOrder: Level[] = ["error", "warn", "notice", "info", "access", "debug"];

const ranges = [
  { id: "all", label: "All time", ms: Infinity },
  { id: "15m", label: "Last 15 minutes", ms: 15 * 60_000 },
  { id: "1h", label: "Last hour", ms: 60 * 60_000 },
  { id: "24h", label: "Last 24 hours", ms: 24 * 60 * 60_000 },
] as const;

const pad = (n: number) => String(n).padStart(2, "0");
const stamp = (d: Date) =>
  `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;

export default function Logs() {
  const logs = useAppStore((s) => s.logs);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const loadHistory = useAppStore((s) => s.loadHistory);
  const notify = useAppStore((s) => s.notify);
  const [selectedServices, setSelectedServices] = useState<Set<LogService>>(() => {
    // Opened from a service's "View logs": start filtered to it, once.
    const focus = useAppStore.getState().logFilter;
    useAppStore.setState({ logFilter: null });
    return focus ? new Set([focus]) : new Set(services.map((s) => s.id));
  });
  const [wrapLines, setWrapLines] = useState(true);
  const [confirmClear, setConfirmClear] = useState(false);
  const [loading, setLoading] = useState(true);

  // Always show what is on disk, including earlier runs.
  useEffect(() => {
    api
      .logHistory()
      .then(loadHistory)
      .catch((e) => notify("error", String(e)))
      .finally(() => setLoading(false));
  }, [loadHistory, notify]);

  const clearAll = async () => {
    setConfirmClear(false);
    setFrozen(null);
    clearLogs();
    try {
      await api.clearLogs();
    } catch (e) {
      notify("error", String(e));
    }
    loadHistory(await api.logHistory());
  };
  const [query, setQuery] = useState("");
  const [shownLevels, setShownLevels] = useState<Set<Level>>(() => new Set(levelOrder));
  const [range, setRange] = useState<(typeof ranges)[number]["id"]>("all");
  const [autoScroll, setAutoScroll] = useState(true);
  /** When paused, the view is frozen at this many lines. */
  const [frozen, setFrozen] = useState<LogEntry[] | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  const source = frozen ?? logs;
  const allServicesSelected = selectedServices.size === services.length;
  const serviceSummary = allServicesSelected
    ? "All services"
    : selectedServices.size === 1
      ? services.find((s) => selectedServices.has(s.id))?.label ?? "1 service"
      : `${selectedServices.size} services`;
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const since = Date.now() - ranges.find((r) => r.id === range)!.ms;
    return source.filter(
      (l) =>
        selectedServices.has(l.service) &&
        shownLevels.has(l.level) &&
        l.time.getTime() >= since &&
        (!q || l.message.toLowerCase().includes(q)),
    );
  }, [source, selectedServices, query, shownLevels, range]);

  const toggleService = (service: LogService) =>
    setSelectedServices((current) => {
      const next = new Set(current);
      if (next.has(service)) next.delete(service);
      else next.add(service);
      return next;
    });

  const toggleLevel = (level: Level) =>
    setShownLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) next.delete(level);
      else next.add(level);
      return next;
    });

  const exportLogs = async () => {
    const path = await save({
      defaultPath: `lamprabit-logs-${stamp(new Date()).replace(/[: ]/g, "-")}.log`,
      filters: [{ name: "Log file", extensions: ["log", "txt"] }],
    });
    if (!path) return;
    const text = visible
      .map((l) => `[${stamp(l.time)}] [${badge[l.service].label}] ${l.level.padEnd(6)} | ${l.message}`)
      .join("\n");
    try {
      await api.saveText(path, text + "\n");
      notify("info", `Exported ${visible.length} lines`);
    } catch (e) {
      notify("error", String(e));
    }
  };
  const copyLogs = async () => {
    const text = visible.map((l) => `[${stamp(l.time)}] [${badge[l.service].label}] ${l.level.padEnd(6)} | ${l.message}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      notify("info", `Copied ${visible.length} log ${visible.length === 1 ? "line" : "lines"}`);
    } catch (e) {
      notify("error", String(e));
    }
  };
  const newWhilePaused = frozen ? Math.max(0, logs.length - frozen.length) : 0;

  useEffect(() => {
    if (autoScroll && !frozen) scroller.current?.scrollTo({ top: scroller.current.scrollHeight });
  }, [visible.length, autoScroll, frozen]);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden motion-stagger">
      <PageHeader title="Logs" subtitle="Live output from your local services." />

      <Card className="mb-3 shrink-0 space-y-2.5 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <details className="group relative shrink-0">
            <summary className="flex h-10 min-w-40 cursor-pointer list-none items-center justify-between gap-3 rounded-lg border border-line bg-surface px-3 text-sm hover:border-accent/50 [&::-webkit-details-marker]:hidden">
              <span>{serviceSummary}</span><ChevronDown size={14} className="text-muted transition-transform group-open:rotate-180" />
            </summary>
            <div className="absolute top-full left-0 z-30 mt-1.5 w-60 rounded-xl border border-line bg-surface p-1.5 shadow-float">
              <label className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-sm font-semibold hover:bg-surface-2">
                <input type="checkbox" className="size-4 accent-(--accent)" checked={allServicesSelected} onChange={() => setSelectedServices(allServicesSelected ? new Set() : new Set(services.map((s) => s.id)))} />
                All services
              </label>
              <div className="my-1 h-px bg-line" />
              {services.map((s) => (
                <label key={s.id} className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm hover:bg-surface-2">
                  <input type="checkbox" className="size-4 accent-(--accent)" checked={selectedServices.has(s.id)} onChange={() => toggleService(s.id)} />
                  {s.label}
                </label>
              ))}
            </div>
          </details>

          <label className="flex h-10 min-w-56 flex-1 items-center gap-2 rounded-lg border border-line bg-surface px-3 focus-within:border-accent">
            <Search size={15} className="shrink-0 text-muted" aria-hidden />
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search log messages" aria-label="Search log messages" className="w-full bg-transparent text-sm outline-none placeholder:text-muted" />
            {query && <button type="button" onClick={() => setQuery("")} aria-label="Clear search" className="rounded p-1 text-muted hover:bg-surface-2 hover:text-fg"><X size={14} /></button>}
          </label>

          <label className="relative">
            <span className="sr-only">Time range</span>
            <select value={range} onChange={(e) => setRange(e.target.value as typeof range)} aria-label="Time range" className="h-10 w-40 appearance-none rounded-lg border border-line bg-surface px-3 pr-8 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/20">
              {ranges.map((r) => <option key={r.id} value={r.id}>{r.label}</option>)}
            </select>
            <ChevronDown size={14} className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-muted" aria-hidden />
          </label>

          <button type="button" onClick={() => setFrozen((current) => current ? null : logs)} aria-pressed={!frozen} className="flex h-10 items-center gap-2 rounded-lg border border-line bg-surface px-3 text-sm font-medium text-fg hover:bg-surface-2">
            <span className={clsx("size-2 rounded-full", frozen ? "bg-slate-400" : "animate-pulse bg-emerald-500")} aria-hidden />
            {frozen ? <><Play size={14} /> Paused</> : <><Pause size={14} /> Live</>}
          </button>
          <Button variant="ghost" onClick={() => setWrapLines((current) => !current)} aria-pressed={wrapLines} title={wrapLines ? "Turn off line wrapping" : "Wrap long lines"}>
            <WrapText size={15} /> {wrapLines ? "Wrap" : "No wrap"}
          </Button>
          <Button variant="ghost" onClick={copyLogs} disabled={visible.length === 0}><Copy size={14} /> Copy</Button>
          <Button variant="ghost" onClick={exportLogs} disabled={visible.length === 0}><Download size={14} /> Export</Button>
          {confirmClear ? (
            <div className="flex items-center gap-2 rounded-lg border border-danger/30 bg-danger-soft px-2 py-1">
              <span className="text-xs text-danger-text">Delete all log files?</span>
              <Button size="sm" variant="danger" onClick={clearAll}>Clear</Button>
              <Button size="sm" onClick={() => setConfirmClear(false)}>Cancel</Button>
            </div>
          ) : (
            <Button variant="ghost" onClick={() => setConfirmClear(true)}><Trash2 size={14} /> Clear</Button>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2 border-t border-line pt-2.5">
          <span className="mr-1 text-xs font-semibold text-muted">Severity</span>
          <div className="flex flex-wrap items-center gap-0.5 rounded-lg border border-line bg-bg p-1" role="group" aria-label="Filter by severity">
            {levelOrder.map((level) => (
              <button key={level} onClick={() => toggleLevel(level)} aria-pressed={shownLevels.has(level)} className={clsx("flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium capitalize transition-colors", shownLevels.has(level) ? clsx("bg-surface shadow-card", levels[level].text) : "text-muted hover:bg-surface-2 hover:text-fg")}>
                <span className={clsx("size-1.5 rounded-full", levels[level].dot)} />{level}
              </button>
            ))}
          </div>
          <span className="ml-auto text-xs tabular-nums text-muted">{visible.length} matching {visible.length === 1 ? "line" : "lines"}</span>
        </div>
      </Card>

      <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#10151c] text-slate-200 shadow-float" aria-label="Log output">
        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-white/10 bg-[#141b24] px-4 py-2.5 text-xs">
          <span className="font-semibold text-slate-200">Log output</span>
          <span className="text-slate-400">{serviceSummary} · {visible.length} {visible.length === 1 ? "line" : "lines"}</span>
        </div>
        <div ref={scroller} className="selectable min-h-0 flex-1 overflow-auto overscroll-contain pb-3 font-mono text-[12px] leading-6">
          <div className={clsx("sticky top-0 z-10 grid gap-0 border-b border-white/10 bg-[#141b24] px-4 py-2 text-[11px] font-semibold tracking-wide text-slate-400 uppercase", wrapLines ? "grid-cols-[11rem_7rem_6rem_minmax(0,1fr)] min-w-[38rem]" : "grid-cols-[11rem_7rem_6rem_max-content] w-max min-w-full")}>
            <span>Timestamp</span><span>Service</span><span>Level</span><span>Message</span>
          </div>
          {visible.length === 0 ? (
            <div className="flex min-h-48 items-center justify-center px-4 text-center font-sans text-sm text-slate-400">
              {loading ? "Loading logs…" : logs.length === 0 ? "No log output yet. Start a service to see its logs here." : "No lines match your filter."}
            </div>
          ) : (
            <div className="min-w-0">
              {visible.map((l) => (
                <div key={l.id} className={clsx("grid items-start gap-0 px-4 py-1.5 font-mono text-[12px] leading-6 hover:bg-white/5", wrapLines ? "grid-cols-[11rem_7rem_6rem_minmax(0,1fr)] min-w-[38rem]" : "grid-cols-[11rem_7rem_6rem_max-content] w-max min-w-full")}>
                  <span className="whitespace-nowrap pr-3 text-slate-400">{stamp(l.time)}</span>
                  <span className="pr-3"><span className={clsx("inline-flex rounded-md px-2 py-0.5 text-[11px]", badge[l.service].className)}>{badge[l.service].label}</span></span>
                  <span className={clsx("whitespace-nowrap pr-3 capitalize", levels[l.level].text)}><span className={clsx("mr-2 inline-block size-1.5 rounded-full align-middle", levels[l.level].dot)} />{l.level}</span>
                  <span className={clsx("min-w-0", wrapLines ? "break-words whitespace-pre-wrap" : "whitespace-pre")}>{l.message}</span>
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="sticky bottom-0 z-10 flex shrink-0 flex-wrap items-center gap-3 border-t border-white/10 bg-[#141b24] px-4 py-2.5 text-xs text-slate-300">
          <span className={clsx("size-2 rounded-full", frozen ? "bg-slate-400" : "animate-pulse bg-emerald-400")} />
          <span className={clsx("font-semibold", frozen ? "text-slate-300" : "text-emerald-300")}>{frozen ? "Paused" : "Live"}</span>
          <span className="text-slate-500">·</span>
          <span>{frozen ? `${newWhilePaused} new ${newWhilePaused === 1 ? "line" : "lines"} since pausing` : "Waiting for service output"}</span>
          <span className="flex-1" />
          <Switch checked={autoScroll} onChange={setAutoScroll} label="Auto-scroll" />
        </div>
      </section>
    </div>
  );
}
