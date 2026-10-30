import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  ArrowLeft,
  Box,
  Boxes,
  Cake,
  Check,
  Code2,
  Droplet,
  ExternalLink,
  FileText,
  Flame,
  FlameKindling,
  FolderCog,
  FolderOpen,
  FolderPlus,
  Layers,
  Loader2,
  Newspaper,
  X,
  Zap,
  type LucideIcon,
} from "lucide-react";
import clsx from "clsx";
import { api, type ProjectProgress, type ProjectTemplate, type Server } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Switch, inputClass } from "./ui";

/** Icons a template can name (`icon:`); anything else gets a box. */
const icons: Record<string, LucideIcon> = {
  code: Code2,
  file: FileText,
  flame: Flame,
  "flame-kindling": FlameKindling,
  boxes: Boxes,
  newspaper: Newspaper,
  cake: Cake,
  droplet: Droplet,
  zap: Zap,
  layers: Layers,
  box: Box,
};

/** Tile colours a template can name (`color:`), spelled out so Tailwind keeps them. */
const tiles: Record<string, string> = {
  red: "bg-red-500/10 text-red-500",
  orange: "bg-orange-500/10 text-orange-500",
  amber: "bg-amber-500/10 text-amber-600",
  sky: "bg-sky-500/10 text-sky-600",
  blue: "bg-blue-500/10 text-blue-600",
  indigo: "bg-indigo-500/10 text-indigo-500",
  pink: "bg-pink-500/10 text-pink-600",
  rose: "bg-rose-500/10 text-rose-600",
  teal: "bg-teal-500/10 text-teal-600",
  zinc: "bg-zinc-500/15 text-fg",
};

/** Folder name → database name: lowercase, dashes become underscores. */
const dbName = (name: string) => name.toLowerCase().replace(/[^a-z0-9_]/g, "_").slice(0, 64);
const validName = (name: string) => /^[A-Za-z0-9][A-Za-z0-9_-]{0,62}$/.test(name);
/** "8.4.27" >= "8.2" */
const atLeast = (version: string, minimum: string) => {
  const [a, b] = [version, minimum].map((v) => v.split(/\D+/).filter(Boolean).map(Number));
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return true;
};

type Phase = { kind: "form" } | { kind: "running" } | { kind: "done"; url: string; note: string | null } | { kind: "failed"; error: string };

export default function NewProject() {
  const open = useAppStore((s) => s.newProjectOpen);
  const setOpen = useAppStore((s) => s.setNewProjectOpen);
  const notify = useAppStore((s) => s.notify);
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings, enabled: open });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes, enabled: open });
  const paths = useQuery({ queryKey: ["paths"], queryFn: api.envPaths, enabled: open });
  const sites = useQuery({ queryKey: ["sites"], queryFn: api.listSites, enabled: open });
  // Read every time it opens, so templates you just added show up.
  const catalog = useQuery({ queryKey: ["templates"], queryFn: api.listTemplates, enabled: open, staleTime: 0 });

  const [name, setName] = useState("");
  const [templateId, setTemplateId] = useState("blank");
  const [php, setPhp] = useState<string | null>(null);
  const [server, setServer] = useState<Server | null>(null);
  const [ssl, setSsl] = useState(true);
  const [withDb, setWithDb] = useState(false);
  const [db, setDb] = useState("");
  const [dbEdited, setDbEdited] = useState(false);
  const [variables, setVariables] = useState<Record<string, string>>({});
  const [optional, setOptional] = useState<Record<string, boolean>>({});
  const [approved, setApproved] = useState(false);
  const [phase, setPhase] = useState<Phase>({ kind: "form" });
  const [steps, setSteps] = useState<string[]>([]);
  const [output, setOutput] = useState<string[]>([]);
  const outputEnd = useRef<HTMLDivElement>(null);

  const templates = catalog.data?.templates ?? [];
  const template: ProjectTemplate | undefined = templates.find((t) => t.id === templateId) ?? templates[0];
  const phpVersions = installed.data?.find((i) => i.runtime === "php")?.versions ?? [];
  const hasDatabase = !!installed.data?.some((i) => (i.runtime === "mariadb" || i.runtime === "mysql") && i.active);
  const dbLabel = settings.data?.defaultDatabase === "mysql" ? "MySQL" : "MariaDB";
  const taken = sites.data?.sites.some((s) => s.name.toLowerCase() === name.toLowerCase());
  const nameError = name && !validName(name) ? "Letters, numbers, dashes and underscores; start with a letter or number." : taken ? "A folder with this name already exists in www." : null;
  const needsPhp = !!template?.usesPhp && phpVersions.length === 0;
  const phpInUse = php ?? settings.data?.versions.php ?? null;
  const phpTooOld = !!template?.minPhp && !!phpInUse && !atLeast(phpInUse, template.minPhp);
  const dbUse = template?.database ?? "none";
  const dbMissing = dbUse === "required" && !hasDatabase;
  const tld = settings.data?.tld ?? "test";
  const domain = `${(name || "my-project").toLowerCase().replace(/_/g, "-")}.${tld}`;
  // Risky steps count only when they'll run (optional ones can be unticked).
  const risks = (template?.risks ?? []).filter((r) => {
    const opt = template?.optionalSteps.find((o) => o.id === r.step);
    return !opt || (optional[opt.id] ?? opt.default);
  });

  // Reset when reopened; follow the HTTPS default.
  useEffect(() => {
    if (!open) return;
    setName("");
    setTemplateId("blank");
    setPhp(null);
    setServer(null);
    setSsl(settings.data?.httpsByDefault ?? true);
    setDb("");
    setDbEdited(false);
    setPhase({ kind: "form" });
    setSteps([]);
    setOutput([]);
  }, [open, settings.data?.httpsByDefault]);

  // A different template brings its own defaults.
  useEffect(() => {
    if (!template) return;
    setVariables(Object.fromEntries(template.variables.map((v) => [v.id, v.default ?? (v.type === "choice" ? v.options[0] : v.type === "bool" ? "false" : "")])));
    setOptional(Object.fromEntries(template.optionalSteps.map((o) => [o.id, o.default])));
    setApproved(false);
    setWithDb(template.database === "required" || (template.database === "optional" && hasDatabase));
  }, [template?.id, template?.source, hasDatabase]);

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

  const usesDb = withDb && dbUse !== "none";
  const canCreate = useMemo(
    () =>
      !!template &&
      validName(name) &&
      !taken &&
      !needsPhp &&
      !phpTooOld &&
      !dbMissing &&
      (!usesDb || /^[A-Za-z0-9_]{1,64}$/.test(db)) &&
      (risks.length === 0 || approved),
    [template, name, taken, needsPhp, phpTooOld, dbMissing, usesDb, db, risks.length, approved],
  );

  if (!open) return null;
  const running = phase.kind === "running";

  const create = async () => {
    if (!template) return;
    setPhase({ kind: "running" });
    setSteps([]);
    setOutput([]);
    try {
      const created = await api.createProject({
        name,
        template: template.id,
        php: template.usesPhp ? php : null,
        server,
        ssl,
        database: usesDb ? db : null,
        variables,
        optional,
        approveRisky: approved,
      });
      setPhase({ kind: "done", url: created.url, note: created.note });
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
              <input autoFocus className={inputClass} placeholder="my-project" value={name} onChange={(e) => setName(e.target.value.trim())} />
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
              {catalog.isLoading && <p className="text-sm text-muted">Loading templates…</p>}
              {catalog.error && <p className="text-sm text-danger-text">{String(catalog.error)}</p>}
              <div className="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
                {templates.map((t) => {
                  const Icon = icons[t.icon ?? ""] ?? Box;
                  return (
                    <button
                      key={t.id}
                      onClick={() => setTemplateId(t.id)}
                      aria-pressed={template?.id === t.id}
                      className={clsx(
                        "flex items-start gap-3 rounded-xl border p-3 text-left transition-colors",
                        template?.id === t.id ? "border-accent bg-accent-soft" : "border-line hover:border-accent/40",
                      )}
                    >
                      <div className={clsx("flex size-9 shrink-0 items-center justify-center rounded-lg", tiles[t.color ?? ""] ?? "bg-surface-2 text-fg")}>
                        <Icon size={18} />
                      </div>
                      <div className="min-w-0">
                        <div className="flex items-center gap-1.5 font-semibold">
                          <span className="truncate">{t.name}</span>
                          {t.source === "user" && <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[10px] font-semibold text-muted uppercase">Yours</span>}
                        </div>
                        <div className="text-xs text-muted">{t.description}</div>
                        {t.duration && <div className="mt-1 text-[11px] text-muted/80">{t.duration}</div>}
                      </div>
                    </button>
                  );
                })}
              </div>
              {(catalog.data?.broken.length ?? 0) > 0 && (
                <div className="mt-3 rounded-xl border border-warn/40 bg-warn/10 p-3 text-xs">
                  <p className="font-semibold text-fg">Some of your templates couldn't be read</p>
                  <ul className="mt-1 space-y-0.5 text-muted">
                    {catalog.data!.broken.map((b) => (
                      <li key={b.file}>
                        <span className="font-mono">{b.file.split(/[\\/]/).pop()}</span>: {b.error}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              {template?.usesPhp && (
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
                  {needsPhp ? (
                    <span className="mt-1.5 block text-xs text-danger-text">Install PHP on the Services page first.</span>
                  ) : phpTooOld ? (
                    <span className="mt-1.5 block text-xs text-danger-text">
                      {template.name} needs PHP {template.minPhp} or newer. Pick another version, or install one from Services → PHP.
                    </span>
                  ) : (
                    template.minPhp && <span className="mt-1.5 block text-xs text-muted">Needs PHP {template.minPhp} or newer.</span>
                  )}
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
              {template?.variables.map((v) => (
                <label key={v.id} className={clsx("block", v.type === "bool" && "sm:col-span-2")}>
                  {v.type === "bool" ? (
                    <span className="flex items-center justify-between gap-4">
                      <span>
                        <span className="block text-sm font-semibold">{v.label}</span>
                        {v.help && <span className="block text-xs text-muted">{v.help}</span>}
                      </span>
                      <Switch checked={variables[v.id] === "true"} onChange={(on) => setVariables((cur) => ({ ...cur, [v.id]: String(on) }))} label={v.label} />
                    </span>
                  ) : (
                    <>
                      <span className="mb-1.5 block text-sm font-semibold">{v.label}</span>
                      {v.type === "choice" ? (
                        <select className={inputClass} value={variables[v.id] ?? ""} onChange={(e) => setVariables((cur) => ({ ...cur, [v.id]: e.target.value }))}>
                          {v.options.map((o) => (
                            <option key={o} value={o}>
                              {o}
                            </option>
                          ))}
                        </select>
                      ) : (
                        <input className={inputClass} value={variables[v.id] ?? ""} onChange={(e) => setVariables((cur) => ({ ...cur, [v.id]: e.target.value }))} />
                      )}
                      {v.help && <span className="mt-1.5 block text-xs text-muted">{v.help}</span>}
                    </>
                  )}
                </label>
              ))}
            </div>

            <div className="divide-y divide-line rounded-xl bg-surface-2">
              <div className="flex items-center justify-between gap-4 px-4 py-3">
                <div>
                  <div className="text-sm font-semibold">HTTPS</div>
                  <div className="text-xs text-muted">A trusted local certificate for {domain}.</div>
                </div>
                <Switch checked={ssl} onChange={setSsl} label="HTTPS" />
              </div>
              {dbUse !== "none" && (
                <div className="px-4 py-3">
                  <div className="flex items-center justify-between gap-4">
                    <div>
                      <div className="text-sm font-semibold">{dbUse === "required" ? "Database" : "Create a database"}</div>
                      <div className={clsx("text-xs", dbMissing ? "text-danger-text" : "text-muted")}>
                        {hasDatabase
                          ? `${dbLabel}, user root without a password. It starts if it isn't running.`
                          : dbUse === "required"
                            ? `${template?.name} needs a database: install MariaDB or MySQL on the Services page first.`
                            : "Install MariaDB or MySQL on the Services page to create one."}
                      </div>
                    </div>
                    {dbUse === "optional" && <Switch checked={withDb} onChange={(v) => hasDatabase && setWithDb(v)} label="Create a database" />}
                  </div>
                  {usesDb && hasDatabase && (
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
              {template?.optionalSteps.map((o) => (
                <label key={o.id} className="flex cursor-pointer items-center gap-3 px-4 py-3">
                  <input
                    type="checkbox"
                    className="size-4 accent-(--accent)"
                    checked={optional[o.id] ?? o.default}
                    onChange={(e) => setOptional((cur) => ({ ...cur, [o.id]: e.target.checked }))}
                  />
                  <span className="text-sm">{o.label}</span>
                </label>
              ))}
            </div>

            {risks.length > 0 && (
              <div className="rounded-xl border border-danger/30 bg-danger-soft p-4 text-sm">
                <p className="flex items-center gap-2 font-semibold text-danger-text">
                  <AlertTriangle size={16} /> This template runs steps that need your OK
                </p>
                <p className="mt-1 text-danger-text/90">It didn't come with lampRabit, so check what these steps do before running them.</p>
                <ul className="mt-2 space-y-2">
                  {risks.map((r) => (
                    <li key={r.step} className="text-danger-text">
                      <span className="block text-xs font-semibold">{r.heading}</span>
                      <code className="selectable mt-0.5 block rounded-md bg-surface/70 px-2 py-1 font-mono text-xs break-all">{r.command}</code>
                      <span className="mt-0.5 block text-xs opacity-80">{r.reasons.join(" · ")}</span>
                    </li>
                  ))}
                </ul>
                <label className="mt-3 flex cursor-pointer items-center gap-2 font-medium text-danger-text">
                  <input type="checkbox" className="size-4 accent-(--danger)" checked={approved} onChange={(e) => setApproved(e.target.checked)} />
                  I've checked these steps; run them
                </label>
              </div>
            )}
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
                <div className="mt-1 text-sm text-muted">{phase.note ?? "If your system asked to update the hosts file, approve it so the domain resolves."}</div>
              </div>
            )}
            {phase.kind === "failed" && (
              <div className="selectable rounded-xl border border-danger/30 bg-danger-soft p-4 text-sm whitespace-pre-wrap text-danger-text">{phase.error}</div>
            )}
          </div>
        )}

        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line px-6 py-4">
          {phase.kind === "form" && (
            <>
              <Button
                variant="ghost"
                className="mr-auto"
                onClick={() => api.openTemplatesFolder().catch((e) => notify("error", String(e)))}
                title={catalog.data ? `Add your own templates (.yaml) in ${catalog.data.folder}` : undefined}
              >
                <FolderCog size={15} /> Your templates
              </Button>
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
