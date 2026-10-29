import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderOpen, Puzzle, RefreshCw } from "lucide-react";
import ExtensionsDialog from "../components/ExtensionsDialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import clsx from "clsx";
import {
  api,
  runtimeLabels,
  type InstalledRuntime,
  type Release,
  type Runtime,
} from "../lib/api";
import { installKey, useAppStore } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { useRuntimeUpdates } from "../lib/updates";
import { runtimeTiles } from "../components/runtimes";
import { Button, Card, IconTile, PageHeader } from "../components/ui";
import NodeCard from "../components/NodeCard";
import { ActiveTile, AvailableList, InstalledTile, InstalledTiles, NoActiveTile, VersionsLayout } from "../components/versions";
import { TunnelsPanel } from "../components/Tunnels";

/** `source` describes the download on Windows; `unix`, where it differs, on Linux and macOS. */
const roles: Record<Runtime, { blurb: string; source: string; unix?: string }> = {
  apache: { blurb: "Serves your sites", source: "Apache Lounge builds", unix: "lampRabit's builds of Apache httpd" },
  nginx: {
    blurb: "Serves your sites",
    source: "Official nginx.org Windows builds (PGP-signed, no checksum)",
    unix: "lampRabit's builds of nginx.org releases",
  },
  php: {
    blurb: "Runs your code",
    source: "Thread-safe builds from windows.php.net",
    unix: "lampRabit's static builds with common extensions built in, Redis and Memcached clients included",
  },
  mariadb: { blurb: "Stores your data", source: "Official MariaDB releases" },
  mysql: { blurb: "Stores your data", source: "Official MySQL releases; runs next to MariaDB on its own port" },
  mailpit: { blurb: "Catches outgoing mail", source: "Local SMTP server with a web inbox" },
  adminer: { blurb: "Database admin", source: "A single-file database manager" },
  phpmyadmin: { blurb: "Database admin", source: "The classic MySQL/MariaDB web interface" },
  ngrok: { blurb: "Public tunnels", source: "Share sites on a public URL (free account needed)" },
  redis: {
    blurb: "Cache and queues",
    source: "Real Redis from the community redis-windows build (Redis has no official Windows build), with redis-cli",
    unix: "lampRabit's builds of Redis, with redis-cli",
  },
  cloudflared: { blurb: "Public tunnels", source: "Cloudflare quick tunnels, no account needed" },
  composer: { blurb: "PHP packages", source: "PHP's package manager; run composer from the lampRabit terminal" },
  phpredisadmin: { blurb: "Redis admin", source: "Browse and edit Redis keys in the browser, like phpMyAdmin" },
  memcached: {
    blurb: "Memory cache",
    source: "nono303's Windows build of Memcached (no official one exists), checked and repackaged by lampRabit",
    unix: "lampRabit's builds of Memcached",
  },
};

const versionKey = (v: string) => v.split(".").map(Number);
const newestFirst = (a: string, b: string) => {
  const [x, y] = [versionKey(a), versionKey(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
  }
  return 0;
};

/** The page's groups, in order; ids are the jump-link anchors. */
const groups: { id: string; title: string; blurb: string }[] = [
  { id: "web", title: "Web server and PHP", blurb: "Apache or Nginx serve your sites; PHP runs them. Install several PHP versions and pick one per site." },
  { id: "databases", title: "Databases", blurb: "MariaDB and MySQL can run side by side. The default one is set in Settings → Databases." },
  { id: "tools", title: "Developer tools", blurb: "Optional extras: package managers, mail catching, database and cache tools." },
  { id: "sharing", title: "Sharing", blurb: "Put a project or any local port on a public URL." },
];

export default function Services() {
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const find = (r: Runtime) => installed.data?.find((i) => i.runtime === r);
  const card = (r: Runtime, extra?: { openUrl?: string; note?: string; action?: ReactNode }) => (
    <RuntimeCard key={r} runtime={r} installed={find(r)} {...extra} />
  );
  const defaultDb = settings.data?.defaultDatabase ?? "mariadb";

  return (
    <>
      <PageHeader title="Services" subtitle="Your stack, and every version you can run it on." />
      <nav aria-label="Service groups" className="-mt-2 mb-5 flex flex-wrap gap-2">
        {groups.map((g) => (
          <button
            key={g.id}
            onClick={() => document.getElementById(`services-${g.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" })}
            className="rounded-full border border-line bg-surface px-3 py-1 text-sm text-muted hover:border-accent/40 hover:text-fg"
          >
            {g.title}
          </button>
        ))}
      </nav>

      <div className="space-y-8">
        <Group id="web">{(["apache", "nginx", "php"] as Runtime[]).map((r) => card(r))}</Group>

        <Group id="databases">
          {(["mariadb", "mysql"] as Runtime[]).map((r) =>
            card(r, { note: find(r)?.active && r === defaultDb ? "Default database: new projects and the mysql command use it." : undefined }),
          )}
        </Group>

        <Group id="tools">
          {card("composer", { note: !find("php")?.active ? "Composer runs on PHP: install a PHP version first." : undefined })}
          <NodeCard />
          {card("mailpit", { openUrl: toolUrl("mailpit", sites.data, settings.data?.ports.mailUi) })}
          {card("phpmyadmin", { openUrl: toolUrl("phpmyadmin", sites.data, settings.data?.ports.mailUi) })}
          {card("adminer", { openUrl: toolUrl("adminer", sites.data, settings.data?.ports.mailUi) })}
          {card("redis", {
            note:
              find("redis")?.active && !settings.data?.addToPath
                ? "redis-cli works in lampRabit's terminal; turn on Settings → Terminal → Add lampRabit's commands to PATH to use it anywhere."
                : undefined,
          })}
          {card("phpredisadmin", {
            openUrl: toolUrl("phpredisadmin", sites.data, settings.data?.ports.mailUi),
            note: !find("redis")?.active ? "Shows the data of lampRabit's Redis: install Redis too." : undefined,
          })}
          {card("memcached")}
        </Group>

        <Group id="sharing">
          {(["cloudflared", "ngrok"] as Runtime[]).map((r) => card(r))}
          <TunnelsPanel />
        </Group>
      </div>
    </>
  );
}

function Group({ id, children }: { id: string; children: ReactNode }) {
  const g = groups.find((x) => x.id === id)!;
  return (
    <section id={`services-${id}`} aria-labelledby={`services-${id}-title`} className="scroll-mt-4 space-y-4">
      <div>
        <h2 id={`services-${id}-title`} className="text-xl font-bold tracking-tight">
          {g.title}
        </h2>
        <p className="mt-0.5 text-sm text-muted">{g.blurb}</p>
      </div>
      {children}
    </section>
  );
}

/**
 * One runtime: the version in use as a large tile, other installed versions next to it,
 * and the versions available to download as a list.
 */
function RuntimeCard({
  runtime,
  installed,
  openUrl: url,
  note,
  action,
}: {
  runtime: Runtime;
  installed?: InstalledRuntime;
  /** Shown as an Open button once the tool is installed. */
  openUrl?: string;
  /** A requirement or tip shown under the title. */
  note?: string;
  /** An extra button in the header. */
  action?: ReactNode;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const installs = useAppStore((s) => s.installs);
  const [extOpen, setExtOpen] = useState(false);
  const label = runtimeLabels[runtime];
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const os = paths.data?.os;
  const updates = useRuntimeUpdates().data?.filter((u) => u.runtime === runtime) ?? [];

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
      queryClient.invalidateQueries({ queryKey: ["runtime-updates"] });
    },
    onError,
  });
  const activate = useMutation({ mutationFn: (v: string) => api.setActiveVersion(runtime, v), onSuccess: refresh, onError });
  const remove = useMutation({
    mutationFn: (v: string) => api.removeRuntime(runtime, v),
    onSuccess: refresh,
    onError,
  });

  const have = [...(installed?.versions ?? [])].sort(newestFirst);
  const active = installed?.active ?? null;
  const others = have.filter((v) => v !== active);
  const available = (catalog.data ?? []).filter((r) => !have.includes(r.version)).sort((a, b) => newestFirst(a.version, b.version));
  const folder = (v: string) => (paths.data ? `${paths.data.root}${os === "windows" ? "\\" : "/"}bin${os === "windows" ? "\\" : "/"}${runtime}${os === "windows" ? "\\" : "/"}${v}` : null);
  const progressOf = (v: string) => installs[installKey(runtime, v)];
  const update = (v: string) => updates.find((u) => u.installed === v);
  const source = (os !== "windows" && roles[runtime].unix) || roles[runtime].source;

  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-start gap-4 border-b border-line px-5 py-4">
        <IconTile className={runtimeTiles[runtime].className}>{runtimeTiles[runtime].icon(22)}</IconTile>
        <div className="min-w-0 flex-1 basis-60">
          <h3 className="text-lg font-bold tracking-tight">{label}</h3>
          <p className="text-sm text-muted">
            {roles[runtime].blurb} · {source}
          </p>
          {note && <p className="mt-1 text-xs font-medium text-accent-text">{note}</p>}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {action}
          {runtime === "php" && active && os === "windows" && (
            <Button size="sm" variant="soft" onClick={() => setExtOpen(true)}>
              <Puzzle size={14} /> Extensions
            </Button>
          )}
          {url && active && (
            <Button size="sm" variant="soft" onClick={() => openUrl(url).catch(onError)}>
              <ExternalLink size={14} /> Open
            </Button>
          )}
          <Button size="sm" onClick={() => catalog.refetch()} disabled={catalog.isFetching} aria-label={`Check for new ${label} versions`} title="Check for new versions">
            <RefreshCw size={14} className={clsx(catalog.isFetching && "animate-spin")} />
          </Button>
        </div>
      </div>
      {runtime === "php" && <ExtensionsDialog open={extOpen} onClose={() => setExtOpen(false)} />}

      <VersionsLayout
        active={
          active ? (
            <ActiveTile
              version={active}
              details={
                <>
                  {(catalog.data?.find((r) => r.version === active)?.label || have.length > 1) && (
                    <div className="flex flex-wrap gap-1.5">
                      {catalog.data?.find((r) => r.version === active)?.label && (
                        <span className="rounded bg-surface/70 px-1.5 py-0.5">{catalog.data.find((r) => r.version === active)!.label}</span>
                      )}
                      {have.length > 1 && <span>{have.length} versions installed</span>}
                    </div>
                  )}
                  {update(active) && !progressOf(update(active)!.release.version) && (
                    <button
                      onClick={() => install.mutate(update(active)!.release)}
                      className="text-left font-semibold text-accent-text hover:underline"
                      title="Installs alongside; switch to it when you're ready"
                    >
                      ↑ {update(active)!.release.version} available, install it
                    </button>
                  )}
                </>
              }
            >
              {folder(active) && (
                <Button size="sm" onClick={() => openPath(folder(active)!).catch(onError)}>
                  <FolderOpen size={13} /> Folder
                </Button>
              )}
            </ActiveTile>
          ) : (
            <NoActiveTile text="Pick a version from the list to install it. The first one you install is used automatically." />
          )
        }
      >
        {others.length > 0 && (
          <InstalledTiles>
            {others.map((v) => (
              <InstalledTile
                key={v}
                version={v}
                label={catalog.data?.find((r) => r.version === v)?.label}
                name={`${label} ${v}`}
                busy={activate.isPending || remove.isPending}
                onUse={() => activate.mutate(v)}
                onRemove={() => remove.mutate(v)}
              />
            ))}
          </InstalledTiles>
        )}
        <AvailableList
          title={active ? "Available to download" : "Choose a version"}
          name={label}
          loading={catalog.isLoading}
          error={catalog.isError ? catalog.error : undefined}
          items={available.map((r) => {
            const p = progressOf(r.version);
            const pct = p?.total ? Math.round((p.downloaded / p.total) * 100) : null;
            return {
              version: r.version,
              label: r.label,
              progress: p ? (p.stage === "downloading" ? pct : null) : undefined,
              stage: p?.stage,
            };
          })}
          onInstall={(v) => {
            const release = available.find((r) => r.version === v);
            if (release) install.mutate(release);
          }}
        />
      </VersionsLayout>
    </Card>
  );
}
