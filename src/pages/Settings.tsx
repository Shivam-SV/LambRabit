import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, FolderOpen, Layers, Monitor, Moon, Plug, Save, Settings2, Sun } from "lucide-react";
import { api, runtimeLabels, type EnvPaths, type Runtime, type Settings as SettingsT } from "../lib/api";
import { useAppStore } from "../lib/store";
import { useTheme } from "../lib/theme";
import { runtimeTiles } from "../components/runtimes";
import { Button, IconTile, PageHeader, SectionCard, Segmented, Switch, inputClass } from "../components/ui";

type Os = EnvPaths["os"];

/** `only` limits a field to the platforms where it does anything. */
const portFields: { key: keyof SettingsT["ports"]; label: string; hint: string; only?: Os[] }[] = [
  { key: "http", label: "HTTP", hint: "Apache HTTP port." },
  { key: "https", label: "HTTPS", hint: "Apache HTTPS port (SSL)." },
  { key: "mysql", label: "MariaDB", hint: "MySQL-compatible database port." },
  { key: "phpFpm", label: "PHP-FPM", hint: "PHP FastCGI Process Manager port.", only: ["macos", "linux"] },
];

const runtimeHints = (os: Os | undefined): Record<Runtime, string> => ({
  apache: os === "windows" ? "HTTP server (Apache Lounge builds)" : "HTTP server",
  php: os === "windows" ? "Thread-safe builds from windows.php.net" : "Runs as PHP-FPM",
  mariadb: "MySQL-compatible database server",
});

const hostsFile: Record<Os, string> = {
  windows: String.raw`C:\Windows\System32\drivers\etc\hosts`,
  macos: "/etc/hosts",
  linux: "/etc/hosts",
};

type Versions = Record<Runtime, string | null>;

export default function Settings() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const navigate = useAppStore((s) => s.navigate);
  const { theme, setTheme } = useTheme();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
  const [draft, setDraft] = useState<SettingsT | null>(null);
  const [versions, setVersions] = useState<Versions | null>(null);

  const activeVersions = useMemo<Versions | null>(
    () =>
      installed.data
        ? (Object.fromEntries(installed.data.map((i) => [i.runtime, i.active])) as Versions)
        : null,
    [installed.data],
  );

  useEffect(() => {
    if (settings.data) setDraft(settings.data);
  }, [settings.data]);
  useEffect(() => {
    if (activeVersions) setVersions(activeVersions);
  }, [activeVersions]);

  const dirty =
    !!draft &&
    !!settings.data &&
    (JSON.stringify({ ...draft, versions: null, sites: null }) !==
      JSON.stringify({ ...settings.data, versions: null, sites: null }) ||
      JSON.stringify(versions) !== JSON.stringify(activeVersions));

  const save = useMutation({
    mutationFn: async () => {
      if (!draft || !settings.data) return;
      // Versions are switched separately so running services restart onto them, and
      // per-site options are owned by the Sites page, so both come from the latest copy.
      const latest = await api.getSettings();
      await api.saveSettings({ ...draft, versions: latest.versions, sites: latest.sites });
      for (const runtime of Object.keys(versions ?? {}) as Runtime[]) {
        const version = versions?.[runtime];
        if (version && version !== activeVersions?.[runtime]) await api.setActiveVersion(runtime, version);
      }
    },
    onSuccess: () => {
      for (const key of ["settings", "installed", "services", "paths", "sites"]) {
        queryClient.invalidateQueries({ queryKey: [key] });
      }
      notify("info", "Settings saved. Restart running services to apply port changes.");
    },
    onError: (e) => notify("error", String(e)),
  });

  const browse = async () => {
    const dir = await open({ directory: true, defaultPath: draft?.rootDir, title: "Choose environment root" });
    if (typeof dir === "string" && draft) setDraft({ ...draft, rootDir: dir });
  };

  if (!draft || !versions) return null;

  return (
    <>
      <PageHeader
        title="Settings"
        subtitle="Configure your local environment."
        actions={
          <div className="flex items-center gap-4">
            {dirty ? (
              <span className="flex items-center gap-2 text-sm text-warn">
                <span className="size-2 rounded-full bg-warn" /> Unsaved changes
              </span>
            ) : (
              <span className="flex items-center gap-2 text-sm text-muted">
                <CheckCircle2 size={17} className="text-accent-text" /> All changes saved
              </span>
            )}
            <Button size="lg" variant="primary" onClick={() => save.mutate()} disabled={!dirty || save.isPending}>
              <Save size={17} /> Save changes
            </Button>
          </div>
        }
      />

      <div className="grid gap-5 lg:grid-cols-2">
        <SectionCard icon={<Settings2 size={22} />} title="General" subtitle="Basic configuration for your local environment.">
          <div className="space-y-5">
            <Field label="Environment root" hint="Holds runtimes, configs, www, databases and logs.">
              <div className="flex gap-2">
                <input
                  className={inputClass}
                  value={draft.rootDir}
                  onChange={(e) => setDraft({ ...draft, rootDir: e.target.value })}
                />
                <Button className="h-auto" onClick={browse}>
                  <FolderOpen size={16} /> Browse…
                </Button>
              </div>
            </Field>
            <Field label="Site domain suffix" hint={`Folders in www/ are served as <name>.${draft.tld || "test"}`}>
              <input
                className={inputClass}
                value={draft.tld}
                onChange={(e) => setDraft({ ...draft, tld: e.target.value.replace(/^\.+/, "") })}
              />
            </Field>
            <Toggle
              label="HTTPS by default"
              hint="Sites use HTTPS with a locally trusted certificate unless you turn it off for a site."
              checked={draft.httpsByDefault}
              onChange={(v) => setDraft({ ...draft, httpsByDefault: v })}
            />
            {draft.tld === "localhost" ? (
              <p className="text-xs text-muted">
                .localhost domains resolve on their own, so the hosts file is left untouched.
              </p>
            ) : (
              <Toggle
                label="Update hosts file automatically"
                hint={`Adds your sites to ${os ? hostsFile[os] : "the hosts file"} when Apache starts. Your system asks for administrator approval when the list changes.`}
                checked={draft.manageHosts}
                onChange={(v) => setDraft({ ...draft, manageHosts: v })}
              />
            )}
          </div>
        </SectionCard>

        <SectionCard icon={<Plug size={22} />} title="Ports" subtitle="Network ports used by local services.">
          <div className="grid gap-x-5 gap-y-5 sm:grid-cols-2">
            {portFields.filter((f) => !f.only || (os && f.only.includes(os))).map(({ key, label, hint }) => (
              <Field key={key} label={label} hint={hint}>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  className={inputClass}
                  value={draft.ports[key]}
                  onChange={(e) => setDraft({ ...draft, ports: { ...draft.ports, [key]: Number(e.target.value) } })}
                />
              </Field>
            ))}
          </div>
        </SectionCard>

        <SectionCard icon={<Layers size={22} />} title="Active versions" subtitle="Which installed version each service runs.">
          <div className="space-y-4">
            {(["apache", "php", "mariadb"] as Runtime[]).map((runtime) => {
              const options = installed.data?.find((i) => i.runtime === runtime)?.versions ?? [];
              return (
                <div key={runtime} className="flex items-start gap-4">
                  <IconTile className={`size-10 rounded-xl ${runtimeTiles[runtime].className}`}>
                    {runtimeTiles[runtime].icon(20)}
                  </IconTile>
                  <div className="w-20 pt-2 font-semibold">{runtimeLabels[runtime]}</div>
                  <div className="min-w-0 flex-1">
                    {options.length > 0 ? (
                      <select
                        className={inputClass}
                        value={versions[runtime] ?? ""}
                        onChange={(e) => setVersions({ ...versions, [runtime]: e.target.value })}
                      >
                        {options.map((v) => (
                          <option key={v}>{v}</option>
                        ))}
                      </select>
                    ) : (
                      <button
                        onClick={() => navigate("services")}
                        className="w-full rounded-lg border border-dashed border-line px-3 py-2 text-left text-sm text-muted hover:border-accent hover:text-accent-text"
                      >
                        Not installed. Install from Services
                      </button>
                    )}
                    <p className="mt-1 text-xs text-muted">{runtimeHints(os)[runtime]}</p>
                  </div>
                </div>
              );
            })}
          </div>
        </SectionCard>

        <SectionCard icon={<Monitor size={22} />} title="Appearance" subtitle="Choose how lampRabit looks on your device.">
          <Segmented
            value={theme}
            onChange={setTheme}
            className="py-1.5"
            options={[
              { value: "system", label: "System", icon: <Monitor size={16} /> },
              { value: "light", label: "Light", icon: <Sun size={16} /> },
              { value: "dark", label: "Dark", icon: <Moon size={16} /> },
            ]}
          />
          <p className="mt-3 text-sm text-muted">
            Automatically match your system theme, or choose a light or dark theme. Applies instantly.
          </p>
        </SectionCard>
      </div>
    </>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-semibold">{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-xs text-muted">{hint}</span>}
    </label>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div>
        <div className="text-sm font-semibold">{label}</div>
        <div className="mt-0.5 text-xs text-muted">{hint}</div>
      </div>
      <Switch checked={checked} onChange={onChange} label={label} />
    </div>
  );
}
