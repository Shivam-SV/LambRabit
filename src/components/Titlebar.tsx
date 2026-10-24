import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openPath } from "@tauri-apps/plugin-opener";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Copy, FolderOpen, Minus, Play, Square, SquareTerminal, X } from "lucide-react";
import clsx from "clsx";
import { api } from "../lib/api";
import { useAppStore } from "../lib/store";
import { useAppUpdate } from "../lib/updates";
import Logo from "./Logo";
import { Button } from "./ui";

/**
 * The window's title bar. On Windows and Linux the native frame is off, so this draws
 * the minimize / maximize / close buttons; on macOS the native traffic lights sit on
 * top of it (overlay title bar), so it only leaves room for them.
 */
export default function Titlebar() {
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const update = useAppUpdate((s) => s.available);
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
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
    <header
      data-tauri-drag-region
      className={clsx(
        "flex h-16 shrink-0 items-center gap-3 border-b border-line bg-surface/80 backdrop-blur-md select-none",
        os === "macos" ? "pr-4 pl-20" : "pl-5",
      )}
    >
      <Logo className="pointer-events-none size-9 shrink-0" />
      <div className="pointer-events-none leading-tight">
        <div className="font-bold tracking-tight">
          lamp<span className="text-accent-text">Rabit</span>
        </div>
        <div className="text-xs text-muted">Local dev environment</div>
      </div>

      <div className="ml-auto flex items-center gap-2">
        {update && (
          <button
            onClick={() => navigate("settings")}
            className="rounded-full bg-accent-soft px-3 py-1 text-xs font-semibold text-accent-text hover:brightness-95"
          >
            Update {update.version} available
          </button>
        )}
        <Button size="sm" onClick={() => paths.data && openPath(paths.data.www).catch(onError)} disabled={!paths.data}>
          <FolderOpen size={15} /> Open www
        </Button>
        <Button size="sm" onClick={() => api.openTerminal(null).catch(onError)} aria-label="Open terminal" title="Terminal with php, composer and mariadb">
          <SquareTerminal size={15} />
        </Button>
        {anyRunning ? (
          <Button size="sm" onClick={() => stopAll.mutate()} disabled={stopAll.isPending}>
            <Square size={12} className="fill-current" /> Stop all
          </Button>
        ) : (
          <Button size="sm" variant="primary" onClick={() => startAll.mutate()} disabled={startAll.isPending}>
            <Play size={14} /> Start all
          </Button>
        )}
      </div>

      {os && os !== "macos" && <WindowControls />}
    </header>
  );
}

function WindowControls() {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    win.isMaximized().then(setMaximized);
    const off = win.onResized(() => win.isMaximized().then(setMaximized));
    return () => {
      off.then((f) => f());
    };
  }, [win]);

  const base = "flex h-16 w-12 items-center justify-center text-muted transition-colors";
  return (
    <div className="ml-2 flex self-stretch">
      <button aria-label="Minimize" className={clsx(base, "hover:bg-surface-2 hover:text-fg")} onClick={() => win.minimize()}>
        <Minus size={16} />
      </button>
      <button
        aria-label={maximized ? "Restore" : "Maximize"}
        className={clsx(base, "hover:bg-surface-2 hover:text-fg")}
        onClick={() => win.toggleMaximize()}
      >
        {maximized ? <Copy size={13} className="-scale-x-100" /> : <Square size={13} />}
      </button>
      <button aria-label="Close" className={clsx(base, "hover:bg-[#e81123] hover:text-white")} onClick={() => win.close()}>
        <X size={17} />
      </button>
    </div>
  );
}
