import { FileText, Globe, House, Layers, Settings } from "lucide-react";
import clsx from "clsx";
import { useAppStore, type PageId } from "../lib/store";

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
            <Icon size={21} strokeWidth={page === id ? 2 : 1.6} />
            {label}
          </button>
        ))}
      </div>
    </nav>
  );
}
