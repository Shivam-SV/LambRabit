import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { openPath } from "@tauri-apps/plugin-opener";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderOpen, Play, Square } from "lucide-react";
import {
  api,
  runtimeLabels,
  serviceLabels,
  type InstallProgress,
  type LogEvent,
  type StatusEvent,
} from "./lib/api";
import { useAppStore } from "./lib/store";
import Overview from "./pages/Overview";
import Sites from "./pages/Sites";
import Logs from "./pages/Logs";
import Settings from "./pages/Settings";
import Versions from "./pages/Versions";
import Notices from "./components/Notices";
import Logo from "./components/Logo";
import BottomNav from "./components/BottomNav";
import { Button } from "./components/ui";

function useBackendEvents() {
  const queryClient = useQueryClient();
  const { pushLog, notify, setInstall, record } = useAppStore.getState();

  useEffect(() => {
    const unlisten = [
      listen<StatusEvent>("service-status", ({ payload }) => {
        queryClient.invalidateQueries({ queryKey: ["services"] });
        const label = serviceLabels[payload.kind];
        if (payload.state === "running") record(`${label} started`, label, "ok");
        if (payload.state === "stopped") record(`${label} stopped`, label, "muted");
        if (payload.state === "error") record(payload.message ?? `${label} failed`, label, "error");
        if (payload.message) notify("error", payload.message);
      }),
      listen<LogEvent>("service-log", ({ payload }) => pushLog(payload.kind, payload.line)),
      listen<InstallProgress>("install-progress", ({ payload }) => {
        setInstall(payload);
        if (payload.stage === "done" || payload.stage === "error") {
          queryClient.invalidateQueries({ queryKey: ["installed"] });
          queryClient.invalidateQueries({ queryKey: ["services"] });
        }
        if (payload.stage === "done") {
          record(`${runtimeLabels[payload.runtime]} ${payload.version} installed`, "Services", "info");
        }
      }),
    ];
    return () => unlisten.forEach((p) => p.then((off) => off()));
  }, [queryClient, pushLog, notify, setInstall, record]);
}

function AppHeader() {
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
    <header className="flex flex-wrap items-center justify-between gap-4 pt-7 pb-8">
      <div className="flex items-center gap-3.5">
        <Logo className="size-12 shrink-0" />
        <div>
          <div className="text-xl leading-tight font-bold tracking-tight">
            lamp<span className="text-accent-text">Rabit</span>
          </div>
          <div className="text-sm text-muted">Local dev environment</div>
        </div>
      </div>
      <div className="flex gap-2.5">
        <Button size="lg" onClick={() => paths.data && openPath(paths.data.www).catch(onError)} disabled={!paths.data}>
          <FolderOpen size={18} /> Open www
        </Button>
        {anyRunning ? (
          <Button size="lg" onClick={() => stopAll.mutate()} disabled={stopAll.isPending}>
            <Square size={15} className="fill-current" /> Stop all
          </Button>
        ) : (
          <Button size="lg" variant="primary" onClick={() => startAll.mutate()} disabled={startAll.isPending}>
            <Play size={17} /> Start all
          </Button>
        )}
      </div>
    </header>
  );
}

export default function App() {
  const page = useAppStore((s) => s.page);
  const scroller = useRef<HTMLDivElement>(null);
  useBackendEvents();
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
  }, [page]);

  return (
    <div ref={scroller} className="h-full overflow-y-auto">
      <div className="mx-auto max-w-6xl px-6 pb-32 sm:px-10">
        <AppHeader />
        <main>
          {page === "overview" && <Overview />}
          {page === "sites" && <Sites />}
          {page === "services" && <Versions />}
          {page === "logs" && <Logs />}
          {page === "settings" && <Settings />}
        </main>
      </div>
      <BottomNav />
      <Notices />
    </div>
  );
}
