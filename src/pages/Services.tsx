import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDownToLine, Check, ExternalLink, RefreshCw, Trash2 } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import clsx from "clsx";
import {
  api,
  coreRuntimes,
  runtimeLabels,
  toolRuntimes,
  type InstalledRuntime,
  type Release,
  type Runtime,
  type Settings,
  type ServiceInfo,
} from "../lib/api";
import { installKey, useAppStore } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { runtimeTiles } from "../components/runtimes";
import { Button, Card, IconTile, PageHeader } from "../components/ui";
import NodeCard from "../components/NodeCard";

const roles: Record<Runtime, { blurb: string; source: string }> = {
  apache: { blurb: "Serves your sites", source: "Apache Lounge builds" },
  nginx: { blurb: "Serves your sites", source: "Official nginx.org Windows builds (PGP-signed, no checksum)" },
  php: { blurb: "Runs your code", source: "Thread-safe builds from windows.php.net" },
  mariadb: { blurb: "Stores your data", source: "Official MariaDB releases" },
  mailpit: { blurb: "Catches outgoing mail", source: "Local SMTP server with a web inbox" },
  adminer: { blurb: "Database admin", source: "A single-file database manager" },
  phpmyadmin: { blurb: "Database admin", source: "The classic MySQL/MariaDB web interface" },
  ngrok: { blurb: "Public tunnels", source: "Share sites on a public URL (free account needed)" },
  cloudflared: { blurb: "Public tunnels", source: "Cloudflare quick tunnels, no account needed" },
};

const order = coreRuntimes;


const versionKey = (v: string) => v.split(".").map(Number);
const newestFirst = (a: string, b: string) => {
  const [x, y] = [versionKey(a), versionKey(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
  }
  return 0;
};

export default function Services() {
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const find = (r: Runtime) => installed.data?.find((i) => i.runtime === r);

  return (
    <>
      <PageHeader title="Services" subtitle="Your stack, and every version you can run it on." />
      <div className="space-y-5">
        <StackDiagram installed={installed.data ?? []} services={services.data ?? []} settings={settings.data} />
        {order.map((r) => (
          <RuntimeRail key={r} runtime={r} installed={find(r)} />
        ))}

        <div className="pt-4">
          <h2 className="text-xl font-bold tracking-tight">Developer tools</h2>
          <p className="mt-1 text-muted">
            Optional extras. Database tools are served by Apache; Mailpit runs as its own service.
          </p>
        </div>
        <NodeCard />
        {toolRuntimes.map((r) => (
          <RuntimeRail
            key={r}
            runtime={r}
            installed={find(r)}
            openUrl={toolUrl(r, sites.data, settings.data?.ports.mailUi)}
          />
        ))}
      </div>
    </>
  );
}

/** Web server → PHP → MariaDB as connected nodes; the links animate while traffic can flow. */
function StackDiagram({
  installed,
  services,
  settings,
}: {
  installed: InstalledRuntime[];
  services: ServiceInfo[];
  settings?: Settings;
}) {
  const pools = useQuery({ queryKey: ["php-status"], queryFn: api.phpStatus, refetchInterval: 5000 });
  const svc = (kind: string) => services.find((s) => s.kind === kind);
  const active = (r: Runtime) => installed.find((x) => x.runtime === r)?.active ?? null;
  const mode = settings?.webServer ?? "apache";
  const servers = (["apache", "nginx"] as const).filter((s) => (s === "apache" ? mode !== "nginx" : mode !== "apache"));
  const webUp = servers.some((s) => svc(s)?.state === "running");
  const dbUp = svc("mariadb")?.state === "running";
  const phpUp = (pools.data ?? []).some((p) => p.alive > 0);
  const workers = (pools.data ?? []).reduce((n, p) => n + p.alive, 0);

  const nodes: {
    key: string;
    role: string;
    title: string;
    version: ReactNode;
    tile: Runtime;
    up: boolean;
    text: string;
  }[] = [
    {
      key: "web",
      role: "Serves your sites",
      title: servers.map((s) => runtimeLabels[s]).join(" + "),
      version: servers.map((s) => active(s) ?? "—").join(" · "),
      tile: servers[0],
      up: webUp,
      text: servers
        .map((s) => {
          const info = svc(s);
          return info?.state === "running" ? `${runtimeLabels[s]} on ${info.port}` : `${runtimeLabels[s]} stopped`;
        })
        .join(" · "),
    },
    {
      key: "php",
      role: "Runs your code",
      title: "PHP",
      version: (pools.data?.length ? pools.data.map((p) => p.version) : [active("php") ?? "—"]).join(" · "),
      tile: "php",
      up: phpUp,
      text: phpUp ? `${workers} FastCGI worker${workers === 1 ? "" : "s"}` : "Starts with the web server",
    },
    {
      key: "db",
      role: "Stores your data",
      title: "MariaDB",
      version: active("mariadb") ?? "—",
      tile: "mariadb",
      up: dbUp,
      text: dbUp ? `Running on port ${svc("mariadb")?.port}` : "Stopped",
    },
  ];
  const links = [
    { label: "FastCGI", live: webUp && phpUp },
    { label: "mysqli · PDO", live: phpUp && dbUp },
  ];

  return (
    <Card className="overflow-hidden p-6">
      <div className="mb-6 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-lg font-bold tracking-tight">Your stack</h2>
        <p className="text-sm text-muted">Requests flow left to right while services are running.</p>
      </div>
      <div className="flex flex-col items-stretch gap-3 md:flex-row md:items-center">
        {nodes.map((n, i) => (
          <div key={n.key} className="contents">
            <div
              className={clsx(
                "flex flex-1 items-center gap-4 rounded-2xl border p-4 transition-colors",
                n.up ? "border-accent/30 bg-accent-soft/60" : "border-line bg-bg",
              )}
            >
              <IconTile className={clsx("size-14", runtimeTiles[n.tile].className)}>{runtimeTiles[n.tile].icon(26)}</IconTile>
              <div className="min-w-0">
                <div className="text-xs font-medium tracking-wide text-muted uppercase">{n.role}</div>
                <div className="font-bold">{n.title}</div>
                <div className="truncate font-mono text-lg font-semibold tracking-tight">{n.version}</div>
                <div className="mt-0.5 flex items-center gap-1.5 text-xs text-muted">
                  <span className={clsx("size-1.5 shrink-0 rounded-full", n.up ? "bg-ok" : "bg-muted/50")} />
                  <span className="truncate">{n.text}</span>
                </div>
              </div>
            </div>
            {i < links.length && (
              <div className="flex shrink-0 flex-col items-center gap-1.5 md:w-24">
                <span className="text-[11px] font-medium text-muted">{links[i].label}</span>
                <div className={clsx("h-0.5 w-full rounded-full max-md:h-6 max-md:w-0.5", links[i].live ? "flow-line" : "bg-line")} />
              </div>
            )}
          </div>
        ))}
      </div>
    </Card>
  );
}

interface Node {
  version: string;
  release?: Release;
  installed: boolean;
  active: boolean;
}

/** Every known version of one runtime as stops on a rail, newest first. */
function RuntimeRail({
  runtime,
  installed,
  openUrl: url,
}: {
  runtime: Runtime;
  installed?: InstalledRuntime;
  /** Shown as an Open button once the tool is installed. */
  openUrl?: string;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const installs = useAppStore((s) => s.installs);
  const [confirm, setConfirm] = useState<string | null>(null);
  const label = runtimeLabels[runtime];

  const catalog = useQuery({
    queryKey: ["catalog", runtime],
    queryFn: () => api.runtimeCatalog(runtime),
    staleTime: 10 * 60 * 1000,
  });

  const refresh = () => {
    for (const key of ["installed", "services", "settings"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const onError = (e: unknown) => notify("error", String(e));
  const install = useMutation({
    mutationFn: (r: Release) => api.installRuntime(r),
    onSuccess: (_, r) => {
      notify("info", `${label} ${r.version} installed`);
      refresh();
    },
    onError,
  });
  const activate = useMutation({ mutationFn: (v: string) => api.setActiveVersion(runtime, v), onSuccess: refresh, onError });
  const remove = useMutation({
    mutationFn: (v: string) => api.removeRuntime(runtime, v),
    onSuccess: () => {
      setConfirm(null);
      refresh();
    },
    onError,
  });

  const have = installed?.versions ?? [];
  const nodes: Node[] = [...new Set([...have, ...(catalog.data ?? []).map((r) => r.version)])]
    .sort(newestFirst)
    .map((version) => ({
      version,
      release: catalog.data?.find((r) => r.version === version),
      installed: have.includes(version),
      active: installed?.active === version,
    }));

  return (
    <Card className="p-6">
      <div className="mb-5 flex items-start gap-4">
        <IconTile className={runtimeTiles[runtime].className}>{runtimeTiles[runtime].icon(22)}</IconTile>
        <div className="min-w-0 flex-1">
          <h2 className="text-lg font-bold tracking-tight">{label}</h2>
          <p className="text-sm text-muted">
            {roles[runtime].source} · {have.length} installed
            {catalog.data && ` · ${catalog.data.filter((r) => !have.includes(r.version)).length} available`}
          </p>
        </div>
        {url && installed?.active && (
          <Button size="sm" variant="soft" onClick={() => openUrl(url).catch(onError)}>
            <ExternalLink size={14} /> Open
          </Button>
        )}
        <Button size="sm" onClick={() => catalog.refetch()} disabled={catalog.isFetching} aria-label="Check for new versions">
          <RefreshCw size={14} className={clsx(catalog.isFetching && "animate-spin")} />
        </Button>
      </div>

      {catalog.isError && <p className="mb-3 text-sm text-danger-text">{String(catalog.error)}</p>}

      <div className="-mx-6 overflow-x-auto px-6 pb-1">
        <ol className="relative inline-flex gap-3 pt-6">
          {/* the rail */}
          <span className="absolute top-2.75 right-4 left-4 h-0.5 rounded-full bg-line" aria-hidden />
          {catalog.isLoading && nodes.length === 0 && (
            <li className="pt-2 text-sm text-muted">Fetching versions…</li>
          )}
          {nodes.map((n) => {
            const progress = installs[installKey(runtime, n.version)];
            const pct = progress?.total ? Math.round((progress.downloaded / progress.total) * 100) : null;
            return (
              <li key={n.version} className="relative w-40 shrink-0">
                <span
                  className={clsx(
                    "absolute -top-4.5 left-1/2 size-3.5 -translate-x-1/2 rounded-full border-2",
                    n.active
                      ? "border-accent bg-accent ring-4 ring-accent/20"
                      : n.installed
                        ? "border-fg/60 bg-surface"
                        : "border-dashed border-muted/60 bg-surface",
                  )}
                  aria-hidden
                />
                <div
                  className={clsx(
                    "flex h-full flex-col gap-2 rounded-xl border p-3 transition-colors",
                    n.active
                      ? "border-accent/40 bg-accent-soft"
                      : n.installed
                        ? "border-line bg-surface"
                        : "border-dashed border-line bg-bg",
                  )}
                >
                  <div className="flex items-center justify-between gap-1">
                    <span className={clsx("font-mono text-lg font-semibold", !n.installed && "text-muted")}>
                      {n.version}
                    </span>
                    {n.release?.label && (
                      <span className="rounded bg-surface-2 px-1.5 py-0.5 text-[10px] font-medium text-muted">
                        {n.release.label}
                      </span>
                    )}
                  </div>

                  {progress ? (
                    <div className="mt-auto space-y-1.5">
                      <div className="text-xs text-muted capitalize">
                        {progress.stage}
                        {progress.stage === "downloading" && pct !== null && ` ${pct}%`}
                      </div>
                      <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
                        <div
                          className={clsx("h-full rounded-full bg-accent transition-[width]", progress.stage !== "downloading" && "animate-pulse")}
                          style={{ width: progress.stage === "downloading" && pct !== null ? `${pct}%` : "100%" }}
                        />
                      </div>
                    </div>
                  ) : n.active ? (
                    <div className="mt-auto flex items-center gap-1.5 text-sm font-semibold text-accent-text">
                      <Check size={15} /> In use
                    </div>
                  ) : n.installed ? (
                    confirm === n.version ? (
                      <div className="mt-auto flex gap-1.5">
                        <Button size="sm" variant="danger" className="flex-1" onClick={() => remove.mutate(n.version)} disabled={remove.isPending}>
                          Delete
                        </Button>
                        <Button size="sm" onClick={() => setConfirm(null)}>
                          Keep
                        </Button>
                      </div>
                    ) : (
                      <div className="mt-auto flex gap-1.5">
                        <Button size="sm" className="flex-1" onClick={() => activate.mutate(n.version)} disabled={activate.isPending}>
                          Use
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => setConfirm(n.version)} aria-label={`Remove ${label} ${n.version}`}>
                          <Trash2 size={14} />
                        </Button>
                      </div>
                    )
                  ) : (
                    n.release && (
                      <Button size="sm" variant="soft" className="mt-auto" onClick={() => install.mutate(n.release!)}>
                        <ArrowDownToLine size={14} /> Install
                      </Button>
                    )
                  )}
                </div>
              </li>
            );
          })}
        </ol>
      </div>
    </Card>
  );
}
