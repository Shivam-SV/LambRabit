import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Download, Hexagon, Loader2, Trash2 } from "lucide-react";
import clsx from "clsx";
import { api } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Card, IconTile } from "./ui";

/** Node.js through NVM: install NVM on request, then install, switch and remove versions. */
export default function NodeCard() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
  const status = useQuery({ queryKey: ["node"], queryFn: api.nodeStatus });
  const available = useQuery({
    queryKey: ["node-available"],
    queryFn: api.nodeAvailable,
    enabled: !!status.data?.manager,
    staleTime: 60 * 60 * 1000,
  });
  const [pick, setPick] = useState("");
  const [confirm, setConfirm] = useState<string | null>(null);
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["node"] });
  const onError = (e: unknown) => notify("error", String(e));

  const installNvm = useMutation({ mutationFn: api.nvmInstall, onSuccess: refresh, onError });
  const install = useMutation({
    mutationFn: (v: string) => api.nodeInstall(v),
    onSuccess: (_, v) => {
      notify("info", `Node ${v} installed`);
      setPick("");
      refresh();
    },
    onError,
  });
  const use = useMutation({ mutationFn: (v: string) => api.nodeUse(v), onSuccess: refresh, onError });
  const remove = useMutation({
    mutationFn: (v: string) => api.nodeUninstall(v),
    onSuccess: () => {
      setConfirm(null);
      refresh();
    },
    onError,
  });

  const data = status.data;
  const busy = install.isPending || use.isPending || remove.isPending;
  const choices = (available.data ?? []).filter((r) => !data?.installed.includes(r.version));

  return (
    <Card className="p-6">
      <div className="mb-5 flex items-start gap-4">
        <IconTile className="bg-lime-600/10 text-lime-600">
          <Hexagon size={22} strokeWidth={1.75} />
        </IconTile>
        <div className="min-w-0 flex-1">
          <h2 className="text-lg font-bold tracking-tight">Node.js</h2>
          <p className="text-sm text-muted">
            {data?.manager
              ? `Managed with ${data.manager}${data.managerVersion ? ` ${data.managerVersion}` : ""} · ${data.installed.length} installed`
              : "Versions are managed through NVM, installed only when you ask."}
          </p>
        </div>
        {data?.current && (
          <div className="text-right">
            <div className="text-xs text-muted">Default</div>
            <div className="font-mono text-lg font-semibold">{data.current}</div>
          </div>
        )}
      </div>

      {status.isLoading ? (
        <p className="text-sm text-muted">Checking for NVM…</p>
      ) : !data?.manager ? (
        <div className="flex flex-wrap items-center gap-4 rounded-xl border border-dashed border-line p-4">
          <p className="min-w-60 flex-1 text-sm text-muted">
            {os === "windows"
              ? "Installs nvm-windows with its official installer. Windows asks for administrator approval once. Switching Node versions later needs admin rights unless Developer Mode is on."
              : "Runs the official nvm install script, which adds nvm to your shell profile."}
          </p>
          <Button variant="primary" onClick={() => installNvm.mutate()} disabled={installNvm.isPending}>
            {installNvm.isPending ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />} Install NVM
          </Button>
        </div>
      ) : (
        <div className="grid gap-5 md:grid-cols-2">
          <section>
            <h3 className="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Installed</h3>
            {data.installed.length === 0 ? (
              <p className="text-sm text-muted">No Node versions yet.</p>
            ) : (
              <ul className="divide-y divide-line rounded-xl border border-line">
                {data.installed.map((v) => (
                  <li key={v} className="flex items-center gap-2 px-3 py-2 text-sm">
                    <span className="font-mono">{v}</span>
                    {data.current === v && (
                      <span className="flex items-center gap-1 rounded bg-accent-soft px-1.5 py-0.5 text-xs font-medium text-accent-text">
                        <Check size={12} /> Default
                      </span>
                    )}
                    <span className="flex-1" />
                    {confirm === v ? (
                      <>
                        <Button size="sm" variant="danger" onClick={() => remove.mutate(v)} disabled={busy}>
                          Remove
                        </Button>
                        <Button size="sm" onClick={() => setConfirm(null)}>
                          Keep
                        </Button>
                      </>
                    ) : (
                      <>
                        {data.current !== v && (
                          <Button size="sm" onClick={() => use.mutate(v)} disabled={busy}>
                            Use
                          </Button>
                        )}
                        <Button size="sm" variant="ghost" onClick={() => setConfirm(v)} aria-label={`Remove Node ${v}`} disabled={busy}>
                          <Trash2 size={14} />
                        </Button>
                      </>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {data.needsElevation && (
              <p className="mt-2 text-xs text-muted">Switching versions asks for administrator approval (turn on Developer Mode to avoid it).</p>
            )}
          </section>
          <section>
            <h3 className="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Add a version</h3>
            <div className="flex gap-2">
              <select
                value={pick}
                onChange={(e) => setPick(e.target.value)}
                className="min-w-0 flex-1 rounded-lg border border-line bg-surface px-2.5 text-sm"
                aria-label="Node version to install"
              >
                <option value="">{available.isLoading ? "Loading versions…" : "Choose a version"}</option>
                {choices.map((r) => (
                  <option key={r.version} value={r.version}>
                    {r.version} {r.lts ? `· LTS (${r.lts})` : "· Current"}
                  </option>
                ))}
              </select>
              <Button variant="primary" onClick={() => pick && install.mutate(pick)} disabled={!pick || busy}>
                {install.isPending ? <Loader2 size={15} className={clsx("animate-spin")} /> : <Download size={15} />} Install
              </Button>
            </div>
            <p className="mt-2 text-xs text-muted">
              Projects with an .nvmrc file show the version they expect on the Sites page.
            </p>
          </section>
        </div>
      )}
    </Card>
  );
}
