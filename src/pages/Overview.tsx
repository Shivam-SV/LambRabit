import { useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  Activity as Pulse,
  AlertTriangle,
  Check,
  ChevronRight,
  Copy,
  Database,
  DatabaseZap,
  ExternalLink,
  Feather,
  FileCog,
  FileText,
  FolderOpen,
  Globe,
  Keyboard,
  Layers,
  Layers3,
  Mail,
  Moon,
  Pencil,
  Play,
  Plus,
  RotateCw,
  ScrollText,
  Server as ServerIcon,
  Square,
  Sun,
  SunMedium,
  Table2,
  TerminalSquare,
  Trash2,
  Zap,
} from "lucide-react";
import clsx from "clsx";
import {
  api,
  runtimeLabels,
  serviceLabels,
  type Database as DatabaseKind,
  type PortStatus,
  type ServiceInfo,
  type ServiceKind,
  type Settings,
  type Site,
  type SitesInfo,
} from "../lib/api";
import { useAppStore, type Activity } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { runtimeTiles } from "../components/runtimes";
import { formatClock, formatUptime, useNow } from "../lib/time";
import { randomQuote } from "../lib/quotes";
import { connection, copyText, envText } from "../lib/connections";
import { formatBytes, formatCpu, useUsage, useUsageSampler } from "../lib/usage";
import { serverEnabled, useStack } from "../lib/stack";
import { useHotkeys } from "../lib/shortcuts";
import { Button, Card, Pill, Segmented } from "../components/ui";
import Menu from "../components/Menu";
import ProblemPanel from "../components/ProblemPanel";
import ProfilesDialog from "../components/ProfilesDialog";
import Sparkline from "../components/Sparkline";
import { Kbd } from "../components/Dialog";

const greetings = [
  { until: 5, text: "Good evening", icon: Moon },
  { until: 12, text: "Good morning", icon: Sun },
  { until: 17, text: "Good afternoon", icon: SunMedium },
  { until: 24, text: "Good evening", icon: Moon },
];

const serviceMeta: Record<ServiceKind, { icon: typeof Feather; tile: string; blurb: string }> = {
  apache: { icon: Feather, tile: "bg-rose-500/10 text-rose-500", blurb: "Web server for your local sites" },
  nginx: { icon: ServerIcon, tile: "bg-emerald-600/10 text-emerald-600", blurb: "Fast web server for your local sites" },
  mariadb: { icon: Database, tile: "bg-amber-600/12 text-amber-600", blurb: "Database server for your local sites" },
  mysql: { icon: DatabaseZap, tile: runtimeTiles.mysql.className, blurb: "Oracle MySQL, next to MariaDB" },
  mailpit: { icon: Mail, tile: "bg-sky-500/10 text-sky-500", blurb: "Catches mail sent by your apps" },
  redis: { icon: Zap, tile: "bg-red-500/10 text-red-500", blurb: "Cache, sessions and queues" },
  memcached: { icon: Layers3, tile: "bg-teal-500/10 text-teal-600", blurb: "Memory cache for sessions and queries" },
};

/** Services listed only once installed; the core stack is always listed, with an Install button. */
const optional: ServiceKind[] = ["mysql", "mailpit", "redis", "memcached"];

const visible = (s: ServiceInfo, settings?: Settings) =>
  (!optional.includes(s.kind) || s.installed) && serverEnabled(s.kind, settings?.webServer);

export default function Overview() {
  const now = useNow();
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const queryClient = useQueryClient();
  const connMode = useAppStore((s) => s.connMode);
  const setConnMode = useAppStore((s) => s.setConnMode);
  const openNewProject = useAppStore((s) => s.setNewProjectOpen);
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 15_000 });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const ports = useQuery({ queryKey: ["port-status"], queryFn: api.portStatus, refetchInterval: 5000 });
  const stack = useStack();

  const list = (services.data ?? []).filter((s) => visible(s, settings.data));
  const [selected, setSelected] = useState<Set<ServiceKind>>(new Set());
  const [focus, setFocus] = useState(0);
  const [profilesDialog, setProfilesDialog] = useState<{ initial?: ServiceKind[]; profile?: Settings["profiles"][number] } | null>(null);
  const [quote] = useState(randomQuote);
  const rows = useRef<(HTMLElement | null)[]>([]);
  useUsageSampler(stack.running.length > 0);

  const hour = new Date(now).getHours();
  const greeting = greetings.find((g) => hour < g.until)!;
  const troubled = list.filter((s) => s.problem || (s.state === "running" && s.health?.status === "degraded"));
  const siteCount = sites.data?.sites.length ?? 0;
  const installedCount = stack.usable.length;

  useEffect(() => {
    if (stack.profile) setSelected(new Set(stack.profile.services));
  }, [stack.profile?.name, stack.profile?.services.join("|")]);

  const deleteProfile = useMutation({
    mutationFn: async (name: string) => {
      if (!settings.data) throw new Error("Settings are still loading");
      return api.saveSettings({ ...settings.data, profiles: settings.data.profiles.filter((p) => p.name !== name) });
    },
    onSuccess: (saved, name) => {
      queryClient.setQueryData(["settings"], saved);
      if (useAppStore.getState().activeProfile === name) useAppStore.getState().setActiveProfile(null);
      notify("info", `Deleted ${name} profile`);
    },
    onError: (e) => notify("error", String(e)),
  });

  const selectable = list.filter((s) => s.installed);
  const chosen = selectable.filter((s) => selected.has(s.kind)).map((s) => s.kind);
  const allSelected = selectable.length > 0 && chosen.length === selectable.length;
  const toggle = (kind: ServiceKind) =>
    setSelected((cur) => {
      const next = new Set(cur);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });

  const focusRow = (step: number) => {
    // The first press lands on the current row; later ones move from it.
    const onRow = rows.current.includes(document.activeElement as HTMLElement);
    const clamped = Math.max(0, Math.min(list.length - 1, onRow ? focus + step : focus));
    setFocus(clamped);
    rows.current[clamped]?.focus();
  };
  const current = list[focus];
  const copy = (text: string, what: string) =>
    navigator.clipboard.writeText(text).then(
      () => notify("info", `Copied ${what}`),
      (e) => notify("error", String(e)),
    );

  useHotkeys({
    arrowdown: () => focusRow(1),
    j: () => focusRow(1),
    arrowup: () => focusRow(-1),
    k: () => focusRow(-1),
    " ": () => current?.installed && toggle(current.kind),
    a: () => setSelected(allSelected ? new Set() : new Set(selectable.map((s) => s.kind))),
    enter: () => {
      if (!current?.installed) return;
      if (current.state === "running") stack.stop([current.kind]);
      else stack.start([current.kind]);
    },
    s: () => stack.start(chosen.length ? chosen : current?.installed ? [current.kind] : []),
    x: () => stack.stop(chosen.length ? chosen : current?.installed ? [current.kind] : []),
    r: () => current?.state === "running" && api.restartService(current.kind).catch((e) => notify("error", String(e))),
    o: () => current && openAction(current, sites.data, settings.data, navigate, notify)?.run(),
    c: () => current?.installed && copy(copyText(connection(current.kind, current.port), connMode), `${current.label} ${connMode === "env" ? ".env" : "URL"}`),
    e: () => setConnMode(connMode === "url" ? "env" : "url"),
    l: () => {
      if (current) useAppStore.getState().showLogs(current.kind);
    },
  });

  return (
    <div className="motion-stagger space-y-6">
      <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
        <div>
          <h1 className="flex items-center gap-3 text-3xl font-extrabold tracking-tight">
            {greeting.text}, developer
            <greeting.icon size={28} strokeWidth={1.75} className="text-accent-text" aria-hidden />
          </h1>
          <p className="mt-2 text-muted" aria-live="polite">
            {installedCount === 0 ? (
              "Install Apache, PHP and MariaDB from Services to get started."
            ) : (
              <>
                {/* <span className={clsx("font-semibold", stack.running.length > 0 ? "text-accent-text" : "text-fg")}>
                  {stack.running.length} of {installedCount} services running
                </span>
                <span className="mx-2 text-line" aria-hidden>
                  •
                </span>
                {siteCount} local {siteCount === 1 ? "site" : "sites"}
                {troubled.length > 0 && (
                  <>
                    <span className="mx-2 text-line" aria-hidden>
                      •
                    </span>
                    <span className="font-medium text-danger-text">
                      {troubled.map((s) => s.label).join(" and ")} {troubled.length === 1 ? "needs" : "need"} attention
                    </span>
                  </>
                )} */}
              </>
            )}
          </p>
          {installedCount > 0 && troubled.length === 0 && (
            <figure className="mt-2 max-w-2xl">
              <blockquote className="text-muted italic">“{quote.text}”</blockquote>
              <figcaption className="mt-0.5 text-xs text-muted/80">— {quote.author}</figcaption>
            </figure>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {installedCount === 0 ? (
            <Button variant="primary" onClick={() => navigate("services")}>
              Set up your stack
            </Button>
          ) : (
            <>
              <Button variant="primary" onClick={stack.startStack} disabled={stack.busy} title="Shift+S">
                <Play size={14} /> {stack.profile ? `Start ${stack.profile.name}` : "Start all"}
              </Button>
              <Button onClick={stack.stopAll} disabled={stack.busy || stack.running.length === 0} title="Shift+X">
                <Square size={12} className="fill-current" /> Stop all
              </Button>
            </>
          )}
        </div>
      </header>

      <VcRuntimeBanner />

      <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_340px]">
        <section aria-labelledby="services-heading">
          <div className="mb-6">
            {installedCount > 0 && (
            <ProfileTiles
              profiles={settings.data?.profiles ?? []}
              active={stack.profile?.name ?? null}
              onSelect={(name) => {
                useAppStore.getState().setActiveProfile(name);
                const profileServices = settings.data?.profiles.find((p) => p.name === name)?.services;
                setSelected(new Set(profileServices ?? selectable.map((s) => s.kind)));
              }}
              onManage={() => setProfilesDialog({})}
              onEdit={(profile) => setProfilesDialog({ profile })}
              onDelete={(name) => {
                if (window.confirm(`Delete the “${name}” profile?`)) deleteProfile.mutate(name);
              }}
              onStartAll={stack.startStack}
              onStartProfile={stack.startProfile}
              busy={stack.busy}
            />
          )}
          </div>
          <div className="mb-3 px-1">
            <div className="flex items-center gap-3">
              <h2 id="services-heading" className="text-base font-bold">Services</h2>
              <span className="text-sm text-muted">{stack.running.length} of {installedCount} running</span>
            </div>
            <div className="flex flex-wrap items-center gap-2.5">
              <label className="flex cursor-pointer items-center gap-2 text-sm text-muted select-none">
                <input
                  type="checkbox"
                  className="size-4 accent-(--accent)"
                  checked={allSelected}
                  ref={(el) => {
                    if (el) el.indeterminate = chosen.length > 0 && !allSelected;
                  }}
                  onChange={() => setSelected(allSelected ? new Set() : new Set(selectable.map((s) => s.kind)))}
                  disabled={selectable.length === 0}
                />
                Select all
              </label>
              <span className="h-5 w-px bg-line ml-auto" aria-hidden />
              <span className="text-sm text-muted tabular-nums" aria-live="polite">
                {chosen.length} selected
              </span>
              <Button size="sm" variant="ghost" disabled={chosen.length === 0} onClick={() => setProfilesDialog({ initial: chosen })}>
                <Layers size={13} /> Save as profile
              </Button>
            </div>
          </div>

          <ul className="space-y-2.5" aria-label="Services">
            {list.map((s, i) => (
              <li key={s.kind}>
                <ServiceRow
                  ref={(el) => {
                    rows.current[i] = el;
                  }}
                  service={s}
                  now={now}
                  sites={sites.data}
                  settings={settings.data}
                  ports={ports.data}
                  selected={selected.has(s.kind)}
                  onToggle={() => toggle(s.kind)}
                  tabIndex={i === focus ? 0 : -1}
                  onFocus={() => setFocus(i)}
                  inProfile={!!stack.profile?.services.includes(s.kind)}
                  profileActive={!!stack.profile}
                  onCopy={copy}
                />
              </li>
            ))}
          </ul>
          <p className="mt-3 flex items-center gap-1.5 px-1 text-xs text-muted">
            <Keyboard size={13} aria-hidden /> Press <Kbd>?</Kbd> for keyboard shortcuts: <Kbd>J</Kbd>
            <Kbd>K</Kbd> move, <Kbd>Space</Kbd> select, <Kbd>Enter</Kbd> start or stop.
          </p>
        </section>

        <aside className="space-y-4" aria-label="Shortcuts and connections">
          <QuickShortcuts sites={sites.data} services={services.data ?? []} settings={settings.data} />
          <ConnectPanel services={stack.usable} onCopy={copy} />
          <RecentProjects sites={sites.data?.sites ?? []} onNew={() => openNewProject(true)} />
          <RecentActivity />
        </aside>
      </div>

      {profilesDialog && <ProfilesDialog initial={profilesDialog.initial} profile={profilesDialog.profile} onClose={() => setProfilesDialog(null)} />}
    </div>
  );
}

function ProfileTiles({
  profiles,
  active,
  onSelect,
  onManage,
  onEdit,
  onDelete,
  onStartAll,
  onStartProfile,
  busy,
}: {
  profiles: Settings["profiles"];
  active: string | null;
  onSelect: (name: string | null) => void;
  onManage: () => void;
  onEdit: (profile: Settings["profiles"][number]) => void;
  onDelete: (name: string) => void;
  onStartAll: () => void;
  onStartProfile: (services: ServiceKind[]) => void;
  busy: boolean;
}) {
  return (
    <section aria-labelledby="profiles-heading">
      <div className="mb-2 flex items-center justify-between px-1">
        <div>
          <h2 id="profiles-heading" className="text-base font-bold">Profiles</h2>
          <p className="text-xs text-muted">Choose a stack to start, or select it to focus its services.</p>
        </div>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(168px,1fr))] gap-3" role="radiogroup" aria-label="Service profiles">
        {[{ name: null, label: "All services", services: [] as ServiceKind[] }, ...profiles.map((p) => ({ ...p, label: p.name }))].map((profile) => {
          const selected = profile.name === active;
          return (
            <article
              key={profile.name ?? "all"}
              className={clsx(
                "relative flex aspect-square flex-col rounded-2xl border p-3 transition-colors",
                selected ? "border-accent/60 bg-accent-soft/50 shadow-card" : "border-line bg-surface hover:border-accent/30",
              )}
            >
              {profile.name && (
                <div className="absolute top-2 right-2">
                  <Menu
                    label={`${profile.name} profile actions`}
                    plain
                    items={[
                      { label: "Edit profile", icon: <Pencil size={14} />, onSelect: () => onEdit(profile) },
                      { label: "Delete profile", icon: <Trash2 size={14} />, onSelect: () => onDelete(profile.name) },
                    ]}
                  />
                </div>
              )}
              <button
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => onSelect(profile.name)}
                className="min-h-0 flex-1 rounded-lg pr-7 text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
              >
                <span className="flex items-center gap-2 text-sm font-bold"><Layers size={16} className={selected ? "text-accent-text" : "text-muted"} />{profile.name ?? profile.label}</span>
                <span className="mt-2 line-clamp-4 block text-xs leading-relaxed text-muted">
                  {profile.name ? profile.services.map((kind) => serviceLabels[kind]).join(" · ") : "Every installed service"}
                </span>
              </button>
              <Button
                size="sm"
                variant={selected ? "primary" : "soft"}
                className="mt-2 w-full justify-center"
                disabled={busy}
                onClick={() => profile.name ? onStartProfile(profile.services) : onStartAll()}
              >
                <Play size={12} /> Start {profile.name ?? "all"}
              </Button>
            </article>
          );
        })}
        <button
          type="button"
          onClick={onManage}
          className="flex aspect-square flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-line bg-transparent p-4 text-sm font-semibold text-muted transition-colors hover:border-accent/50 hover:bg-accent-soft/30 hover:text-accent-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        >
          <span className="flex size-10 items-center justify-center rounded-full border border-dashed border-current"><Plus size={19} /></span>
          Add new profile
        </button>
      </div>
    </section>
  );
}

/** What "Open" does for a service: its site, inbox or database tool. */
function openAction(
  service: ServiceInfo,
  sites: SitesInfo | undefined,
  settings: Settings | undefined,
  navigate: (p: "services") => void,
  notify: (tone: "error", text: string) => void,
): { label: string; icon: typeof ExternalLink; run: () => void; always?: boolean } | null {
  const onError = (e: unknown) => notify("error", String(e));
  const mailUi = settings?.ports.mailUi;
  if (service.kind === "apache" || service.kind === "nginx") {
    return { label: "Open in browser", icon: ExternalLink, run: () => openUrl(connection(service.kind, service.port).url + "/").catch(onError) };
  }
  if (service.kind === "mariadb" || service.kind === "mysql") {
    // phpMyAdmin, else Adminer; both are served by the web server and reach either database.
    const tool = (["phpmyadmin", "adminer"] as const).find((t) => toolUrl(t, sites, mailUi));
    if (!tool) return { label: "Install phpMyAdmin", icon: Table2, run: () => navigate("services"), always: true };
    // Open on this database's server: phpMyAdmin numbers its servers (default database
    // first, as configs.rs writes them); Adminer takes the address.
    const order = (["mariadb", "mysql"] as const)
      .filter((d) => settings?.versions[d])
      .sort((a, b) => Number(b === settings?.defaultDatabase) - Number(a === settings?.defaultDatabase));
    const url =
      tool === "phpmyadmin"
        ? `${toolUrl(tool, sites, mailUi)}?server=${Math.max(1, order.indexOf(service.kind) + 1)}`
        : `${toolUrl(tool, sites, mailUi)}?server=127.0.0.1:${service.port}&username=root`;
    return { label: `Open ${runtimeLabels[tool]}`, icon: Table2, run: () => openUrl(url).catch(onError) };
  }
  if (service.kind === "redis") {
    const url = toolUrl("phpredisadmin", sites, mailUi);
    return url
      ? { label: "Open phpRedisAdmin", icon: Table2, run: () => openUrl(url).catch(onError) }
      : { label: "Install phpRedisAdmin", icon: Table2, run: () => navigate("services"), always: true };
  }
  if (service.kind === "mailpit") {
    return { label: "Open inbox", icon: ExternalLink, run: () => openUrl(toolUrl("mailpit", sites, mailUi) ?? "http://localhost:8025/").catch(onError) };
  }
  return null;
}

/** A port another program holds, found before the service is started. */
function conflictFor(service: ServiceInfo, ports?: PortStatus[]) {
  if (service.state === "running" || service.state === "starting") return null;
  const taken = ports?.find((p) => p.port === service.port && !p.free && !p.usedByService);
  return taken ?? null;
}

function ServiceRow({
  ref,
  service,
  now,
  sites,
  settings,
  ports,
  selected,
  onToggle,
  tabIndex,
  onFocus,
  inProfile,
  profileActive,
  onCopy,
}: {
  ref: (el: HTMLElement | null) => void;
  service: ServiceInfo;
  now: number;
  sites?: SitesInfo;
  settings?: Settings;
  ports?: PortStatus[];
  selected: boolean;
  onToggle: () => void;
  tabIndex: number;
  onFocus: () => void;
  inProfile?: boolean;
  profileActive?: boolean;
  onCopy: (text: string, what: string) => void;
}) {
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const showLogs = useAppStore((s) => s.showLogs);
  const connMode = useAppStore((s) => s.connMode);
  const usage = useUsage((s) => s.series[service.kind]);
  // PHP's workers belong to no one service; the web servers show them, since that's
  // where a slow page's time goes.
  const php = useUsage((s) => (service.kind === "apache" || service.kind === "nginx" ? s.latest.php : undefined));
  const onError = (e: unknown) => notify("error", String(e));
  // A failed start shows on the row itself (ProblemPanel), so no toast for it.
  const start = useMutation({ mutationFn: () => api.startService(service.kind), onError: () => { } });
  const stop = useMutation({ mutationFn: () => api.stopService(service.kind), onError });
  const restart = useMutation({ mutationFn: () => api.restartService(service.kind), onError });
  const meta = serviceMeta[service.kind];
  const running = service.state === "running";
  const busy = service.state === "starting" || service.state === "stopping" || restart.isPending || start.isPending || stop.isPending;
  const degraded = running && service.health?.status === "degraded";
  const conflict = conflictFor(service, ports);
  const action = openAction(service, sites, settings, navigate, notify);
  const conn = connection(service.kind, service.port);
  const connValue = copyText(conn, connMode);

  const status = !service.installed
    ? { tone: "muted" as const, text: "Not installed" }
    : degraded
      ? { tone: "danger" as const, text: "Not responding" }
      : running
        ? { tone: "ok" as const, text: service.health?.status === "healthy" ? "Healthy" : "Running" }
        : service.problem
          ? { tone: "danger" as const, text: "Failed" }
          : service.state === "starting" || service.state === "stopping"
            ? { tone: "warn" as const, text: service.state === "starting" ? "Starting" : "Stopping" }
            : { tone: "muted" as const, text: "Stopped" };

  const dbMenu =
    service.kind === "mariadb" || service.kind === "mysql"
      ? [{ label: "Open command-line client", icon: <TerminalSquare size={15} />, onSelect: () => api.openDbClient(service.kind as DatabaseKind).catch(onError) }]
      : [];

  return (
    <article
      ref={ref}
      tabIndex={tabIndex}
      onFocus={(e) => e.target === e.currentTarget && onFocus()}
      onMouseDown={onFocus}
      aria-label={`${service.label}, ${status.text}${service.installed ? `, port ${service.port}` : ""}`}
      className={clsx(
        "rounded-2xl border bg-surface px-4 py-3.5 shadow-card transition-colors outline-none focus-visible:ring-2 focus-visible:ring-accent/50",
        selected ? "border-accent/50 bg-accent-soft/40" : running ? "border-accent/25" : "border-line",
        !service.installed && "border-dashed",
        profileActive && !inProfile && "opacity-75",
      )}
    >
      <div className="flex items-start gap-3.5 mb-4">
        <input
          type="checkbox"
          className="mt-3.5 size-4 shrink-0 accent-(--accent) disabled:opacity-30"
          checked={selected}
          onChange={onToggle}
          disabled={!service.installed}
          aria-label={`Select ${service.label}`}
          tabIndex={-1}
        />
        <div className={clsx("flex size-11 shrink-0 items-center justify-center rounded-xl", meta.tile)} aria-hidden>
          <meta.icon size={21} strokeWidth={1.75} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <h3 className="text-[15px] font-bold">{service.label}</h3>
            <Pill tone={status.tone}>{status.text}</Pill>
            {service.version && <span className="font-mono text-xs text-muted">v{service.version}</span>}
          </div>
          <p className="mt-0.5 truncate text-sm text-muted">{meta.blurb}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {!service.installed ? (
            <Button size="sm" variant="primary" onClick={() => navigate("services")}>
              Install
            </Button>
          ) : running || service.state === "stopping" ? (
            <>
              <Button size="sm" onClick={() => stop.mutate()} disabled={busy} aria-label={`Stop ${service.label}`}>
                <Square size={11} className="fill-current" /> Stop
              </Button>
              <Button size="sm" variant="ghost" onClick={() => restart.mutate()} disabled={busy} aria-label={`Restart ${service.label}`} title="Restart (R)">
                <RotateCw size={14} className={clsx(restart.isPending && "animate-spin")} />
              </Button>
            </>
          ) : (
            <Button size="sm" variant="soft" onClick={() => start.mutate()} disabled={busy} aria-label={`Start ${service.label}`}>
              <Play size={12} /> Start
            </Button>
          )}
          {action && service.installed && (
            <Button
              size="sm"
              variant="ghost"
              onClick={action.run}
              disabled={!running && !action.always}
              aria-label={action.label}
              title={`${action.label} (O)`}
            >
              <action.icon size={14} />
            </Button>
          )}
          <Menu
            items={[
              ...dbMenu,
              {
                label: service.kind === "mailpit" ? "Open data folder" : "Open config file",
                icon: <FileCog size={15} />,
                onSelect: () => openPath(service.configPath).catch(() => notify("error", "Created on first start")),
              },
              {
                label: "Open error log",
                icon: <FileText size={15} />,
                onSelect: () => openPath(service.logPath).catch(() => notify("error", "No log file yet")),
              },
              { label: "View logs", icon: <ScrollText size={15} />, onSelect: () => showLogs(service.kind) },
            ]}
          />
        </div>
      </div>

      {service.installed && (
        <div className="mt-3 flex flex-wrap items-end gap-x-6 gap-y-3 pl-[4.4rem]">
          <Stat label="Port" value={service.port} live={running} />
          <Stat label="PID" value={service.pid ?? "—"} />
          <Stat label="Uptime" value={running && service.startedAt ? formatUptime(now - service.startedAt) : "—"} />
          {running && (
            <>
              <Sparkline label="CPU" values={usage?.cpu ?? []} format={formatCpu} floor={10} note="100% is one full CPU core" />
              <Sparkline label="RAM" values={usage?.memory ?? []} format={formatBytes} floor={64 * 1024 ** 2} />
              {php && (
                <span title={`All PHP workers: ${php.processes} process${php.processes === 1 ? "" : "es"}`}>
                  <Stat label="PHP" value={`${formatCpu(php.cpu)} · ${formatBytes(php.memory)}`} />
                </span>
              )}
            </>
          )}
          <button
            onClick={() => onCopy(connValue, `${service.label} ${connMode === "env" ? ".env" : "URL"}`)}
            className="group ml-auto flex max-w-full min-w-0 items-center gap-2 rounded-lg border border-line bg-bg px-2.5 py-1.5 text-left font-mono text-xs text-fg hover:border-accent/40"
            title={connMode === "env" ? envText(conn.env) : `Copy ${conn.url} (C)`}
            aria-label={`Copy ${service.label} connection ${connMode === "env" ? "as .env lines" : conn.url}`}
          >
            <span className="truncate">
              {connMode === "url" ? conn.url : conn.env.length === 1 ? envText(conn.env) : `${conn.env[0][0]}=… +${conn.env.length - 1} more`}
            </span>
            <Copy size={13} className="shrink-0 text-muted group-hover:text-accent-text" />
          </button>
        </div>
      )}

      {conflict && (
        <div className="mt-3 ml-[4.4rem] flex items-start gap-2.5 rounded-xl border border-warn/30 bg-warn/10 p-3 text-sm" role="status">
          <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warn" aria-hidden />
          <div>
            <span className="font-medium">Port {conflict.port} is in use</span>
            <span className="text-muted">
              {" "}
              by {conflict.owner ? `${conflict.owner.name} (PID ${conflict.owner.pid})` : "another program"}, so {service.label} can't start on it.
              Stop that program, or pick another port in Settings → Ports.
            </span>
          </div>
        </div>
      )}
      {degraded && (
        <div className="mt-3 ml-[4.4rem] flex items-start gap-2.5 rounded-xl border border-warn/30 bg-warn/10 p-3 text-sm" role="status">
          <Pulse size={16} className="mt-0.5 shrink-0 text-warn" aria-hidden />
          <div>
            <span className="font-medium">The process is running but not answering.</span>{" "}
            <span className="text-muted">{service.health?.detail} Try Restart, or check its logs.</span>
          </div>
        </div>
      )}
      {service.problem && !running && (
        <div className="ml-[4.4rem]">
          <ProblemPanel kind={service.kind} problem={service.problem} />
        </div>
      )}
    </article>
  );
}

function Stat({ label, value, live }: { label: string; value: string | number; live?: boolean }) {
  return (
    <div className="flex min-w-12 flex-col gap-0.5">
      <span className="text-[10px] font-bold tracking-wider text-muted uppercase">{label}</span>
      <span className={clsx("font-mono text-[13px] tabular-nums", live ? "font-semibold text-accent-text" : "text-fg")}>{value}</span>
    </div>
  );
}

/** Windows: Apache and PHP need Microsoft's Visual C++ runtime. */
function VcRuntimeBanner() {
  const notify = useAppStore((s) => s.notify);
  const status = useQuery({ queryKey: ["vc-runtime"], queryFn: api.vcRuntimeStatus });
  const install = useMutation({
    mutationFn: api.vcRuntimeInstall,
    onSuccess: () => {
      notify("info", "Visual C++ runtime installed");
      status.refetch();
    },
    onError: (e) => notify("error", String(e)),
  });
  if (!status.data?.applicable || status.data.installed) return null;
  return (
    <div className="flex flex-wrap items-center gap-4 rounded-2xl border border-warn/30 bg-warn/10 px-5 py-3.5">
      <p className="min-w-60 flex-1 text-sm">
        Apache and PHP need the Microsoft Visual C++ runtime, which isn't installed. Windows asks for administrator approval once.
      </p>
      <Button variant="primary" onClick={() => install.mutate()} disabled={install.isPending}>
        {install.isPending ? "Installing…" : "Install runtime"}
      </Button>
    </div>
  );
}

function PanelTitle({ title, extra }: { title: string; extra?: ReactNode }) {
  return (
    <div className="mb-2 flex items-center justify-between gap-2 px-1">
      <h2 className="text-sm font-bold">{title}</h2>
      {extra}
    </div>
  );
}

/** One-click tools, each saying what it needs before it can open. */
function QuickShortcuts({ sites, services, settings }: { sites?: SitesInfo; services: ServiceInfo[]; settings?: Settings }) {
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const mailUi = settings?.ports.mailUi;
  const up = (kind: ServiceKind) => services.some((s) => s.kind === kind && s.state === "running");
  const web = settings?.webServer === "nginx" ? "nginx" : "apache";
  const db = up("mariadb") || up("mysql");

  const installed = (kind: ServiceKind) => services.some((s) => s.kind === kind && s.installed);
  const onError = (e: unknown) => notify("error", String(e));
  const webTool = (tool: "phpmyadmin" | "adminer" | "phpredisadmin") => () => {
    const url = toolUrl(tool, sites, mailUi);
    if (url) openUrl(url).catch(onError);
  };
  const tools = [
    { tool: "mailpit" as const, label: "Mailpit inbox", ready: up("mailpit"), need: "Start Mailpit", readyText: "Open inbox", installed: installed("mailpit"), open: () => openUrl(toolUrl("mailpit", sites, mailUi) ?? "http://localhost:8025/").catch(onError) },
    { tool: "phpmyadmin" as const, label: "phpMyAdmin", ready: up(web) && db, need: `Start ${serviceLabels[web]} and a database`, readyText: "Browse databases", installed: !!toolUrl("phpmyadmin", sites, mailUi), open: webTool("phpmyadmin") },
    { tool: "adminer" as const, label: "Adminer", ready: up(web) && db, need: `Start ${serviceLabels[web]} and a database`, readyText: "Lightweight database admin", installed: !!toolUrl("adminer", sites, mailUi), open: webTool("adminer") },
    { tool: "phpredisadmin" as const, label: "phpRedisAdmin", ready: up(web) && up("redis"), need: `Start ${serviceLabels[web]} and Redis`, readyText: "Browse Redis keys", installed: !!toolUrl("phpredisadmin", sites, mailUi), open: webTool("phpredisadmin") },
  ];
  const readyCount = tools.filter((t) => t.installed && t.ready).length;

  return (
    <Card className="p-4">
      <PanelTitle title="Quick shortcuts" extra={<span className="text-xs text-muted">{readyCount} of {tools.length} ready</span>} />
      <div className="space-y-1">
        {tools.map((t) => {
          const tiles = runtimeTiles[t.tool];
          const hint = !t.installed ? "Not installed. Click to install" : t.ready ? t.readyText : t.need;
          return (
            <button
              key={t.tool}
              onClick={() => (t.installed ? t.open() : navigate("services"))}
              disabled={t.installed && !t.ready}
              className="group flex w-full items-center gap-3 rounded-xl border border-transparent px-2 py-2 text-left hover:border-line hover:bg-surface-2/60 disabled:cursor-not-allowed disabled:opacity-70 disabled:hover:border-transparent disabled:hover:bg-transparent"
            >
              <span className={clsx("flex size-8 shrink-0 items-center justify-center rounded-lg", tiles.className)} aria-hidden>
                {tiles.icon(16)}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{t.label}</span>
                <span className={clsx("block truncate text-xs", t.installed && t.ready ? "font-medium text-accent-text" : "text-muted")}>{hint}</span>
              </span>
              <ChevronRight size={15} className="shrink-0 text-muted" aria-hidden />
            </button>
          );
        })}
      </div>
    </Card>
  );
}

/** Connection details for the installed services, as URLs or ready-to-paste .env lines. */
function ConnectPanel({ services, onCopy }: { services: ServiceInfo[]; onCopy: (text: string, what: string) => void }) {
  const mode = useAppStore((s) => s.connMode);
  const setMode = useAppStore((s) => s.setConnMode);
  const [copied, setCopied] = useState<string | null>(null);
  const shown = services.filter((s) => s.kind !== "nginx" || !services.some((x) => x.kind === "apache"));
  if (shown.length === 0) return null;

  const copy = (key: string, text: string, what: string) => {
    onCopy(text, what);
    setCopied(key);
    setTimeout(() => setCopied((c) => (c === key ? null : c)), 1500);
  };
  const allEnv = shown.map((s) => `# ${s.label}\n${envText(connection(s.kind, s.port).env)}`).join("\n\n");

  return (
    <Card className="p-4">
      <PanelTitle
        title="Connect your app"
        extra={
          <Segmented
            value={mode}
            onChange={setMode}
            className="p-0.5! text-xs"
            options={[
              { value: "url", label: "URL" },
              { value: "env", label: ".env" },
            ]}
          />
        }
      />
      <ul className="space-y-1.5">
        {shown.map((s) => {
          const c = connection(s.kind, s.port);
          const text = copyText(c, mode);
          return (
            <li key={s.kind}>
              <button
                onClick={() => copy(s.kind, text, `${s.label} ${mode === "env" ? ".env" : "URL"}`)}
                className="group flex w-full items-start gap-2 rounded-lg border border-line bg-bg px-2.5 py-2 text-left hover:border-accent/40"
                aria-label={`Copy ${s.label} connection`}
              >
                <span className="min-w-0 flex-1">
                  <span className="block text-xs font-semibold text-muted">{s.label}</span>
                  <span className="block font-mono text-xs break-all whitespace-pre-wrap text-fg">{text}</span>
                </span>
                {copied === s.kind ? (
                  <Check size={14} className="mt-0.5 shrink-0 text-accent-text" aria-hidden />
                ) : (
                  <Copy size={14} className="mt-0.5 shrink-0 text-muted group-hover:text-accent-text" aria-hidden />
                )}
              </button>
            </li>
          );
        })}
      </ul>
      <Button size="sm" variant="soft" className="mt-3 w-full" onClick={() => copy("all", allEnv, "all services as .env")}>
        {copied === "all" ? <Check size={14} /> : <Copy size={14} />} Copy everything as .env
      </Button>
    </Card>
  );
}

function RecentProjects({ sites, onNew }: { sites: Site[]; onNew: () => void }) {
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const recent = [...sites].sort((a, b) => b.modified - a.modified).slice(0, 4);

  return (
    <Card className="p-4">
      <PanelTitle
        title="Recent projects"
        extra={
          <button onClick={() => navigate("sites")} className="text-xs font-semibold text-accent-text hover:underline">
            View all ›
          </button>
        }
      />
      {recent.length === 0 && <p className="px-1 py-2 text-sm text-muted">Each folder in www becomes a site.</p>}
      <ul className="space-y-0.5">
        {recent.map((site) => (
          <li key={site.path} className="group flex items-center gap-1">
            <button
              onClick={() => openUrl(site.url).catch(onError)}
              className="flex min-w-0 flex-1 items-center gap-3 rounded-xl px-2 py-2 text-left hover:bg-surface-2/60"
              title={`Open ${site.url}`}
            >
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent-text" aria-hidden>
                <Globe size={16} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{site.domain}</span>
                <span className="selectable block truncate font-mono text-xs text-muted">{site.path}</span>
              </span>
              <span className="shrink-0 rounded-md bg-surface-2 px-1.5 py-0.5 text-[11px] font-medium text-muted">
                {serviceLabels[site.server]}
              </span>
            </button>
            <Button size="sm" variant="ghost" onClick={() => openPath(site.path).catch(onError)} aria-label={`Open ${site.name} folder`}>
              <FolderOpen size={14} />
            </Button>
          </li>
        ))}
      </ul>
      <button
        onClick={onNew}
        className="mt-1.5 flex w-full items-center justify-center gap-1.5 rounded-xl border border-dashed border-line py-2 text-sm font-medium text-muted hover:border-accent/50 hover:bg-accent-soft/40 hover:text-accent-text"
        title="New project (N)"
      >
        <Plus size={14} /> New project
      </button>
    </Card>
  );
}

const activityDot: Record<Activity["tone"], string> = {
  ok: "bg-ok",
  info: "bg-sky-500",
  muted: "bg-muted/50",
  error: "bg-danger",
};

function RecentActivity() {
  const activity = useAppStore((s) => s.activity);
  const navigate = useAppStore((s) => s.navigate);

  return (
    <Card className="p-4">
      <PanelTitle
        title="Recent activity"
        extra={
          <button onClick={() => navigate("logs")} className="text-xs font-semibold text-accent-text hover:underline">
            Logs ›
          </button>
        }
      />
      <ul className="space-y-1.5 px-1">
        {activity.slice(0, 4).map((a) => (
          <li key={a.id} className="flex items-center gap-2.5 text-xs">
            <span className={clsx("size-2 shrink-0 rounded-full", activityDot[a.tone])} aria-hidden />
            <span className="font-mono text-muted tabular-nums">{formatClock(a.at)}</span>
            <span className="min-w-0 flex-1 truncate">{a.text}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
