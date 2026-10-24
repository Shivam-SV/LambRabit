import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  ExternalLink,
  FolderOpen,
  FolderPlus,
  Globe,
  Lock,
  LockOpen,
  RefreshCw,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";
import clsx from "clsx";
import type { ReactNode } from "react";
import { api, type Server, type Site } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader, Switch } from "../components/ui";

export default function Sites() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const openNewProject = useAppStore((s) => s.setNewProjectOpen);
  const onError = (e: unknown) => notify("error", String(e));
  const info = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 10_000 });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["sites"] });

  const apply = useMutation({ mutationFn: api.applySites, onSuccess: refresh, onError });
  const trust = useMutation({
    mutationFn: api.trustCa,
    onSuccess: () => {
      notify("info", "Certificate trusted. Restart your browser if it still shows a warning.");
      refresh();
    },
    onError,
  });

  const data = info.data;
  const os = paths.data?.os;

  return (
    <>
      <PageHeader
        title="Sites"
        subtitle={
          <>
            Every folder in <span className="font-mono">www</span> is served at{" "}
            <span className="font-mono">&lt;folder&gt;.{data?.tld ?? "test"}</span>, including sub-domains.
          </>
        }
        actions={
          <>
            <Button onClick={() => paths.data && openPath(paths.data.www).catch(onError)}>
              <FolderOpen size={16} /> Open www
            </Button>
            <Button variant="primary" onClick={() => openNewProject(true)}>
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
            text="Your sites changed since Apache started. Apply to update domains, certificates and the hosts file."
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
                ? "Some domains are not in the hosts file yet. This happens automatically when Apache starts, or update it now (needs administrator approval)."
                : "Some domains are not in the hosts file, and automatic updates are off in Settings."
            }
            action={
              <Button onClick={() => apply.mutate()} disabled={apply.isPending}>
                Update hosts file
              </Button>
            }
          />
        )}
        {data?.caTrusted === false && (
          <Banner
            icon={ShieldCheck}
            tone="warn"
            text={
              <>
                Browsers will warn on HTTPS sites until you trust lampRabit's local certificate authority.
                {os === "windows" && " Windows asks you to confirm once."}{" "}
                <span className="text-muted">Firefox uses its own certificate list; enable "security.enterprise_roots.enabled" there.</span>
              </>
            }
            action={
              os === "linux" ? (
                <Button onClick={() => openPath(data.caPath).catch(onError)}>Show certificate</Button>
              ) : (
                <Button variant="primary" onClick={() => trust.mutate()} disabled={trust.isPending}>
                  <ShieldCheck size={16} /> Trust certificate
                </Button>
              )
            }
          />
        )}
      </div>

      {data?.sites.length === 0 ? (
        <Card className="flex flex-col items-center gap-3 px-6 py-14 text-center">
          <div className="flex size-14 items-center justify-center rounded-full bg-accent-soft text-accent-text">
            <Globe size={26} />
          </div>
          <div className="text-lg font-semibold">No sites yet</div>
          <p className="max-w-md text-muted">
            Create a folder inside <span className="font-mono">www</span> and put an{" "}
            <span className="font-mono">index.php</span> in it, or start from a preset.
          </p>
          <Button variant="primary" onClick={() => openNewProject(true)}>
            <FolderPlus size={16} /> New project
          </Button>
        </Card>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {data?.sites.map((site) => <SiteCard key={site.path} site={site} />)}
        </div>
      )}
    </>
  );
}

function Banner({
  icon: Icon,
  tone,
  text,
  action,
}: {
  icon: LucideIcon;
  tone: "info" | "warn";
  text: ReactNode;
  action: ReactNode;
}) {
  return (
    <div
      className={clsx(
        "flex flex-wrap items-center gap-4 rounded-2xl border px-5 py-3.5",
        tone === "info" ? "border-accent/25 bg-accent-soft" : "border-warn/30 bg-warn/10",
      )}
    >
      <Icon size={20} className={tone === "info" ? "text-accent-text" : "text-warn"} />
      <p className="min-w-60 flex-1 text-sm">{text}</p>
      {action}
    </div>
  );
}

function SiteCard({ site }: { site: Site }) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const phpVersions = installed.data?.find((i) => i.runtime === "php")?.versions ?? [];
  const onSuccess = () => {
    for (const key of ["sites", "settings", "services", "php-status"]) queryClient.invalidateQueries({ queryKey: [key] });
  };
  const toggleSsl = useMutation({ mutationFn: (ssl: boolean) => api.setSiteSsl(site.name, ssl), onSuccess, onError });
  const setPhp = useMutation({ mutationFn: (v: string | null) => api.setSitePhp(site.name, v), onSuccess, onError });
  const setServer = useMutation({ mutationFn: (v: Server | null) => api.setSiteServer(site.name, v), onSuccess, onError });
  const busy = toggleSsl.isPending || setPhp.isPending || setServer.isPending;
  const servesPublic = site.docroot !== site.path;

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-center gap-3">
        <div className="flex size-11 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent-text">
          <Globe size={20} />
        </div>
        <div className="min-w-0">
          <div className="truncate font-semibold">{site.domain}</div>
          <div className="selectable flex items-center gap-1 truncate font-mono text-xs text-muted">
            {site.ssl ? <Lock size={11} className="text-accent-text" /> : <LockOpen size={11} />}
            {site.url}
          </div>
        </div>
      </div>

      <div className="text-sm text-muted">
        <div className="selectable truncate" title={site.docroot}>
          {site.path}
        </div>
        {servesPublic && <div className="mt-0.5 text-xs">Serving the public/ folder</div>}
      </div>

      <div className={clsx("divide-y divide-line rounded-xl bg-surface-2 text-sm", busy && "opacity-60")}>
        <label className="flex items-center justify-between gap-3 px-3 py-2">
          <span className="font-medium">PHP</span>
          <select
            value={site.php ?? ""}
            disabled={busy}
            onChange={(e) => setPhp.mutate(e.target.value || null)}
            className="min-w-0 rounded-md border border-line bg-surface px-2 py-1 text-sm"
          >
            <option value="">Default ({settings.data?.versions.php ?? "none"})</option>
            {phpVersions.map((v) => (
              <option key={v} value={v}>
                {v}
              </option>
            ))}
          </select>
        </label>
        {settings.data?.webServer === "both" && (
          <label className="flex items-center justify-between gap-3 px-3 py-2">
            <span className="font-medium">Server</span>
            <select
              value={site.server}
              disabled={busy}
              onChange={(e) => setServer.mutate(e.target.value as Server)}
              className="rounded-md border border-line bg-surface px-2 py-1 text-sm"
            >
              <option value="apache">Apache</option>
              <option value="nginx">Nginx</option>
            </select>
          </label>
        )}
        <div className="flex items-center justify-between px-3 py-2">
          <span className="font-medium">HTTPS</span>
          <Switch checked={site.ssl} onChange={(v) => toggleSsl.mutate(v)} label={`HTTPS for ${site.domain}`} />
        </div>
      </div>

      <div className="mt-auto flex gap-2">
        <Button variant="soft" className="flex-1" onClick={() => openUrl(site.url).catch(onError)}>
          <ExternalLink size={15} /> Open
        </Button>
        <Button onClick={() => openPath(site.path).catch(onError)} aria-label={`Open ${site.name} folder`}>
          <FolderOpen size={15} />
        </Button>
      </div>
    </Card>
  );
}
