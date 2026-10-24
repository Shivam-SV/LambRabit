import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Code2, Database, Download, Feather, RefreshCw, Trash2 } from "lucide-react";
import clsx from "clsx";
import { api, type InstalledRuntime, type Release, type Runtime } from "../lib/api";
import { installKey, useAppStore } from "../lib/store";
import { Button, Card, PageHeader } from "../components/ui";

const runtimes: { id: Runtime; label: string; blurb: string }[] = [
  { id: "apache", label: "Apache", blurb: "HTTP server (Apache Lounge builds)" },
  { id: "php", label: "PHP", blurb: "Thread-safe builds from windows.php.net" },
  { id: "mariadb", label: "MariaDB", blurb: "MySQL-compatible database server" },
];

const tiles: Record<Runtime, { className: string; icon: React.ReactNode }> = {
  apache: { className: "bg-rose-500/10 text-rose-500", icon: <Feather size={22} strokeWidth={1.75} /> },
  php: { className: "bg-indigo-500/10 text-indigo-500", icon: <Code2 size={22} strokeWidth={1.75} /> },
  mariadb: { className: "bg-amber-600/12 text-amber-600", icon: <Database size={22} strokeWidth={1.75} /> },
};

const stageLabel ={ downloading: "Downloading", verifying: "Verifying", extracting: "Extracting" } as const;

export default function Versions() {
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  return (
    <>
      <PageHeader title="Services" subtitle="Install, switch and remove Apache, PHP and MariaDB versions." />
      <div className="space-y-5">
        {runtimes.map((rt) => (
          <RuntimeCard key={rt.id} {...rt} installed={installed.data?.find((i) => i.runtime === rt.id)} />
        ))}
      </div>
    </>
  );
}

function RuntimeCard({
  id,
  label,
  blurb,
  installed,
}: {
  id: Runtime;
  label: string;
  blurb: string;
  installed?: InstalledRuntime;
}) {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const installs = useAppStore((s) => s.installs);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const catalog = useQuery({
    queryKey: ["catalog", id],
    queryFn: () => api.runtimeCatalog(id),
    staleTime: 10 * 60 * 1000,
  });

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["installed"] });
    queryClient.invalidateQueries({ queryKey: ["services"] });
    queryClient.invalidateQueries({ queryKey: ["settings"] });
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
  const activate = useMutation({
    mutationFn: (version: string) => api.setActiveVersion(id, version),
    onSuccess: refresh,
    onError,
  });
  const remove = useMutation({
    mutationFn: (version: string) => api.removeRuntime(id, version),
    onSuccess: () => {
      setConfirmRemove(null);
      refresh();
    },
    onError,
  });

  const versions = installed?.versions ?? [];
  const available = (catalog.data ?? []).filter((r) => !versions.includes(r.version));

  return (
    <Card className="p-6">
      <div className="mb-5 flex items-start gap-4">
        <div className={clsx("flex size-12 shrink-0 items-center justify-center rounded-xl", tiles[id].className)}>
          {tiles[id].icon}
        </div>
        <div className="flex-1">
          <h2 className="text-lg font-bold tracking-tight">{label}</h2>
          <p className="text-sm text-muted">{blurb}</p>
        </div>
        <Button onClick={() => catalog.refetch()} disabled={catalog.isFetching} title="Refresh available versions">
          <RefreshCw size={14} className={clsx(catalog.isFetching && "animate-spin")} />
        </Button>
      </div>

      <div className="grid gap-5 md:grid-cols-2">
        <section>
          <h3 className="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Installed</h3>
          {versions.length === 0 ? (
            <p className="text-sm text-muted">Nothing installed yet.</p>
          ) : (
            <ul className="divide-y divide-line rounded-lg border border-line">
              {versions.map((v) => {
                const active = installed?.active === v;
                return (
                  <li key={v} className="flex items-center gap-2 px-3 py-2 text-sm">
                    <span className="font-mono">{v}</span>
                    {active && (
                      <span className="flex items-center gap-1 rounded bg-accent-soft px-1.5 py-0.5 text-xs font-medium text-accent-text">
                        <Check size={12} /> Active
                      </span>
                    )}
                    <span className="flex-1" />
                    {confirmRemove === v ? (
                      <>
                        <span className="text-xs text-muted">Delete files?</span>
                        <Button variant="danger" onClick={() => remove.mutate(v)} disabled={remove.isPending}>
                          Delete
                        </Button>
                        <Button onClick={() => setConfirmRemove(null)}>Cancel</Button>
                      </>
                    ) : (
                      <>
                        {!active && (
                          <Button onClick={() => activate.mutate(v)} disabled={activate.isPending}>
                            Use
                          </Button>
                        )}
                        <Button onClick={() => setConfirmRemove(v)} title={`Remove ${label} ${v}`}>
                          <Trash2 size={14} />
                        </Button>
                      </>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <section>
          <h3 className="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Available</h3>
          {catalog.isLoading ? (
            <p className="text-sm text-muted">Fetching versions…</p>
          ) : catalog.isError ? (
            <p className="text-sm text-danger-text">{String(catalog.error)}</p>
          ) : available.length === 0 ? (
            <p className="text-sm text-muted">You have the latest versions.</p>
          ) : (
            <ul className="divide-y divide-line rounded-lg border border-line">
              {available.map((r) => {
                const progress = installs[installKey(r.runtime, r.version)];
                return (
                  <li key={r.version} className="px-3 py-2 text-sm">
                    <div className="flex items-center gap-2">
                      <span className="font-mono">{r.version}</span>
                      {r.label && (
                        <span className="rounded bg-surface-2 px-1.5 py-0.5 text-xs text-muted">{r.label}</span>
                      )}
                      <span className="flex-1" />
                      {progress ? (
                        <span className="text-xs text-muted">
                          {stageLabel[progress.stage as keyof typeof stageLabel]}
                          {progress.stage === "downloading" && progress.total
                            ? ` ${Math.round((progress.downloaded / progress.total) * 100)}%`
                            : "…"}
                        </span>
                      ) : (
                        <Button variant="primary" onClick={() => install.mutate(r)}>
                          <Download size={14} /> Install
                        </Button>
                      )}
                    </div>
                    {progress && (
                      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-2">
                        <div
                          className={clsx(
                            "h-full rounded-full bg-accent transition-[width]",
                            progress.stage !== "downloading" && "animate-pulse",
                          )}
                          style={{
                            width:
                              progress.stage === "downloading" && progress.total
                                ? `${(progress.downloaded / progress.total) * 100}%`
                                : "100%",
                          }}
                        />
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </Card>
  );
}
