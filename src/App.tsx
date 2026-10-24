import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  api,
  runtimeLabels,
  serviceLabels,
  type InstallProgress,
  type RawLogLine,
  type StatusEvent,
} from "./lib/api";
import { useAppStore } from "./lib/store";
import Overview from "./pages/Overview";
import Sites from "./pages/Sites";
import Logs from "./pages/Logs";
import Settings from "./pages/Settings";
import Services from "./pages/Services";
import Notices from "./components/Notices";
import BottomNav from "./components/BottomNav";
import Titlebar from "./components/Titlebar";
import Diagnostics from "./components/Diagnostics";

function useBackendEvents() {
  const queryClient = useQueryClient();
  const { pushLog, loadHistory, notify, setInstall, record } = useAppStore.getState();

  useEffect(() => {
    const unlisten = [
      listen<StatusEvent>("service-status", ({ payload }) => {
        queryClient.invalidateQueries({ queryKey: ["services"] });
        const label = serviceLabels[payload.kind];
        if (payload.state === "running") record(`${label} started`, label, "ok");
        if (payload.state === "stopped") record(`${label} stopped`, label, "muted");
        if (payload.state === "error") record(payload.message ?? `${label} failed to start`, label, "error");
        if (payload.message) notify("error", payload.message);
      }),
      listen("service-health", () => queryClient.invalidateQueries({ queryKey: ["services"] })),
      listen<RawLogLine>("log-line", ({ payload }) => pushLog(payload)),
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
    // Subscribe first, then backfill, so no line falls in between.
    Promise.all(unlisten).then(() => api.logHistory().then(loadHistory)).catch(() => {});
    return () => unlisten.forEach((p) => p.then((off) => off()));
  }, [queryClient, pushLog, loadHistory, notify, setInstall, record]);
}

export default function App() {
  const page = useAppStore((s) => s.page);
  const scroller = useRef<HTMLDivElement>(null);
  useBackendEvents();
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
  }, [page]);

  return (
    <div className="flex h-full flex-col">
      <Titlebar />
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
        <main className="mx-auto max-w-6xl px-6 pt-8 pb-32 sm:px-10">
          {page === "overview" && <Overview />}
          {page === "sites" && <Sites />}
          {page === "services" && <Services />}
          {page === "logs" && <Logs />}
          {page === "settings" && <Settings />}
        </main>
      </div>
      <BottomNav />
      <Notices />
      <Diagnostics />
    </div>
  );
}
