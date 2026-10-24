import { useEffect, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import type { ServiceKind } from "../lib/api";
import { useAppStore } from "../lib/store";
import { Button, Card, PageHeader } from "../components/ui";

const filters: { id: ServiceKind | "all"; label: string }[] = [
  { id: "all", label: "All" },
  { id: "apache", label: "Apache" },
  { id: "mariadb", label: "MariaDB" },
  { id: "php-fpm", label: "PHP-FPM" },
];

export default function Logs() {
  const logs = useAppStore((s) => s.logs);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const [filter, setFilter] = useState<ServiceKind | "all">("all");
  const bottom = useRef<HTMLDivElement>(null);

  const visible = filter === "all" ? logs : logs.filter((l) => l.kind === filter);

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [visible.length]);

  return (
    <div>
      <PageHeader
        title="Logs"
        subtitle="Live output from your services."
        actions={
          <>
            <select
              value={filter}
              onChange={(e) => setFilter(e.target.value as ServiceKind | "all")}
              className="h-9 rounded-lg border border-line bg-surface px-2.5 text-sm shadow-card"
            >
              {filters.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.label}
                </option>
              ))}
            </select>
            <Button onClick={clearLogs}>
              <Trash2 size={14} /> Clear
            </Button>
          </>
        }
      />
      <Card className="h-[calc(100vh-20rem)] min-h-72 overflow-auto p-5">
        {visible.length === 0 ? (
          <p className="text-sm text-muted">No output yet. Service output appears here while they run.</p>
        ) : (
          <pre className="selectable font-mono text-xs leading-relaxed">
            {visible.map((l) => (
              <div key={l.id}>
                <span className="text-muted">{l.at.toLocaleTimeString()} </span>
                <span className="text-accent-text">[{l.kind}] </span>
                {l.line}
              </div>
            ))}
            <div ref={bottom} />
          </pre>
        )}
      </Card>
    </div>
  );
}
