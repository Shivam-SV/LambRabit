import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import { Gauge, Globe, Layers, ScrollText, Settings as SettingsIcon } from "lucide-react";
import clsx from "clsx";
import type { LogEvent, StatusEvent } from "./lib/api";
import { useAppStore } from "./lib/store";
import Dashboard from "./pages/Dashboard";
import Logs from "./pages/Logs";
import Settings from "./pages/Settings";
import Placeholder from "./pages/Placeholder";
import Notices from "./components/Notices";

const pages = [
  { id: "dashboard", label: "Dashboard", icon: Gauge },
  { id: "sites", label: "Sites", icon: Globe },
  { id: "versions", label: "Versions", icon: Layers },
  { id: "logs", label: "Logs", icon: ScrollText },
  { id: "settings", label: "Settings", icon: SettingsIcon },
] as const;

type PageId = (typeof pages)[number]["id"];

function useBackendEvents() {
  const queryClient = useQueryClient();
  const pushLog = useAppStore((s) => s.pushLog);
  const notify = useAppStore((s) => s.notify);

  useEffect(() => {
    const unlisten = [
      listen<StatusEvent>("service-status", ({ payload }) => {
        queryClient.invalidateQueries({ queryKey: ["services"] });
        if (payload.message) notify("error", payload.message);
      }),
      listen<LogEvent>("service-log", ({ payload }) => pushLog(payload.kind, payload.line)),
    ];
    return () => unlisten.forEach((p) => p.then((off) => off()));
  }, [queryClient, pushLog, notify]);
}

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
  useBackendEvents();

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col border-r border-zinc-200 bg-white px-3 py-4 dark:border-zinc-800 dark:bg-zinc-900">
        <div className="mb-6 px-2">
          <div className="text-lg font-semibold tracking-tight">lampRabit</div>
          <div className="text-xs text-zinc-500">Local dev environment</div>
        </div>
        <ul className="space-y-1">
          {pages.map(({ id, label, icon: Icon }) => (
            <li key={id}>
              <button
                onClick={() => setPage(id)}
                className={clsx(
                  "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-sm transition-colors",
                  page === id
                    ? "bg-orange-50 font-medium text-orange-700 dark:bg-orange-500/10 dark:text-orange-400"
                    : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800",
                )}
              >
                <Icon size={16} />
                {label}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <main className="min-w-0 flex-1 overflow-y-auto">
        {page === "dashboard" && <Dashboard />}
        {page === "sites" && (
          <Placeholder title="Sites" phase={4} text="Folders in www/ become http(s)://name.test with local SSL." />
        )}
        {page === "versions" && (
          <Placeholder title="Versions" phase={2} text="Download and switch Apache, PHP and MariaDB versions." />
        )}
        {page === "logs" && <Logs />}
        {page === "settings" && <Settings />}
      </main>

      <Notices />
    </div>
  );
}
