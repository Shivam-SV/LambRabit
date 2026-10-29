import { useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Clock,
  Code2,
  Copy,
  Database,
  ExternalLink,
  FolderOpen,
  FolderPlus,
  GitBranch,
  Globe,
  Hexagon,
  RefreshCw,
  Search,
  ShieldCheck,
  SquareTerminal,
  type LucideIcon,
} from "lucide-react";
import clsx from "clsx";
import { api, serviceLabels, type Server, type Settings, type Site, type SitesInfo } from "../lib/api";
import { useAppStore } from "../lib/store";
import { toolUrl } from "../lib/tools";
import { formatAgo, useNow } from "../lib/time";
import { Button, Card, PageHeader, Switch, inputClass } from "../components/ui";
import { ShareButton, TunnelRow, useProjectTunnel } from "../components/Tunnels";

/** Badge colors per framework; anything else gets the neutral style. */
const frameworkStyle: Record<string, string> = {
  Laravel: "bg-red-500/12 text-red-600 dark:text-red-400",
  WordPress: "bg-sky-600/12 text-sky-700 dark:text-sky-400",
  Symfony: "bg-zinc-500/15 text-fg",
  Drupal: "bg-blue-500/12 text-blue-600 dark:text-blue-400",
  CodeIgniter: "bg-orange-500/12 text-orange-600 dark:text-orange-400",
  CakePHP: "bg-rose-500/12 text-rose-600 dark:text-rose-400",
  Yii: "bg-emerald-500/12 text-emerald-700 dark:text-emerald-400",
  Slim: "bg-teal-500/12 text-teal-700 dark:text-teal-400",
  PHP: "bg-indigo-500/12 text-indigo-600 dark:text-indigo-400",
  Static: "bg-surface-2 text-muted",
};

type Sort = "recent" | "name";

export default function Sites() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const openNewProject = useAppStore((s) => s.setNewProjectOpen);
  const onError = (e: unknown) => notify("error", String(e));
  const info = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 10_000 });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["sites"] });
  const [query, setQuery] = useState("");
  const [framework, setFramework] = useState<string | null>(null);
  const [sort, setSort] = useState<Sort>("recent");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  const apply = useMutation({ mutationFn: api.applySites, onSuccess: refresh, onError });
  const trust = useMutation({
    mutationFn: api.trustCa,
    onSuccess: (note) => {
      notify(note ? "error" : "info", note ?? "Certificate trusted. Restart your browser if it still shows a warning.");
      refresh();
    },
    onError,
  });

  const data = info.data;
  const os = paths.data?.os;
  const all = useMemo(() => data?.sites ?? [], [data]);
  const frameworks = useMemo(() => [...new Set(all.map((s) => s.framework ?? "Other"))].sort(), [all]);
  const q = query.trim().toLowerCase();
  const shown = all
    .filter((s) => !framework || (s.framework ?? "Other") === framework)
    .filter((s) => !q || s.name.toLowerCase().includes(q) || s.domain.includes(q) || (s.gitBranch ?? "").toLowerCase().includes(q))
    .sort((a, b) => (sort === "name" ? a.name.localeCompare(b.name) : b.modified - a.modified));
  const selectedSite = shown.find((site) => site.path === selectedPath) ?? shown[0];

  return (
    <>
      <PageHeader
        title="Sites"
        subtitle={
          <>
            Every folder in <span className="font-mono">www</span> is served at{" "}
            <span className="font-mono">&lt;folder&gt;.{data?.tld ?? "test"}</span>, sub-domains included.
          </>
        }
        actions={
          <>
            <Button onClick={() => paths.data && openPath(paths.data.www).catch(onError)}>
              <FolderOpen size={16} /> Open www
            </Button>
            <Button onClick={() => openNewProject(true)} title="New project (N)">
              <FolderPlus size={16} /> New project
            </Button>
            <Button onClick={() => info.refetch()} disabled={info.isFetching} aria-label="Refresh sites">
              <RefreshCw size={15} className={clsx(info.isFetching && "animate-spin")} />
            </Button>
          </>
        }
      />

      <div className="mb-5 space-y-3">
        {data?.needsApply && (
          <Banner
            icon={RefreshCw}
            tone="info"
            text="Your sites changed since the web server started. Apply to update domains, certificates and the hosts file."
            action={
              <Button variant="primary" onClick={() => apply.mutate()} disabled={apply.isPending}>
                {apply.isPending ? "Applying…" : "Apply changes"}
              </Button>
            }
          />
        )}
        {data && !data.hostsOk && !data.needsApply && data.sites.length > 0 && (
          <Banner
            icon={AlertTriangle}
            tone="warn"
            text={
              data.manageHosts
                ? "Some domains are not in the hosts file yet. This happens automatically when the web server starts, or update it now (needs administrator approval)."
                : "Some domains are not in the hosts file, and automatic updates are off in Settings."
            }
            action={
              <Button onClick={() => apply.mutate()} disabled={apply.isPending}>
                Update hosts file
              </Button>
            }
          />
        )}
        {data && data.unserved.length > 0 && (
          <Banner
            icon={AlertTriangle}
            tone="warn"
            text={
              <>
                {data.unserved.length === 1 ? "One folder in www isn't served:" : `${data.unserved.length} folders in www aren't served:`}
                <ul className="mt-1 space-y-0.5">
                  {data.unserved.map((u) => (
                    <li key={u.name}>
                      <span className="font-mono font-medium">{u.name}</span> <span className="text-muted">{u.reason}</span>
                    </li>
                  ))}
                </ul>
              </>
            }
          />
        )}
        {data && data.shadowedTools.length > 0 && (
          <Banner
            icon={AlertTriangle}
            tone="warn"
            text={`A folder in www uses ${data.shadowedTools.join(" and ")}, so that address shows your folder instead of the built-in tool. Rename the folder to get the tool back.`}
          />
        )}
        {data?.caTrusted === false && (
          <Banner
            icon={ShieldCheck}
            tone="warn"
            text={
              <>
                Browsers will warn on HTTPS sites until you trust lampRabit's local certificate authority.
                {os === "windows" && " Windows asks you to confirm once."}
                {os === "linux" && " Linux asks for your password once; Chrome and Firefox are covered too."}{" "}
                {os !== "linux" && (
                  <span className="text-muted">Firefox uses its own certificate list; enable "security.enterprise_roots.enabled" there.</span>
                )}
              </>
            }
            action={
              <Button variant="primary" onClick={() => trust.mutate()} disabled={trust.isPending}>
                <ShieldCheck size={16} /> Trust certificate
              </Button>
            }
          />
        )}
      </div>

      {all.length === 0 ? (
        data && (
          <Card className="flex flex-col items-center gap-3 px-6 py-14 text-center">
            <div className="flex size-14 items-center justify-center rounded-full bg-accent-soft text-accent-text">
              <Globe size={26} />
            </div>
            <div className="text-lg font-semibold">No sites yet</div>
            <p className="max-w-md text-muted">
              Create a folder inside <span className="font-mono">www</span> with an <span className="font-mono">index.php</span>, or start from a
              preset: Laravel, Symfony, WordPress, a blank PHP or static site.
            </p>
            <Button variant="primary" onClick={() => openNewProject(true)}>
              <FolderPlus size={16} /> New project
            </Button>
          </Card>
        )
      ) : (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-3">
            <div className="relative min-w-56 flex-1 sm:max-w-sm">
              <Search size={15} className="absolute top-1/2 left-3 -translate-y-1/2 text-muted" aria-hidden />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search projects or branches"
                aria-label="Search projects"
                className={clsx(inputClass, "border-line bg-surface pl-9")}
              />
            </div>
            {frameworks.length > 1 && (
              <div className="flex flex-wrap gap-1" role="radiogroup" aria-label="Framework">
                {[null, ...frameworks].map((f) => (
                  <button
                    key={f ?? "all"}
                    role="radio"
                    aria-checked={framework === f}
                    onClick={() => setFramework(f)}
                    className={clsx(
                      "rounded-lg px-2.5 py-1 text-xs font-medium transition-colors",
                      framework === f ? "bg-surface-2 text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
                    )}
                  >
                    {f ?? "All"}
                  </button>
                ))}
              </div>
            )}
            <label className="ml-auto flex items-center gap-2 text-sm text-muted">
              Sort
              <select value={sort} onChange={(e) => setSort(e.target.value as Sort)} className="h-9 rounded-lg border border-line bg-surface px-2 text-sm text-fg">
                <option value="recent">Recently changed</option>
                <option value="name">Name</option>
              </select>
            </label>
            <span className="text-sm text-muted" aria-live="polite">
              {shown.length} of {all.length}
            </span>
          </div>
          {shown.length === 0 ? (
            <p className="py-10 text-center text-muted">No projects match.</p>
          ) : (
            <div className="grid items-stretch gap-4 lg:grid-cols-[minmax(200px,240px)_minmax(0,1fr)]">
              <nav aria-label="Projects" className="min-w-0 lg:h-[calc(100dvh-23rem)] lg:min-h-64 lg:overflow-y-auto lg:overscroll-contain lg:pr-2">
                <div className="flex items-center justify-between border-b border-line px-2 pb-3">
                  <h2 className="text-sm font-bold">Projects</h2>
                  <span className="text-xs tabular-nums text-muted">{shown.length}</span>
                </div>
                <ul className="divide-y divide-line">
                  {shown.map((site) => {
                    const selected = selectedSite?.path === site.path;
                    return (
                      <li key={site.path}>
                        <button
                          type="button"
                          aria-current={selected ? "true" : undefined}
                          onClick={() => setSelectedPath(site.path)}
                          className={clsx(
                            "relative w-full rounded-lg px-3 py-3.5 text-left transition-colors",
                            selected ? "bg-surface-2 text-fg" : "text-fg hover:bg-surface-2/70",
                          )}
                        >
                          <span className="block truncate text-[15px] font-bold">{site.name}</span>
                          <span className="mt-0.5 block truncate font-mono text-xs text-muted">{site.domain}</span>
                          <span className="mt-1 flex items-center gap-1.5 text-[11px] text-muted">
                            {site.framework ?? "Folder"}<span aria-hidden> · </span>{serviceLabels[site.server]}
                            {site.gitBranch && <><span aria-hidden> · </span><GitBranch size={11} />{site.gitBranch}</>}
                          </span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </nav>
              <div className="min-w-0 lg:h-[calc(100dvh-23rem)] lg:min-h-64 lg:overflow-y-auto lg:overscroll-contain lg:pr-2">
                {selectedSite ? <ProjectCard key={selectedSite.path} site={selectedSite} info={data} /> : null}
              </div>
            </div>
          )}
        </>
      )}
    </>
  );
}

function Banner({ icon: Icon, tone, text, action }: { icon: LucideIcon; tone: "info" | "warn"; text: ReactNode; action?: ReactNode }) {
  return (
    <div
      className={clsx(
        "flex flex-wrap items-center gap-4 rounded-2xl border px-5 py-3.5",
        tone === "info" ? "border-accent/25 bg-accent-soft" : "border-warn/30 bg-warn/10",
      )}
    >
      <Icon size={20} className={tone === "info" ? "text-accent-text" : "text-warn"} />
      <div className="min-w-60 flex-1 text-sm">{text}</div>
      {action}
    </div>
  );
}

function ProjectCard({ site, info }: { site: Site; info?: SitesInfo }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const now = useNow();
  const onError = (e: unknown) => notify("error", String(e));
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const node = useQuery({ queryKey: ["node"], queryFn: api.nodeStatus, enabled: !!site.node });
  const tunnel = useProjectTunnel(site);
  const phpVersions = installed.data?.find((i) => i.runtime === "php")?.versions ?? [];
  const [copied, setCopied] = useState(false);
  const onSuccess = () => {
    for (const key of ["sites", "settings", "services", "php-status"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const toggleSsl = useMutation({ mutationFn: (ssl: boolean) => api.setSiteSsl(site.name, ssl), onSuccess, onError });
  const setPhp = useMutation({ mutationFn: (v: string | null) => api.setSitePhp(site.name, v), onSuccess, onError });
  const setServer = useMutation({ mutationFn: (v: Server | null) => api.setSiteServer(site.name, v), onSuccess, onError });
  const busy = toggleSsl.isPending || setPhp.isPending || setServer.isPending;
  const php = site.php ?? settings.data?.versions.php;
  const nodeMissing = !!site.node && !!node.data?.manager && !node.data.installed.some((v) => v === site.node || v.startsWith(`${site.node}.`));
  const dbUrl = site.database ? databaseUrl(site.database, info, settings.data) : null;
  const copyPath = () =>
    navigator.clipboard.writeText(site.path).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    }, onError);

  return (
    <div className="space-y-4">
      {/* Identity: what it is and whether it's up */}
      <Card className="p-5">
      <div className="flex flex-wrap items-center gap-4">
        <div
          className={clsx("flex size-10 shrink-0 items-center justify-center rounded-xl text-sm font-bold", frameworkStyle[site.framework ?? ""] ?? "bg-accent-soft text-accent-text")}
          aria-hidden
        >
          {(site.framework ?? site.name).charAt(0).toUpperCase()}
        </div>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-lg leading-tight font-bold">{site.name}</h2>
          <button
            onClick={() => openUrl(site.url).catch(onError)}
            className="mt-1 flex max-w-full items-center gap-1.5 font-mono text-sm text-muted hover:text-accent-text"
            title={`Open ${site.url}`}
          >
            <span className="truncate">{site.domain}</span>
            <ExternalLink size={12} className="shrink-0" aria-hidden />
          </button>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="primary" onClick={() => openUrl(site.url).catch(onError)}><ExternalLink size={15} /> Open site</Button>
          {!tunnel && <ShareButton site={site} />}
        </div>
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-1 border-t border-line pt-3">
        <Button size="sm" variant="ghost" onClick={() => api.openInEditor(site.path).catch(onError)}><Code2 size={14} /> Editor</Button>
        <Button size="sm" variant="ghost" onClick={() => api.openTerminal(site.path).catch(onError)}><SquareTerminal size={14} /> Terminal</Button>
        <Button size="sm" variant="ghost" onClick={() => openPath(site.path).catch(onError)}><FolderOpen size={14} /> Open folder</Button>
      </div>
      {tunnel && <div className="mt-3"><TunnelRow tunnel={tunnel} /></div>}

      </Card>

      <div className="space-y-4">
      <Card className="p-5">
      <div className="mb-4 border-b border-line pb-3">
        <h3 className="text-sm font-bold">Project details</h3>
      </div>
      <p className="mt-2 text-sm text-muted">
        {site.framework === "PHP" ? "Plain PHP" : site.framework ?? "Folder"} <span aria-hidden>·</span> PHP {php ?? "—"} <span aria-hidden>·</span> {serviceLabels[site.server]}
        {site.docroot !== site.path && <> <span aria-hidden>·</span> serves public/</>}
      </p>
      <ul className="mt-4 flex flex-wrap gap-x-4 gap-y-3 text-sm text-muted">
        {site.gitBranch && (
          <li className="flex items-center gap-1.5" title="Git branch">
            <GitBranch size={13} aria-hidden /> <span className="font-mono text-fg">{site.gitBranch}</span>
          </li>
        )}
        {site.database && (
          <li className="flex items-center gap-1.5" title="Database from the project's config">
            <Database size={13} aria-hidden />
            {dbUrl ? (
              <button onClick={() => openUrl(dbUrl).catch(onError)} className="font-mono text-fg hover:text-accent-text hover:underline" title="Open in phpMyAdmin">
                {site.database}
              </button>
            ) : (
              <span className="font-mono text-fg">{site.database}</span>
            )}
          </li>
        )}
        {site.node && (
          <li className="flex items-center gap-1.5" title="Node version from .nvmrc">
            <Hexagon size={13} aria-hidden /> Node <span className="font-mono text-fg">{site.node}</span>
            {nodeMissing && <span className="text-warn">(not installed)</span>}
          </li>
        )}
        {site.modified > 0 && (
          <li className="flex items-center gap-1.5">
            <Clock size={13} aria-hidden /> Changed {formatAgo(site.modified, now)}
          </li>
        )}
      </ul>

      <button
        onClick={copyPath}
        className="group mt-4 flex min-w-0 w-full items-center gap-2 rounded-lg border border-transparent bg-surface-2 px-3 py-2 text-left font-mono text-xs text-muted hover:text-fg"
        title="Copy folder path"
      >
        <FolderOpen size={13} className="shrink-0" aria-hidden />
        <span className="selectable min-w-0 flex-1 truncate">{site.path}</span>
        {copied ? <Check size={13} className="shrink-0 text-accent-text" /> : <Copy size={13} className="shrink-0 text-muted transition-colors group-hover:text-accent-text" />}
      </button>
      </Card>

      <Card className="p-5">
        <div className="mb-4 border-b border-line pb-3">
          <h3 className="text-sm font-bold">Local environment</h3>
          <p className="mt-1 text-xs text-muted">Runtime and server settings for this project.</p>
        </div>
        <div className={clsx("mt-4 divide-y divide-line rounded-xl border border-line text-sm", busy && "opacity-60")}>
          <label className="flex items-center justify-between gap-3 px-3 py-2">
            <span>PHP version</span>
            <div className="relative w-44">
            <select
              value={site.php ?? ""}
              disabled={busy}
              onChange={(e) => setPhp.mutate(e.target.value || null)}
              className="w-full appearance-none rounded-md border border-line bg-surface-2 px-2 py-1.5 pr-8 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
            >
              <option value="">Default · {settings.data?.versions.php ?? "none"}</option>
              {phpVersions.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
            <ChevronDown size={14} className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-muted" aria-hidden />
            </div>
          </label>
          {settings.data?.webServer === "both" && (
            <label className="flex items-center justify-between gap-3 px-3 py-2">
              <span>Web server</span>
              <div className="relative w-44">
              <select
                value={site.server}
                disabled={busy}
                onChange={(e) => setServer.mutate(e.target.value as Server)}
                className="w-full appearance-none rounded-md border border-line bg-surface-2 px-2 py-1.5 pr-8 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
              >
                <option value="apache">Apache</option>
                <option value="nginx">Nginx</option>
              </select>
              <ChevronDown size={14} className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-muted" aria-hidden />
              </div>
            </label>
          )}
          <div className="flex items-center justify-between gap-3 px-3 py-2">
            <span>HTTPS</span>
            <Switch checked={site.ssl} onChange={(v) => toggleSsl.mutate(v)} label={`HTTPS for ${site.domain}`} />
          </div>
        </div>
      </Card>
      </div>

    </div>
  );
}

/** phpMyAdmin opened on the project's database, on the default database server. */
function databaseUrl(db: string, info?: SitesInfo, settings?: Settings) {
  const base = toolUrl("phpmyadmin", info, settings?.ports.mailUi);
  return base ? `${base}?server=1&db=${encodeURIComponent(db)}` : null;
}
