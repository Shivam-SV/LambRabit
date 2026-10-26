import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  CheckCircle2,
  CircleSlash,
  ExternalLink,
  Feather,
  FolderOpen,
  Layers,
  LifeBuoy,
  Mail,
  Monitor,
  Moon,
  Plug,
  ServerCog,
  Save,
  Settings2,
  Share2,
  RefreshCw,
  Sparkles,
  Stethoscope,
  Sun,
  Wind,
} from "lucide-react";
import clsx from "clsx";
import { api, runtimeLabels, type EnvPaths, type PortStatus, type Runtime, type Settings as SettingsT } from "../lib/api";
import { useAppStore } from "../lib/store";
import { useTheme } from "../lib/theme";
import { useMotion } from "../lib/motion";
import { installAppUpdate, useAppUpdate } from "../lib/updates";
import { getVersion } from "@tauri-apps/api/app";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { runtimeTiles } from "../components/runtimes";
import { Button, IconTile, PageHeader, SectionCard, Segmented, Switch, inputClass } from "../components/ui";

type Os = EnvPaths["os"];

/** `only` limits a field to the platforms where it does anything; `both` to when both web servers run. */
const portFields: { key: keyof SettingsT["ports"]; label: string; hint: string; only?: Os[]; both?: boolean }[] = [
  { key: "http", label: "HTTP", hint: "Main web server HTTP port." },
  { key: "https", label: "HTTPS", hint: "Main web server HTTPS port (SSL)." },
  { key: "nginxHttp", label: "Nginx HTTP", hint: "Nginx's own HTTP port next to Apache.", both: true },
  { key: "nginxHttps", label: "Nginx HTTPS", hint: "Nginx's own HTTPS port next to Apache.", both: true },
  { key: "mysql", label: "MariaDB", hint: "MySQL-compatible database port." },
  { key: "phpFpm", label: "PHP FastCGI ports", hint: "Start of the range PHP workers use: 10 ports per PHP version." },
  { key: "redis", label: "Redis", hint: "Redis-compatible cache (Garnet)." },
  { key: "memcached", label: "Memcached", hint: "System memcached.", only: ["macos", "linux"] },
];

const runtimeHints = (os: Os | undefined): Partial<Record<Runtime, string>> => ({
  apache: os === "windows" ? "HTTP server (Apache Lounge builds)" : "HTTP server",
  nginx: "HTTP server (nginx.org builds)",
  php: "Default for sites without their own version",
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
  const { motion, setMotion } = useMotion();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const installed = useQuery({ queryKey: ["installed"], queryFn: api.installedRuntimes });
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
  const ports = useQuery({ queryKey: ["port-status"], queryFn: api.portStatus, refetchInterval: 5000 });
  const openDiagnostics = useAppStore((s) => s.setDiagnosticsOpen);
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

      <div className="motion-stagger grid gap-5 lg:grid-cols-2">
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
            <Toggle
              label="Keep running in the tray when closed"
              hint="Closing the window leaves lampRabit and your services running; quit from the tray icon."
              checked={draft.closeToTray}
              onChange={(v) => setDraft({ ...draft, closeToTray: v })}
            />
            <AutostartToggle />
            <Toggle
              label="Add PHP, Composer and MariaDB to PATH"
              hint={
                os === "windows"
                  ? "Run php, composer, mysql and mariadb-dump from any terminal, always at your active versions. Applies to terminals opened after saving."
                  : "Adds the shims folder to your shell profile (.zshrc / .bashrc). Applies to terminals opened after saving."
              }
              checked={draft.addToPath}
              onChange={(v) => setDraft({ ...draft, addToPath: v })}
            />
            <Toggle
              label="Start services when lampRabit opens"
              hint="Starts every installed service, so your sites are up right after you sign in."
              checked={draft.startServicesOnLaunch}
              onChange={(v) => setDraft({ ...draft, startServicesOnLaunch: v })}
            />
            <Field label="Editor command" hint={'Used by "Open in editor" on site cards: code (VS Code), cursor, phpstorm, subl, zed…'}>
              <input
                className={inputClass}
                list="editor-presets"
                value={draft.editor}
                onChange={(e) => setDraft({ ...draft, editor: e.target.value })}
              />
              <datalist id="editor-presets">
                {["code", "cursor", "phpstorm", "subl", "zed", "codium"].map((e) => (
                  <option key={e} value={e} />
                ))}
              </datalist>
            </Field>
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

        <SectionCard icon={<ServerCog size={22} />} title="Web server" subtitle="Which server runs your sites.">
          <div className="space-y-5">
            <Segmented
              value={draft.webServer}
              onChange={(webServer) => setDraft({ ...draft, webServer })}
              className="py-1.5"
              options={[
                { value: "apache", label: "Apache" },
                { value: "nginx", label: "Nginx" },
                { value: "both", label: "Both" },
              ]}
            />
            <p className="text-sm text-muted">
              {draft.webServer === "both"
                ? `Apache keeps ports ${draft.ports.http}/${draft.ports.https}; Nginx uses ${draft.ports.nginxHttp}/${draft.ports.nginxHttps}. Pick the server for each site on the Sites page.`
                : draft.webServer === "nginx"
                  ? "Nginx serves every site. It doesn't read .htaccess files; lampRabit routes unknown URLs to index.php, which suits Laravel, Symfony and WordPress."
                  : "Apache serves every site and honours .htaccess files, like XAMPP and Laragon."}
            </p>
            {os === "windows" && (
              <Field label="PHP workers per version" hint="Each worker handles one request at a time. 4 suits most projects; raise it for parallel requests.">
                <input
                  type="number"
                  min={1}
                  max={10}
                  className={inputClass}
                  value={draft.phpWorkers}
                  onChange={(e) => setDraft({ ...draft, phpWorkers: Math.max(1, Math.min(10, Number(e.target.value))) })}
                />
              </Field>
            )}
          </div>
        </SectionCard>

        <SectionCard icon={<Plug size={22} />} title="Ports" subtitle="Network ports used by local services.">
          <div className="grid gap-x-5 gap-y-5 sm:grid-cols-2">
            {portFields
              .filter((f) => (!f.only || (os && f.only.includes(os))) && (!f.both || draft.webServer === "both"))
              .map(({ key, label, hint }) => (
              <Field key={key} label={label} hint={hint}>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  className={inputClass}
                  value={draft.ports[key]}
                  onChange={(e) => setDraft({ ...draft, ports: { ...draft.ports, [key]: Number(e.target.value) } })}
                />
                <PortState
                  port={draft.ports[key]}
                  saved={settings.data?.ports[key]}
                  status={ports.data?.find((p) => p.key === key)}
                />
              </Field>
            ))}
          </div>
        </SectionCard>

        <SectionCard icon={<Layers size={22} />} title="Active versions" subtitle="Which installed version each service runs.">
          <div className="space-y-4">
            {(["apache", "nginx", "php", "mariadb"] as Runtime[])
              .filter((r) => (r === "apache" ? draft.webServer !== "nginx" : r === "nginx" ? draft.webServer !== "apache" : true))
              .map((runtime) => {
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

        <SectionCard
          icon={<Monitor size={22} />}
          title="Appearance"
          subtitle="Choose how lampRabit looks on your device."
          className="lg:col-span-2"
        >
          <div className="grid gap-6 lg:grid-cols-2">
            <div>
              <div className="mb-2 text-sm font-medium">Theme</div>
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
            </div>
            <div>
              <div className="mb-2 text-sm font-medium">Motion</div>
              <Segmented
                value={motion}
                onChange={setMotion}
                className="py-1.5"
                options={[
                  { value: "system", label: "System", icon: <Monitor size={16} /> },
                  { value: "full", label: "Full", icon: <Wind size={16} /> },
                  { value: "reduced", label: "Reduced", icon: <Feather size={16} /> },
                  { value: "off", label: "Off", icon: <CircleSlash size={16} /> },
                ]}
              />
              <p className="mt-3 text-sm text-muted">
                {motion === "system"
                  ? "Follows your system's reduce-motion setting."
                  : motion === "full"
                    ? "Transitions, the animated logo and live diagram flows."
                    : motion === "reduced"
                      ? "Gentle fades only: nothing slides, bounces or plays on its own."
                      : "No animation at all. Loading spinners keep turning so you can tell work is happening."}
              </p>
            </div>
          </div>
        </SectionCard>

        <SectionCard
          icon={<Mail size={22} />}
          title="Email testing"
          subtitle="Mail your apps send lands in Mailpit instead of real inboxes."
          className="lg:col-span-2"
        >
          {!draft.versions.mailpit && (
            <div className="mb-5 flex flex-wrap items-center gap-3 rounded-xl border border-dashed border-line px-4 py-3 text-sm text-muted">
              Mailpit isn't installed yet.
              <Button size="sm" variant="soft" onClick={() => navigate("services")}>
                Install from Services
              </Button>
            </div>
          )}
          <div className="grid gap-6 lg:grid-cols-2">
            <div className="space-y-5">
              <Toggle
                label="Send PHP mail() to Mailpit"
                hint={
                  os === "windows"
                    ? "Sets SMTP and smtp_port in php.ini. Takes effect when Apache restarts."
                    : "Sets sendmail_path to Mailpit's sendmail mode. Takes effect when PHP restarts."
                }
                checked={draft.routeMail}
                onChange={(v) => setDraft({ ...draft, routeMail: v })}
              />
              <div className="grid gap-5 sm:grid-cols-2">
                <Field label="SMTP port" hint="Where apps send mail.">
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    className={inputClass}
                    value={draft.ports.mailSmtp}
                    onChange={(e) => setDraft({ ...draft, ports: { ...draft.ports, mailSmtp: Number(e.target.value) } })}
                  />
                </Field>
                <Field label="Inbox port" hint="Mailpit's web interface.">
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    className={inputClass}
                    value={draft.ports.mailUi}
                    onChange={(e) => setDraft({ ...draft, ports: { ...draft.ports, mailUi: Number(e.target.value) } })}
                  />
                </Field>
              </div>
            </div>
            <div className="space-y-3 rounded-xl bg-surface-2 p-4 text-sm">
              <div className="font-semibold">Two ways apps send mail</div>
              <p className="text-muted">
                <span className="font-medium text-fg">Frameworks (Laravel, Symfony, WordPress SMTP plugins)</span> talk SMTP
                directly. Point them at Mailpit:
              </p>
              <pre className="selectable overflow-x-auto rounded-lg bg-bg px-3 py-2 font-mono text-xs">
                {`MAIL_HOST=127.0.0.1\nMAIL_PORT=${draft.ports.mailSmtp}\nMAIL_USERNAME=null\nMAIL_PASSWORD=null`}
              </pre>
              <p className="text-muted">
                <span className="font-medium text-fg">Plain PHP mail()</span> uses php.ini instead. The switch on the left
                handles that for you.
              </p>
            </div>
          </div>
        </SectionCard>

        <UpdatesCard
          checkUpdates={draft.checkUpdates}
          onCheckUpdatesChange={(v) => setDraft({ ...draft, checkUpdates: v })}
        />

        <SharingCard />

        <SectionCard
          icon={<LifeBuoy size={22} />}
          title="Troubleshooting"
          subtitle="When something doesn't start or behaves oddly."
          className="lg:col-span-2"
        >
          <div className="flex flex-wrap items-center gap-4">
            <p className="min-w-60 flex-1 text-sm text-muted">
              The diagnostics report collects versions, service health, who is using each port, the Apache config
              test, hosts and certificate status, and recent errors. Copy it into an issue or keep it for later.
            </p>
            <Button variant="primary" onClick={() => openDiagnostics(true)}>
              <Stethoscope size={16} /> Run diagnostics
            </Button>
          </div>
        </SectionCard>
      </div>
    </>
  );
}

/** Live status under a port field: free, lampRabit's own, or taken by another program. */
function PortState({ port, saved, status }: { port: number; saved?: number; status?: PortStatus }) {
  const edited = saved !== undefined && port !== saved;
  const check = useQuery({
    queryKey: ["check-port", port],
    queryFn: () => api.checkPort(port),
    enabled: edited && port > 0 && port < 65536,
  });

  let tone: "ok" | "muted" | "warn" = "muted";
  let text: string;
  if (edited) {
    if (check.data === undefined) return null;
    tone = check.data ? "ok" : "warn";
    text = check.data ? "Available" : "In use by another program";
  } else if (!status) {
    return null;
  } else if (status.free) {
    tone = "ok";
    text = "Available";
  } else if (status.usedByService) {
    text = `In use by lampRabit (${status.usedByService})`;
  } else {
    tone = "warn";
    text = status.owner ? `In use by ${status.owner.name} (PID ${status.owner.pid})` : "In use by another program";
  }

  return (
    <span
      className={clsx(
        "mt-1.5 flex items-center gap-1.5 text-xs",
        tone === "ok" && "text-accent-text",
        tone === "muted" && "text-muted",
        tone === "warn" && "text-warn",
      )}
      title={status?.owner?.note ?? undefined}
    >
      <span className={clsx("size-1.5 rounded-full", tone === "ok" ? "bg-ok" : tone === "warn" ? "bg-warn" : "bg-muted/60")} />
      {text}
    </span>
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

/** Ngrok's authtoken lives in the system keychain, never in lampRabit's files. */
function SharingCard() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const saved = useQuery({ queryKey: ["ngrok-token"], queryFn: api.ngrokTokenSaved });
  const [token, setToken] = useState("");
  const save = useMutation({
    mutationFn: (t: string | null) => api.setNgrokToken(t),
    onSuccess: (_, t) => {
      setToken("");
      queryClient.invalidateQueries({ queryKey: ["ngrok-token"] });
      notify("info", t ? "Ngrok authtoken saved to your system keychain" : "Ngrok authtoken removed");
    },
    onError: (e) => notify("error", String(e)),
  });

  return (
    <SectionCard icon={<Share2 size={22} />} title="Sharing" subtitle="Put a site on a public URL for testing webhooks or showing a client." className="lg:col-span-2">
      <div className="grid gap-6 lg:grid-cols-2">
        <div className="space-y-3">
          <div className="text-sm font-semibold">Ngrok authtoken</div>
          <div className="flex gap-2">
            <input
              type="password"
              className={inputClass}
              placeholder={saved.data ? "Saved. Paste a new one to replace it" : "Paste your token"}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              autoComplete="off"
            />
            <Button variant="primary" className="h-auto" onClick={() => save.mutate(token)} disabled={!token.trim() || save.isPending}>
              Save
            </Button>
            {saved.data && (
              <Button className="h-auto" onClick={() => save.mutate(null)} disabled={save.isPending}>
                Remove
              </Button>
            )}
          </div>
          <p className="text-xs text-muted">
            {saved.data ? "A token is saved in your system keychain. " : ""}
            Ngrok needs a free account; copy the token from dashboard.ngrok.com → Your Authtoken. The free plan gives
            one random URL at a time.
          </p>
        </div>
        <div className="space-y-2 rounded-xl bg-surface-2 p-4 text-sm">
          <div className="font-semibold">Cloudflare quick tunnels</div>
          <p className="text-muted">
            Need no account or token: install Cloudflare Tunnel from Services and use the share button on a site. Each
            tunnel gets a random trycloudflare.com address that lasts until you stop it.
          </p>
        </div>
      </div>
    </SectionCard>
  );
}

function UpdatesCard({ checkUpdates, onCheckUpdatesChange }: { checkUpdates: boolean; onCheckUpdatesChange: (v: boolean) => void }) {
  const notify = useAppStore((s) => s.notify);
  const version = useQuery({ queryKey: ["app-version"], queryFn: getVersion });
  const { available, checking, error, checked, checkNow } = useAppUpdate();
  const [progress, setProgress] = useState<number | null | undefined>(undefined);

  const install = async () => {
    if (!available) return;
    setProgress(null);
    try {
      await installAppUpdate(available, setProgress);
    } catch (e) {
      setProgress(undefined);
      notify("error", String(e));
    }
  };

  return (
    <SectionCard icon={<Sparkles size={22} />} title="Updates" subtitle={`lampRabit ${version.data ?? ""}`} className="lg:col-span-2">
      <div className="flex flex-wrap items-center gap-4">
        <div className="min-w-60 flex-1 text-sm">
          {available ? (
            <>
              <div className="font-semibold text-accent-text">Version {available.version} is available</div>
              <button
                className="mt-1 inline-flex items-center gap-1 text-accent-text hover:underline"
                onClick={() => openUrl(`https://github.com/Shivam-SV/LampRabit/releases/tag/v${available.version}`).catch(() => {})}
              >
                What's new in {available.version} <ExternalLink size={13} />
              </button>
            </>
          ) : checking ? (
            <span className="text-muted">Checking for updates…</span>
          ) : error ? (
            <span className="text-muted">{error}</span>
          ) : checked ? (
            <span className="text-muted">You're on the latest version.</span>
          ) : (
            <span className="text-muted">Updates are signed and installed in place; your environment is kept.</span>
          )}
        </div>
        {available ? (
          <Button variant="primary" onClick={install} disabled={progress !== undefined}>
            {progress === undefined ? "Install and restart" : progress === null ? "Downloading…" : `Downloading ${progress}%`}
          </Button>
        ) : (
          <Button onClick={() => checkNow()} disabled={checking}>
            <RefreshCw size={15} className={clsx(checking && "animate-spin")} /> Check for updates
          </Button>
        )}
      </div>
      <div className="mt-4">
        <Toggle
          label="Check when lampRabit starts"
          hint="Looks for a new release on GitHub; nothing is downloaded until you choose to install."
          checked={checkUpdates}
          onChange={onCheckUpdatesChange}
        />
      </div>
    </SectionCard>
  );
}

/** Start with the OS. Applied immediately through the OS (no Save needed). */
function AutostartToggle() {
  const notify = useAppStore((s) => s.notify);
  const enabled = useQuery({ queryKey: ["autostart"], queryFn: isAutostartEnabled });
  const queryClient = useQueryClient();
  return (
    <Toggle
      label="Start lampRabit when I sign in"
      hint="Opens in the tray, out of the way. Applies immediately."
      checked={!!enabled.data}
      onChange={async (v) => {
        try {
          await (v ? enableAutostart() : disableAutostart());
          queryClient.invalidateQueries({ queryKey: ["autostart"] });
        } catch (e) {
          notify("error", String(e));
        }
      }}
    />
  );
}
