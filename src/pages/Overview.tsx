import { useMutation, useQuery } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  ChevronRight,
  Code2,
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
  Mail,
  Table2,
} from "lucide-react";
import clsx from "clsx";
import { api, runtimeLabels, type ServiceInfo, type ServiceKind, type Site, type SitesInfo } from "../lib/api";
import { useAppStore, type Activity } from "../lib/store";
import { formatClock, formatUptime, useNow } from "../lib/time";
import { Button, Card, Pill } from "../components/ui";
import Menu from "../components/Menu";

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
  "php-fpm": { icon: Code2, tile: "bg-indigo-500/10 text-indigo-500", blurb: "Runs PHP for your local sites" },
  mailpit: { icon: Mail, tile: "bg-sky-500/10 text-sky-500", blurb: "Catches mail sent by your apps" },
};

export default function Overview() {
  const now = useNow();
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 15_000 });

  const hour = new Date(now).getHours();
  const greeting = greetings.find((g) => hour < g.until)!;
  const installed = services.data?.filter((s) => s.installed) ?? [];
  const running = installed.filter((s) => s.state === "running");
  const siteCount = sites.data?.sites.length ?? 0;

  const status =
    installed.length === 0
      ? { tone: "muted", title: "Set up your stack", line: "Install Apache, PHP and MariaDB from Services" }
      : running.length === installed.length
        ? { tone: "ok", title: "All systems running", line: "Your local development environment is running smoothly." }
        : running.length > 0
          ? { tone: "warn", title: "Partially running", line: "Some services are stopped." }
          : { tone: "muted", title: "All services stopped", line: "Start your stack to begin working." };

  return (
    <div className="space-y-6">
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
          )}
        >
          <div className="flex items-center gap-2.5">
            <span
              className={clsx(
                "size-2.5 rounded-full",
                status.tone === "ok" && "bg-ok",
                status.tone === "warn" && "bg-warn",
                status.tone === "muted" && "bg-muted/50",
              )}
            />
            <span
              className={clsx(
                "font-semibold",
                status.tone === "ok" && "text-accent-text",
                status.tone === "warn" && "text-warn",
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

      <section className="grid gap-5 md:grid-cols-2">
        {services.data
          // Optional services only appear once installed.
          ?.filter((s) => s.kind !== "mailpit" || s.installed)
          .map((s) => <ServiceCard key={s.kind} service={s} now={now} sites={sites.data} />)}
      </section>

      <section className="grid gap-5 md:grid-cols-2">
        <RecentSites sites={sites.data?.sites ?? []} />
        <RecentActivity />
      </section>
    </div>
  );
}

function ServiceCard({ service, now, sites }: { service: ServiceInfo; now: number; sites?: SitesInfo }) {
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const onError = (e: unknown) => notify("error", String(e));
  const start = useMutation({ mutationFn: () => api.startService(service.kind), onError });
  const stop = useMutation({ mutationFn: () => api.stopService(service.kind), onError });
  const restart = useMutation({ mutationFn: () => api.restartService(service.kind), onError });
  const meta = serviceMeta[service.kind];
  const running = service.state === "running";
  const busy = service.state === "starting" || service.state === "stopping" || restart.isPending;

  const pill = !service.installed ? (
    <Pill tone="muted">Not installed</Pill>
  ) : running ? (
    <Pill tone="ok">Running</Pill>
  ) : service.state === "stopped" || service.state === "error" ? (
    <Pill tone="muted">Stopped</Pill>
  ) : (
    <Pill tone="warn">{service.state === "starting" ? "Starting" : "Stopping"}</Pill>
  );

  const action =
    service.kind === "apache"
      ? {
          label: "Open in browser",
          icon: ExternalLink,
          run: () => openUrl(`http://localhost${service.port === 80 ? "" : `:${service.port}`}/`).catch(onError),
        }
      : service.kind === "mariadb"
        ? { label: "Open with client", icon: TerminalSquare, run: () => api.openDbClient().catch(onError) }
        : service.kind === "mailpit"
          ? {
              label: "Open inbox",
              icon: ExternalLink,
              run: () => openUrl(sites?.tools.find((t) => t.tool === "mailpit")?.proxy ?? "http://localhost:8025/").catch(onError),
            }
          : null;

  // Database tools served by Apache, opened from the MariaDB card.
  const dbTools =
    service.kind === "mariadb"
      ? (sites?.tools ?? [])
          .filter((t) => t.tool === "adminer" || t.tool === "phpmyadmin")
          .map((t) => ({
            label: `Open ${runtimeLabels[t.tool!]}`,
            icon: <Table2 size={15} />,
            onSelect: () => openUrl(sites?.hostsOk ? t.url : `http://localhost/${t.tool}/`).catch(onError),
          }))
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
            ...dbTools,
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
          <Button variant="soft" className="ml-auto" onClick={action.run} disabled={!running}>
            <action.icon size={16} /> {action.label}
          </Button>
        )}
      </div>
    </Card>
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
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));

  return (
    <Card className="p-6">
      <PanelHeader title="Recent sites" onViewAll={() => navigate("sites")} />
      {sites.length === 0 ? (
        <div className="py-6 text-center text-sm text-muted">
          <Globe size={28} className="mx-auto mb-2 opacity-50" />
          No sites yet. Each folder you create in <span className="font-mono">www</span> becomes a site.
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
