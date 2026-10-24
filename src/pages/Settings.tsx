import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Save } from "lucide-react";
import { api, type Settings as SettingsT } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader, inputClass } from "../components/ui";

const portFields = [
  { key: "http", label: "HTTP" },
  { key: "https", label: "HTTPS" },
  { key: "mysql", label: "MariaDB" },
  { key: "phpFpm", label: "PHP-FPM (macOS/Linux)" },
] as const;

export default function Settings() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const [draft, setDraft] = useState<SettingsT | null>(null);

  useEffect(() => {
    if (settings.data) setDraft(settings.data);
  }, [settings.data]);

  const save = useMutation({
    mutationFn: api.saveSettings,
    onSuccess: (saved) => {
      queryClient.setQueryData(["settings"], saved);
      queryClient.invalidateQueries({ queryKey: ["services"] });
      queryClient.invalidateQueries({ queryKey: ["paths"] });
      notify("info", "Settings saved");
    },
    onError: (e) => notify("error", String(e)),
  });

  if (!draft) return null;

  return (
    <>
      <PageHeader
        title="Settings"
        subtitle="Where lampRabit keeps things and which ports it uses."
        actions={
          <Button variant="primary" onClick={() => save.mutate(draft)} disabled={save.isPending}>
            <Save size={14} /> Save
          </Button>
        }
      />
      <div className="max-w-2xl space-y-5">
        <Card className="space-y-4 p-6">
          <h2 className="text-lg font-bold tracking-tight">General</h2>
          <Field label="Environment root" hint="Holds runtimes, configs, www, databases and logs.">
            <input
              className={inputClass}
              value={draft.rootDir}
              onChange={(e) => setDraft({ ...draft, rootDir: e.target.value })}
            />
          </Field>
          <Field label="Site domain suffix" hint={`Folders in www/ are served as <name>.${draft.tld}`}>
            <input
              className={inputClass}
              value={draft.tld}
              onChange={(e) => setDraft({ ...draft, tld: e.target.value.replace(/^\.+/, "") })}
            />
          </Field>
        </Card>

        <Card className="space-y-4 p-6">
          <h2 className="text-lg font-bold tracking-tight">Ports</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            {portFields.map(({ key, label }) => (
              <Field key={key} label={label}>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  className={inputClass}
                  value={draft.ports[key]}
                  onChange={(e) =>
                    setDraft({ ...draft, ports: { ...draft.ports, [key]: Number(e.target.value) } })
                  }
                />
              </Field>
            ))}
          </div>
        </Card>
      </div>
    </>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="text-sm font-medium">{label}</span>
      {children}
      {hint && <span className="block text-xs text-muted">{hint}</span>}
    </label>
  );
}
