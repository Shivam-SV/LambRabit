import { useQuery } from "@tanstack/react-query";
import { Layers3 } from "lucide-react";
import { api } from "../lib/api";
import { Card, IconTile } from "./ui";

/** Memcached comes from the system, so this card only reports what's available. */
export default function MemcachedCard() {
  const os = useQuery({ queryKey: ["paths"], queryFn: api.envPaths }).data?.os;
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const memcached = services.data?.find((s) => s.kind === "memcached");

  return (
    <Card className="p-6">
      <div className="flex items-start gap-4">
        <IconTile className="bg-teal-500/10 text-teal-600">
          <Layers3 size={22} strokeWidth={1.75} />
        </IconTile>
        <div className="min-w-0 flex-1">
          <h2 className="text-lg font-bold tracking-tight">Memcached</h2>
          <p className="text-sm text-muted">
            {os === "windows"
              ? "Not available on Windows: there are no maintained Memcached builds for it. Use Redis (above) for caching and sessions; Laravel, Symfony and WordPress all support it."
              : memcached?.installed
                ? `Using memcached ${memcached.version} from your system. Start it from Overview.`
                : "lampRabit runs the memcached installed on your system. Install it with brew install memcached (macOS) or sudo apt install memcached (Debian/Ubuntu), then reopen this page."}
          </p>
        </div>
      </div>
    </Card>
  );
}
