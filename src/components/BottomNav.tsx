import { FileText, Globe, House, Layers, Settings } from "lucide-react";
import clsx from "clsx";
import { useAppStore, type PageId } from "../lib/store";
import { useRuntimeUpdates } from "../lib/updates";

const items: { id: PageId; label: string; icon: typeof House }[] = [
  { id: "overview", label: "Overview", icon: House },
  { id: "sites", label: "Sites", icon: Globe },
  { id: "services", label: "Services", icon: Layers },
  { id: "logs", label: "Logs", icon: FileText },
  { id: "settings", label: "Settings", icon: Settings },
];

export default function BottomNav() {
  const page = useAppStore((s) => s.page);
  const navigate = useAppStore((s) => s.navigate);
  const runtimeUpdates = useRuntimeUpdates().data?.length ?? 0;

  return (
    <nav className="fixed inset-x-0 bottom-4 z-10 flex justify-center px-4">
      <div className="flex items-center gap-1 rounded-2xl border border-line bg-nav p-1.5 shadow-float backdrop-blur-md">
        {items.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => navigate(id)}
            aria-current={page === id ? "page" : undefined}
            className={clsx(
              "flex w-[5.25rem] flex-col items-center gap-1 rounded-xl py-2 text-xs transition-colors",
              page === id
                ? "bg-accent-soft font-semibold text-accent-text"
                : "text-muted hover:bg-surface-2 hover:text-fg",
            )}
          >
            <span className="relative">
              <Icon size={21} strokeWidth={page === id ? 2 : 1.6} />
              {id === "services" && runtimeUpdates > 0 && (
                <span className="absolute -top-0.5 -right-1 size-2 rounded-full bg-accent ring-2 ring-surface" aria-label="Updates available" />
              )}
            </span>
            {label}
          </button>
        ))}
      </div>
    </nav>
  );
}
