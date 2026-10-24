import { useMutation, useQuery } from "@tanstack/react-query";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpen, Play, Square } from "lucide-react";
import clsx from "clsx";
import { api, type ServiceInfo, type ServiceState } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader } from "../components/ui";

const stateStyles: Record<ServiceState, { dot: string; label: string }> = {
  running: { dot: "bg-ok", label: "Running" },
  stopping: { dot: "bg-warn animate-pulse", label: "Stopping" },
  stopped: { dot: "bg-muted/60", label: "Stopped" },
  error: { dot: "bg-danger", label: "Error" },
};

export default function Dashboard() {
  const notify = useAppStore((s) => s.notify);
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths });
  const onError = (e: unknown) => notify("error", String(e));

  const startAll = useMutation({
    mutationFn: api.startAll,
    onSuccess: (errors) => errors.forEach((e) => notify("error", e)),
    onError,
  });
  const stopAll = useMutation({ mutationFn: api.stopAll, onError });

  const anyRunning = services.data?.some((s) => s.state === "running");

  return (
    <>
      <PageHeader
        title="Dashboard"
        actions={
          <>
            <Button onClick={() => paths.data && openPath(paths.data.www)} disabled={!paths.data}>
              <FolderOpen size={15} /> www
            </Button>
            {anyRunning ? (
              <Button variant="danger" onClick={() => stopAll.mutate()} disabled={stopAll.isPending}>
                <Square size={14} /> Stop all
              </Button>
            ) : (
              <Button variant="primary" onClick={() => startAll.mutate()} disabled={startAll.isPending}>
                <Play size={14} /> Start all
              </Button>
            )}
          </>
        }
      />

      <div className="grid gap-4 px-8 pb-8 sm:grid-cols-2 xl:grid-cols-3">
        {services.data?.map((s) => <ServiceCard key={s.kind} service={s} />)}
      </div>

      {paths.data && (
        <p className="selectable px-8 pb-8 text-xs text-muted">
          Environment root: <code className="font-mono">{paths.data.root}</code>
        </p>
      )}
    </>
  );
}

function ServiceCard({ service }: { service: ServiceInfo }) {
  const notify = useAppStore((s) => s.notify);
  const onError = (e: unknown) => notify("error", String(e));
  const start = useMutation({ mutationFn: () => api.startService(service.kind), onError });
  const stop = useMutation({ mutationFn: () => api.stopService(service.kind), onError });
  const style = stateStyles[service.state];
  const running = service.state === "running";

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-start justify-between">
        <div>
          <div className="font-medium">{service.label}</div>
          <div className="text-sm text-muted">
            {service.version ? `v${service.version}` : "Not installed"}
          </div>
        </div>
        <span className="flex items-center gap-1.5 text-xs text-muted">
          <span className={clsx("size-2 rounded-full", style.dot)} />
          {style.label}
        </span>
      </div>

      <dl className="grid grid-cols-2 gap-y-1 text-sm">
        <dt className="text-muted">Port</dt>
        <dd className="text-right font-mono">{service.port}</dd>
        <dt className="text-muted">PID</dt>
        <dd className="text-right font-mono">{service.pid ?? "—"}</dd>
      </dl>

      {running ? (
        <Button variant="secondary" onClick={() => stop.mutate()} disabled={stop.isPending}>
          <Square size={14} /> Stop
        </Button>
      ) : (
        <Button
          variant="primary"
          onClick={() => start.mutate()}
          disabled={!service.installed || start.isPending || service.state === "stopping"}
          title={service.installed ? undefined : "Install this runtime from the Versions page"}
        >
          <Play size={14} /> Start
        </Button>
      )}
    </Card>
  );
}
