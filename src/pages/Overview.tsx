import { useMutation, useQuery } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  ChevronRight,
  Server as ServerIcon,
  Database,
  ExternalLink,
  Feather,
  FileCog,
  FileText,
  FolderOpen,
  Globe,
  Moon,
  Play,
  RotateCw,
  ScrollText,
  Square,
  Sun,
  SunMedium,
  TerminalSquare,
  Activity as Pulse,
  Zap,
  Layers3,
  Mail,
  Table2,
} from "lucide-react";
import clsx from "clsx";
import {
  api,
  runtimeLabels,
  toolRuntimes,
  type Runtime,
  type ServiceInfo,
  type ServiceKind,
  type Site,
  type SitesInfo,
  type WebServer,
} from "../lib/api";
import { useAppStore, type Activity } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { runtimeTiles } from "../components/runtimes";
import { formatClock, formatUptime, useNow } from "../lib/time";
import { Button, Card, Pill } from "../components/ui";
import Menu from "../components/Menu";
import ProblemPanel from "../components/ProblemPanel";

const greetings = [
  { until: 5, text: "Good evening", icon: Moon },
  { until: 12, text: "Good morning", icon: Sun },
  { until: 17, text: "Good afternoon", icon: SunMedium },
  { until: 21, text: "Good evening", icon: Moon },
  { until: 24, text: "Good evening", icon: Moon },
];

const serviceMeta: Record<ServiceKind, { icon: typeof Feather; tile: string; blurb: string }> = {
  apache: { icon: Feather, tile: "bg-rose-500/10 text-rose-500", blurb: "Web server for your local sites" },
  mariadb: { icon: Database, tile: "bg-amber-600/12 text-amber-600", blurb: "Database server for your local sites" },
  nginx: { icon: ServerIcon, tile: "bg-emerald-600/10 text-emerald-600", blurb: "Fast web server for your local sites" },
  mailpit: { icon: Mail, tile: "bg-sky-500/10 text-sky-500", blurb: "Catches mail sent by your apps" },
  redis: { icon: Zap, tile: "bg-red-500/10 text-red-500", blurb: "Redis-compatible cache (Garnet)" },
  memcached: { icon: Layers3, tile: "bg-teal-500/10 text-teal-600", blurb: "Memory cache for sessions and queries" },
};

export default function Overview() {
  const now = useNow();
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 15_000 });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const mailUi = settings.data?.ports.mailUi;

  const hour = new Date(now).getHours();
  const greeting = greetings.find((g) => hour < g.until)!;
  const installed = services.data?.filter((s) => s.installed && serverEnabled(s.kind, settings.data?.webServer)) ?? [];
  const running = installed.filter((s) => s.state === "running");
  const siteCount = sites.data?.sites.length ?? 0;

  const troubled = installed.filter((s) => s.problem || s.health?.status === "degraded");

  const status =
    installed.length === 0
      ? { tone: "muted", title: "Set up your stack", line: "Install Apache, PHP and MariaDB from Services" }
      : troubled.length > 0
        ? {
            tone: "danger",
            title: "Needs attention",
            line: `${troubled.map((s) => s.label).join(" and ")} ${troubled.length === 1 ? "has" : "have"} a problem. Details are on the card below.`,
          }
      : running.length === installed.length
        ? { tone: "ok", title: "All systems running", line: "Your local development environment is running smoothly." }
        : running.length > 0
          ? { tone: "warn", title: "Partially running", line: "Some services are stopped." }
          : { tone: "muted", title: "All services stopped", line: "Start your stack to begin working." };

  return (
    <div className="motion-stagger space-y-6">
      <section className="flex flex-wrap items-center justify-between gap-6">
        <div>
          <h1 className="flex items-center gap-3 text-4xl font-extrabold tracking-tight">
            {greeting.text}, developer
            <greeting.icon size={34} strokeWidth={1.75} className="text-accent-text" />
          </h1>
          <p className="mt-2 text-lg text-muted">{status.line}</p>
        </div>
        <div
          className={clsx(
            "min-w-72 rounded-2xl border px-5 py-4",
            status.tone === "ok" && "border-accent/25 bg-accent-soft",
            status.tone === "warn" && "border-warn/30 bg-warn/10",
            status.tone === "muted" && "border-line bg-surface",
            status.tone === "danger" && "border-danger/30 bg-danger-soft",
          )}
        >
          <div className="flex items-center gap-2.5">
            <span
              className={clsx(
                "size-2.5 rounded-full",
                status.tone === "ok" && "bg-ok",
                status.tone === "warn" && "bg-warn",
                status.tone === "muted" && "bg-muted/50",
                status.tone === "danger" && "bg-danger",
              )}
            />
            <span
              className={clsx(
                "font-semibold",
                status.tone === "ok" && "text-accent-text",
                status.tone === "warn" && "text-warn",
                status.tone === "danger" && "text-danger-text",
              )}
            >
              {status.title}
            </span>
          </div>
          <p className="mt-1 pl-5 text-sm text-muted">
            {running.length} {running.length === 1 ? "service" : "services"} active
            <span className="mx-2">•</span>
            {siteCount} local {siteCount === 1 ? "site" : "sites"}
          </p>
        </div>
      </section>

      <VcRuntimeBanner />
      <QuickLinks sites={sites.data} services={services.data ?? []} mailUi={mailUi} />

      <section className="grid gap-5 md:grid-cols-2">
        {services.data
          // Optional services only appear once installed; web servers when enabled in Settings.
          ?.filter((s) => (!optional.includes(s.kind) || s.installed) && serverEnabled(s.kind, settings.data?.webServer))
          .map((s) => <ServiceCard key={s.kind} service={s} now={now} sites={sites.data} mailUi={mailUi} />)}
      </section>

      <section className="grid gap-5 md:grid-cols-2">
        <RecentSites sites={sites.data?.sites ?? []} />
        <RecentActivity />
      </section>
    </div>
  );
}

function ServiceCard({
  service,
  now,
  sites,
  mailUi,
}: {
  service: ServiceInfo;
  now: number;
  sites?: SitesInfo;
  mailUi?: number;
}) {
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const onError = (e: unknown) => notify("error", String(e));
  const start = useMutation({ mutationFn: () => api.startService(service.kind), onError });
  const stop = useMutation({ mutationFn: () => api.stopService(service.kind), onError });
  const restart = useMutation({ mutationFn: () => api.restartService(service.kind), onError });
  const meta = serviceMeta[service.kind];
  const running = service.state === "running";
  const busy = service.state === "starting" || service.state === "stopping" || restart.isPending;

  const degraded = running && service.health?.status === "degraded";
  const pill = !service.installed ? (
    <Pill tone="muted">Not installed</Pill>
  ) : degraded ? (
    <Pill tone="danger">Not responding</Pill>
  ) : running ? (
    <Pill tone="ok">{service.health?.status === "healthy" ? "Healthy" : "Running"}</Pill>
  ) : service.problem ? (
    <Pill tone="danger">Failed</Pill>
  ) : service.state === "stopped" || service.state === "error" ? (
    <Pill tone="muted">Stopped</Pill>
  ) : (
    <Pill tone="warn">{service.state === "starting" ? "Starting" : "Stopping"}</Pill>
  );

  // The MariaDB card opens a web database tool: phpMyAdmin, else Adminer, else offer to install one.
  const dbTool = (["phpmyadmin", "adminer"] as const).find((t) => toolUrl(t, sites, mailUi));
  const action: { label: string; icon: typeof ExternalLink; run: () => void; always?: boolean } | null =
    service.kind === "apache"
      ? {
          label: "Open in browser",
          icon: ExternalLink,
          run: () => openUrl(`http://localhost${service.port === 80 ? "" : `:${service.port}`}/`).catch(onError),
        }
      : service.kind === "mariadb"
        ? dbTool
          ? {
              label: `Open ${runtimeLabels[dbTool]}`,
              icon: Table2,
              run: () => openUrl(toolUrl(dbTool, sites, mailUi)!).catch(onError),
            }
          : { label: "Install phpMyAdmin", icon: Table2, run: () => navigate("services"), always: true }
        : service.kind === "mailpit"
          ? {
              label: "Open inbox",
              icon: ExternalLink,
              run: () => openUrl(toolUrl("mailpit", sites, mailUi) ?? "http://localhost:8025/").catch(onError),
            }
          : null;

  // Other ways into the database, in the card's menu.
  const dbMenu =
    service.kind === "mariadb"
      ? [
          ...(["phpmyadmin", "adminer"] as const)
            .filter((t) => t !== dbTool && toolUrl(t, sites, mailUi))
            .map((t) => ({
              label: `Open ${runtimeLabels[t]}`,
              icon: <Table2 size={15} />,
              onSelect: () => openUrl(toolUrl(t, sites, mailUi)!).catch(onError),
            })),
          {
            label: "Open command-line client",
            icon: <TerminalSquare size={15} />,
            onSelect: () => api.openDbClient().catch(onError),
          },
        ]
      : [];

  const stats = [
    { label: "Port", value: service.port },
    { label: "PID", value: service.pid ?? "—" },
    { label: "Uptime", value: running && service.startedAt ? formatUptime(now - service.startedAt) : "—" },
  ];

  return (
    <Card className="p-6">
      <div className="flex items-start gap-4">
        <div className={clsx("flex size-16 shrink-0 items-center justify-center rounded-2xl", meta.tile)}>
          <meta.icon size={30} strokeWidth={1.75} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <h2 className="text-xl font-bold tracking-tight">{service.label}</h2>
            {pill}
            {service.version && <span className="font-mono text-xs text-muted">v{service.version}</span>}
          </div>
          <p className="mt-1 text-muted">{meta.blurb}</p>
        </div>
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
            { label: "View output", icon: <ScrollText size={15} />, onSelect: () => navigate("logs") },
          ]}
        />
      </div>

      <dl className="mt-5 grid grid-cols-3 divide-x divide-line border-t border-line pt-4">
        {stats.map((s) => (
          <div key={s.label} className="px-4 first:pl-0">
            <dt className="text-sm text-muted">{s.label}</dt>
            <dd className="mt-0.5 text-lg font-semibold tabular-nums">{s.value}</dd>
          </div>
        ))}
      </dl>

      {degraded && (
        <div className="mt-5 flex items-start gap-2.5 rounded-xl border border-warn/30 bg-warn/10 p-3 text-sm">
          <Pulse size={16} className="mt-0.5 shrink-0 text-warn" />
          <div>
            <span className="font-medium">The process is running but not answering.</span>{" "}
            <span className="text-muted">{service.health?.detail} Try Restart, or check the Logs page.</span>
          </div>
        </div>
      )}
      {service.problem && !running && <ProblemPanel kind={service.kind} problem={service.problem} />}

      <div className="mt-5 flex flex-wrap items-center gap-2.5">
        {!service.installed ? (
          <Button variant="primary" onClick={() => navigate("services")}>
            Install {service.label}
          </Button>
        ) : running || service.state === "stopping" ? (
          <>
            <Button onClick={() => stop.mutate()} disabled={busy}>
              <Square size={13} className="fill-current" /> Stop
            </Button>
            <Button onClick={() => restart.mutate()} disabled={busy}>
              <RotateCw size={15} className={clsx(restart.isPending && "animate-spin")} /> Restart
            </Button>
          </>
        ) : (
          <Button variant="primary" onClick={() => start.mutate()} disabled={busy || start.isPending}>
            <Play size={15} /> Start
          </Button>
        )}
        {action && (
          <Button variant="soft" className="ml-auto" onClick={action.run} disabled={!running && !action.always}>
            <action.icon size={16} /> {action.label}
          </Button>
        )}
      </div>
    </Card>
  );
}

/** Services shown on Overview only once installed. */
const optional: ServiceKind[] = ["mailpit", "redis", "memcached"];

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

function serverEnabled(kind: ServiceKind, mode: WebServer | undefined) {
  if (kind === "apache") return mode !== "nginx";
  if (kind === "nginx") return mode === "nginx" || mode === "both";
  return true;
}

const quickLinkInfo: Record<string, { label: string; blurb: string; needs: ServiceKind }> = {
  mailpit: { label: "Mailpit inbox", blurb: "Mail sent by your apps", needs: "mailpit" },
  phpmyadmin: { label: "phpMyAdmin", blurb: "Browse and edit databases", needs: "apache" },
  adminer: { label: "Adminer", blurb: "Lightweight database admin", needs: "apache" },
};

/** One-click entry points to the developer tools, with a hint when they can't be reached. */
function QuickLinks({ sites, services, mailUi }: { sites?: SitesInfo; services: ServiceInfo[]; mailUi?: number }) {
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);
  const order: Runtime[] = ["mailpit", "phpmyadmin", "adminer"];
  if (!sites) return null;

  return (
    <section className="grid gap-3 sm:grid-cols-3">
      {order
        .filter((t) => toolRuntimes.includes(t))
        .map((tool) => {
          const info = quickLinkInfo[tool];
          const installed = tool === "mailpit" ? services.some((s) => s.kind === "mailpit" && s.installed) : !!toolUrl(tool, sites, mailUi);
          const url = installed ? toolUrl(tool, sites, mailUi) : undefined;
          const host = services.find((s) => s.kind === info.needs);
          const up = host?.state === "running";
          const tiles = runtimeTiles[tool];
          return (
            <button
              key={tool}
              onClick={() => (url ? openUrl(url).catch((e) => notify("error", String(e))) : navigate("services"))}
              className="group motion-lift flex items-center gap-3 rounded-2xl border border-line bg-surface p-4 text-left shadow-card hover:border-accent/40"
            >
              <div className={clsx("flex size-11 shrink-0 items-center justify-center rounded-xl", tiles.className)}>
                {tiles.icon(20)}
              </div>
              <div className="min-w-0 flex-1">
                <div className="font-semibold">{info.label}</div>
                <div className="flex items-center gap-1.5 truncate text-xs text-muted">
                  {!installed ? (
                    "Not installed. Click to install"
                  ) : up ? (
                    info.blurb
                  ) : (
                    <>
                      <span className="size-1.5 rounded-full bg-warn" />
                      Start {host?.label ?? "the service"} to open
                    </>
                  )}
                </div>
              </div>
              <ExternalLink size={16} className="shrink-0 text-muted transition-colors group-hover:text-accent-text" />
            </button>
          );
        })}
    </section>
  );
}

function PanelHeader({ title, onViewAll }: { title: string; onViewAll: () => void }) {
  return (
    <div className="mb-3 flex items-center justify-between">
      <h2 className="text-lg font-bold tracking-tight">{title}</h2>
      <button onClick={onViewAll} className="flex items-center gap-0.5 text-sm font-medium text-accent-text hover:underline">
        View all <ChevronRight size={15} />
      </button>
    </div>
  );
}

function RecentSites({ sites }: { sites: Site[] }) {
  const navigate = useAppStore((s) => s.navigate);
  const openNewProject = useAppStore((s) => s.setNewProjectOpen);
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));

  return (
    <Card className="p-6">
      <PanelHeader title="Recent sites" onViewAll={() => navigate("sites")} />
      {sites.length === 0 ? (
        <div className="py-6 text-center text-sm text-muted">
          <Globe size={28} className="mx-auto mb-2 opacity-50" />
          No sites yet. Each folder you create in <span className="font-mono">www</span> becomes a site.
          <div className="mt-3">
            <Button size="sm" variant="primary" onClick={() => openNewProject(true)}>
              New project
            </Button>
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-line">
          {sites.slice(0, 3).map((site) => (
            <li key={site.path} className="flex items-center gap-4 py-3">
              <div className="flex size-11 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent-text">
                <Globe size={20} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="truncate font-semibold">{site.domain}</div>
                <div className="selectable truncate text-sm text-muted">{site.path}</div>
              </div>
              <button
                aria-label={`Open ${site.domain} in browser`}
                onClick={() => openUrl(site.url).catch(onError)}
                className="flex size-8 items-center justify-center rounded-lg text-muted hover:bg-surface-2 hover:text-fg"
              >
                <ExternalLink size={17} />
              </button>
              <Menu
                plain
                items={[
                  { label: "Open in browser", icon: <ExternalLink size={15} />, onSelect: () => openUrl(site.url).catch(onError) },
                  { label: "Open folder", icon: <FolderOpen size={15} />, onSelect: () => openPath(site.path).catch(onError) },
                ]}
              />
            </li>
          ))}
        </ul>
      )}
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
    <Card className="p-6">
      <PanelHeader title="Recent activity" onViewAll={() => navigate("logs")} />
      <ul className="divide-y divide-line">
        {activity.slice(0, 5).map((a) => (
          <li key={a.id} className="flex items-center gap-4 py-2.5 text-sm">
            <span className={clsx("size-2.5 shrink-0 rounded-full", activityDot[a.tone])} />
            <span className="font-mono text-muted tabular-nums">{formatClock(a.at)}</span>
            <span className="min-w-0 flex-1 truncate">{a.text}</span>
            <span className="shrink-0 text-muted">{a.source}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
