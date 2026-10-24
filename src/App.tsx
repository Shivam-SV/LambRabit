import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import { Gauge, Globe, Layers, Monitor, Moon, ScrollText, Settings as SettingsIcon, Sun } from "lucide-react";
import clsx from "clsx";
import type { LogEvent, StatusEvent } from "./lib/api";
import { useAppStore } from "./lib/store";
import { useTheme, type Theme } from "./lib/theme";
import Dashboard from "./pages/Dashboard";
import Logs from "./pages/Logs";
import Settings from "./pages/Settings";
import Placeholder from "./pages/Placeholder";
import Notices from "./components/Notices";
import Logo from "./components/Logo";

const pages = [
  { id: "dashboard", label: "Dashboard", icon: Gauge },
  { id: "sites", label: "Sites", icon: Globe },
  { id: "versions", label: "Versions", icon: Layers },
  { id: "logs", label: "Logs", icon: ScrollText },
  { id: "settings", label: "Settings", icon: SettingsIcon },
] as const;

type PageId = (typeof pages)[number]["id"];

const themes: { id: Theme; label: string; icon: typeof Sun }[] = [
  { id: "light", label: "Light", icon: Sun },
  { id: "system", label: "System", icon: Monitor },
  { id: "dark", label: "Dark", icon: Moon },
];

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

function ThemeSwitch() {
  const { theme, setTheme } = useTheme();
  return (
    <div className="flex rounded-md bg-surface-2 p-0.5" role="radiogroup" aria-label="Theme">
      {themes.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          role="radio"
          aria-checked={theme === id}
          title={label}
          onClick={() => setTheme(id)}
          className={clsx(
            "flex flex-1 justify-center rounded py-1 transition-colors",
            theme === id ? "bg-surface text-accent-text shadow-sm" : "text-muted hover:text-fg",
          )}
        >
          <Icon size={14} />
        </button>
      ))}
    </div>
  );
}

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
  useBackendEvents();

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col border-r border-line bg-surface px-3 py-4">
        <div className="mb-6 flex items-center gap-2.5 px-2">
          <Logo className="size-9 shrink-0" />
          <div>
            <div className="font-semibold tracking-tight">
              lamp<span className="text-accent-text">Rabit</span>
            </div>
            <div className="text-xs text-muted">Local dev environment</div>
          </div>
        </div>
        <ul className="space-y-1">
          {pages.map(({ id, label, icon: Icon }) => (
            <li key={id}>
              <button
                onClick={() => setPage(id)}
                className={clsx(
                  "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-sm transition-colors",
                  page === id
                    ? "bg-accent-soft font-medium text-accent-text"
                    : "text-muted hover:bg-surface-2 hover:text-fg",
                )}
              >
                <Icon size={16} />
                {label}
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-auto px-2">
          <ThemeSwitch />
        </div>
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
