import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  Code2,
  ExternalLink,
  FileText,
  Flame,
  FolderOpen,
  FolderPlus,
  Loader2,
  Newspaper,
  Boxes,
  X,
  type LucideIcon,
} from "lucide-react";
import clsx from "clsx";
import { api, type Preset, type ProjectProgress, type Server } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Switch, inputClass } from "./ui";

const presets: { id: Preset; label: string; blurb: string; note: string; icon: LucideIcon; tile: string; db: boolean }[] = [
  { id: "blank", label: "Blank PHP", blurb: "An index.php to start from.", note: "Instant", icon: Code2, tile: "bg-indigo-500/10 text-indigo-500", db: false },
  { id: "laravel", label: "Laravel", blurb: "Latest Laravel, configured for MariaDB and Mailpit.", note: "Composer · about a minute", icon: Flame, tile: "bg-red-500/10 text-red-500", db: true },
  { id: "symfony", label: "Symfony", blurb: "Symfony skeleton with database and mailer set up.", note: "Composer · about a minute", icon: Boxes, tile: "bg-zinc-500/15 text-fg", db: true },
  { id: "wordpress", label: "WordPress", blurb: "Latest WordPress with wp-config.php ready.", note: "Download · ~30 MB", icon: Newspaper, tile: "bg-sky-500/10 text-sky-600", db: true },
  { id: "static", label: "Static site", blurb: "Plain HTML, no PHP.", note: "Instant", icon: FileText, tile: "bg-amber-500/10 text-amber-600", db: false },
];

/** Folder name → database name: lowercase, dashes become underscores. */
const dbName = (name: string) => name.toLowerCase().replace(/[^a-z0-9_]/g, "_").slice(0, 64);
const validName = (name: string) => /^[A-Za-z0-9][A-Za-z0-9_-]{0,62}$/.test(name);

type Phase = { kind: "form" } | { kind: "running" } | { kind: "done"; url: string } | { kind: "failed"; error: string };

export default function NewProject() {
  const open = useAppStore((s) => s.newProjectOpen);
  const setOpen = useAppStore((s) => s.setNewProjectOpen);
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings, enabled: open });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes, enabled: open });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths, enabled: open });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, enabled: open });

  const [name, setName] = useState("");
  const [preset, setPreset] = useState<Preset>("blank");
  const [php, setPhp] = useState<string | null>(null);
  const [server, setServer] = useState<Server | null>(null);
  const [ssl, setSsl] = useState(true);
  const [withDb, setWithDb] = useState(false);
  const [db, setDb] = useState("");
  const [dbEdited, setDbEdited] = useState(false);
  const [phase, setPhase] = useState<Phase>({ kind: "form" });
  const [steps, setSteps] = useState<string[]>([]);
  const [output, setOutput] = useState<string[]>([]);
  const outputEnd = useRef<HTMLDivElement>(null);

  const phpVersions = installed.data?.find((i) => i.runtime === "php")?.versions ?? [];
  const hasMariadb = !!installed.data?.find((i) => i.runtime === "mariadb")?.active;
  const presetInfo = presets.find((p) => p.id === preset)!;
  const taken = sites.data?.sites.some((s) => s.name.toLowerCase() === name.toLowerCase());
  const nameError = name && !validName(name) ? "Letters, numbers, dashes and underscores; start with a letter or number." : taken ? "A folder with this name already exists in www." : null;
  const needsPhp = preset !== "static" && phpVersions.length === 0;
  const tld = settings.data?.tld ?? "test";
  const domain = `${(name || "my-project").toLowerCase().replace(/_/g, "-")}.${tld}`;

  // Reset when reopened; follow the HTTPS default.
  useEffect(() => {
    if (!open) return;
    setName("");
    setPreset("blank");
    setPhp(null);
    setServer(null);
    setSsl(settings.data?.httpsByDefault ?? true);
    setWithDb(false);
    setDb("");
    setDbEdited(false);
    setPhase({ kind: "form" });
    setSteps([]);
    setOutput([]);
  }, [open, settings.data?.httpsByDefault]);

  useEffect(() => setWithDb(presetInfo.db && hasMariadb), [presetInfo.db, hasMariadb]);
  useEffect(() => {
    if (!dbEdited) setDb(dbName(name));
  }, [name, dbEdited]);

  useEffect(() => {
    if (!open) return;
    const off = listen<ProjectProgress>("project-progress", ({ payload }) => {
      if (payload.step) setSteps((s) => [...s, payload.step!]);
      if (payload.line) setOutput((o) => [...o.slice(-300), payload.line!]);
    });
    return () => {
      off.then((f) => f());
    };
  }, [open]);

  useEffect(() => {
    outputEnd.current?.scrollIntoView({ block: "end" });
  }, [output.length]);

  const canCreate = useMemo(
    () => validName(name) && !taken && !needsPhp && (!withDb || /^[A-Za-z0-9_]{1,64}$/.test(db)),
    [name, taken, needsPhp, withDb, db],
  );

  if (!open) return null;
  const running = phase.kind === "running";

  const create = async () => {
    setPhase({ kind: "running" });
    setSteps([]);
    setOutput([]);
    try {
      const url = await api.createProject({
        name,
        preset,
        php: preset === "static" ? null : php,
        server,
        ssl,
        database: withDb && preset !== "static" ? db : null,
      });
      setPhase({ kind: "done", url });
    } catch (e) {
      setPhase({ kind: "failed", error: String(e) });
    }
    for (const key of ["sites", "settings", "services", "php-status"]) queryClient.invalidateQueries({ queryKey: [key] });
  };

  return (
    <div className="motion-backdrop fixed inset-0 z-30 flex items-center justify-center bg-black/40 p-6" onMouseDown={() => !running && setOpen(false)}>
      <div
        role="dialog"
        aria-label="New project"
        className="motion-dialog flex max-h-full w-full max-w-3xl flex-col rounded-2xl border border-line bg-surface shadow-float"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 border-b border-line px-6 py-4">
          <FolderPlus size={20} className="text-accent-text" />
          <div className="flex-1">
            <h2 className="text-lg font-bold tracking-tight">New project</h2>
            <p className="text-sm text-muted">Creates a folder in www with everything set up.</p>
          </div>
          {!running && (
            <button aria-label="Close" onClick={() => setOpen(false)} className="rounded-lg p-1.5 text-muted hover:bg-surface-2">
              <X size={18} />
            </button>
          )}
        </div>

        {phase.kind === "form" ? (
          <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-6 py-5">
            <label className="block">
              <span className="mb-1.5 block text-sm font-semibold">Project name</span>
              <input
                autoFocus
                className={inputClass}
                placeholder="my-project"
                value={name}
                onChange={(e) => setName(e.target.value.trim())}
              />
              {nameError ? (
                <span className="mt-1.5 block text-xs text-danger-text">{nameError}</span>
              ) : (
                <span className="mt-1.5 block font-mono text-xs text-muted">
                  www/{name || "my-project"} → {ssl ? "https" : "http"}://{domain}
                </span>
              )}
            </label>

            <div>
              <span className="mb-2 block text-sm font-semibold">Start from</span>
              <div className="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
                {presets.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => setPreset(p.id)}
                    aria-pressed={preset === p.id}
                    className={clsx(
                      "flex items-start gap-3 rounded-xl border p-3 text-left transition-colors",
                      preset === p.id ? "border-accent bg-accent-soft" : "border-line hover:border-accent/40",
                    )}
                  >
                    <div className={clsx("flex size-9 shrink-0 items-center justify-center rounded-lg", p.tile)}>
                      <p.icon size={18} />
                    </div>
                    <div className="min-w-0">
                      <div className="font-semibold">{p.label}</div>
                      <div className="text-xs text-muted">{p.blurb}</div>
                      <div className="mt-1 text-[11px] text-muted/80">{p.note}</div>
                    </div>
                  </button>
                ))}
              </div>
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              {preset !== "static" && (
                <label className="block">
                  <span className="mb-1.5 block text-sm font-semibold">PHP version</span>
                  <select className={inputClass} value={php ?? ""} onChange={(e) => setPhp(e.target.value || null)}>
                    <option value="">Default ({settings.data?.versions.php ?? "not installed"})</option>
                    {phpVersions.map((v) => (
                      <option key={v} value={v}>
                        {v}
                      </option>
                    ))}
                  </select>
                  {needsPhp && <span className="mt-1.5 block text-xs text-danger-text">Install PHP on the Services page first.</span>}
                </label>
              )}
              {settings.data?.webServer === "both" && (
                <label className="block">
                  <span className="mb-1.5 block text-sm font-semibold">Web server</span>
                  <select className={inputClass} value={server ?? ""} onChange={(e) => setServer((e.target.value || null) as Server | null)}>
                    <option value="">Default (Apache)</option>
                    <option value="apache">Apache</option>
                    <option value="nginx">Nginx</option>
                  </select>
                </label>
              )}
            </div>

            <div className="divide-y divide-line rounded-xl bg-surface-2">
              <div className="flex items-center justify-between gap-4 px-4 py-3">
                <div>
                  <div className="text-sm font-semibold">HTTPS</div>
                  <div className="text-xs text-muted">A trusted local certificate for {domain}.</div>
                </div>
                <Switch checked={ssl} onChange={setSsl} label="HTTPS" />
              </div>
              {preset !== "static" && (
                <div className="px-4 py-3">
                  <div className="flex items-center justify-between gap-4">
                    <div>
                      <div className="text-sm font-semibold">Create a database</div>
                      <div className="text-xs text-muted">
                        {hasMariadb ? "MariaDB, user root without a password. MariaDB starts if it isn't running." : "Install MariaDB on the Services page to create one."}
                      </div>
                    </div>
                    <Switch checked={withDb} onChange={(v) => hasMariadb && setWithDb(v)} label="Create a database" />
                  </div>
                  {withDb && (
                    <input
                      className={clsx(inputClass, "mt-3 font-mono")}
                      value={db}
                      onChange={(e) => {
                        setDb(e.target.value);
                        setDbEdited(true);
                      }}
                      aria-label="Database name"
                    />
                  )}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
            <ol className="space-y-2">
              {steps.map((step, i) => {
                const current = running && i === steps.length - 1;
                const failed = phase.kind === "failed" && i === steps.length - 1;
                return (
                  <li key={i} className="flex items-center gap-2.5 text-sm">
                    {current ? (
                      <Loader2 size={16} className="animate-spin text-accent-text" />
                    ) : failed ? (
                      <X size={16} className="text-danger-text" />
                    ) : (
                      <Check size={16} className="text-accent-text" />
                    )}
                    <span className={clsx(current && "font-medium")}>{step}</span>
                  </li>
                );
              })}
              {running && steps.length === 0 && (
                <li className="flex items-center gap-2.5 text-sm">
                  <Loader2 size={16} className="animate-spin text-accent-text" /> Preparing…
                </li>
              )}
            </ol>
            {output.length > 0 && (
              <pre className="selectable max-h-56 overflow-auto rounded-xl bg-terminal p-3 font-mono text-[11.5px] leading-relaxed text-slate-300">
                {output.join("\n")}
                <div ref={outputEnd} />
              </pre>
            )}
            {phase.kind === "done" && (
              <div className="rounded-xl border border-accent/30 bg-accent-soft p-4">
                <div className="font-semibold text-accent-text">{name} is ready</div>
                <div className="mt-1 text-sm text-muted">
                  {preset === "wordpress"
                    ? "Open the site to finish the WordPress installation."
                    : "If your system asked to update the hosts file, approve it so the domain resolves."}
                </div>
              </div>
            )}
            {phase.kind === "failed" && (
              <div className="selectable rounded-xl border border-danger/30 bg-danger-soft p-4 text-sm whitespace-pre-wrap text-danger-text">
                {phase.error}
              </div>
            )}
          </div>
        )}

        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line px-6 py-4">
          {phase.kind === "form" && (
            <>
              <span className="mr-auto text-xs text-muted">{presetInfo.note}</span>
              <Button onClick={() => setOpen(false)}>Cancel</Button>
              <Button variant="primary" onClick={create} disabled={!canCreate}>
                <FolderPlus size={16} /> Create project
              </Button>
            </>
          )}
          {running && <span className="text-sm text-muted">Working… you can keep this open.</span>}
          {phase.kind === "failed" && (
            <>
              <span className="mr-auto text-xs text-muted">Nothing was left behind; you can try again.</span>
              <Button onClick={() => setPhase({ kind: "form" })}>
                <ArrowLeft size={16} /> Back
              </Button>
            </>
          )}
          {phase.kind === "done" && (
            <>
              <Button onClick={() => paths.data && openPath(`${paths.data.www}/${name}`)}>
                <FolderOpen size={16} /> Open folder
              </Button>
              <Button onClick={() => setOpen(false)}>Done</Button>
              <Button variant="primary" onClick={() => openUrl(phase.url)}>
                <ExternalLink size={16} /> Open site
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
