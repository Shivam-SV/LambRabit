import { useQuery } from "@tanstack/react-query";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FolderOpen, Globe, RefreshCw } from "lucide-react";
import clsx from "clsx";
import { api } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader } from "../components/ui";

export default function Sites() {
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, refetchInterval: 15_000 });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });

  return (
    <>
      <PageHeader
        title="Sites"
        subtitle="Every folder in www is served as a site."
        actions={
          <Button onClick={() => sites.refetch()} disabled={sites.isFetching} aria-label="Refresh sites">
            <RefreshCw size={15} className={clsx(sites.isFetching && "animate-spin")} />
          </Button>
        }
      />

      {sites.data?.length === 0 ? (
        <Card className="flex flex-col items-center gap-3 px-6 py-14 text-center">
          <div className="flex size-14 items-center justify-center rounded-full bg-accent-soft text-accent-text">
            <Globe size={26} />
          </div>
          <div className="text-lg font-semibold">No sites yet</div>
          <p className="max-w-md text-muted">
            Create a folder inside <span className="font-mono">www</span> and put an{" "}
            <span className="font-mono">index.php</span> in it. It shows up here automatically.
          </p>
          <Button variant="primary" onClick={() => paths.data && openPath(paths.data.www).catch(onError)}>
            <FolderOpen size={16} /> Open www
          </Button>
        </Card>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {sites.data?.map((site) => (
            <Card key={site.path} className="flex flex-col gap-4 p-5">
              <div className="flex items-center gap-3">
                <div className="flex size-11 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent-text">
                  <Globe size={20} />
                </div>
                <div className="min-w-0">
                  <div className="truncate font-semibold">{site.name}</div>
                  <div className="selectable truncate font-mono text-xs text-muted">{site.url}</div>
                </div>
              </div>
              <div className="selectable truncate text-sm text-muted" title={site.path}>
                {site.path}
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
          ))}
        </div>
      )}

      <p className="mt-6 text-sm text-muted">
        Pretty domains like <span className="font-mono">myapp.test</span> with HTTPS are coming next.
      </p>
    </>
  );
}
