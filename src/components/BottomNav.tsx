import { Globe, House, Layers, Moon, ScrollText, Settings, Sun } from "lucide-react";
import clsx from "clsx";
import { useAppStore, type PageId } from "../lib/store";
import { useEffectiveTheme, useTheme } from "../lib/theme";

const items: { id: PageId; label: string; icon: typeof House }[] = [
  { id: "overview", label: "Overview", icon: House },
  { id: "sites", label: "Sites", icon: Globe },
  { id: "services", label: "Services", icon: Layers },
  { id: "logs", label: "Logs", icon: ScrollText },
  { id: "settings", label: "Settings", icon: Settings },
];

export default function BottomNav() {
  const page = useAppStore((s) => s.page);
  const navigate = useAppStore((s) => s.navigate);
  const setTheme = useTheme((s) => s.setTheme);
  const effective = useEffectiveTheme();

  return (
    <nav className="fixed inset-x-0 bottom-5 z-10 flex justify-center px-4">
      <div className="flex items-center gap-1 rounded-2xl border border-line bg-nav p-1.5 shadow-float backdrop-blur-md">
        {items.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => navigate(id)}
            aria-current={page === id ? "page" : undefined}
            className={clsx(
              "flex h-10 items-center gap-2 rounded-xl px-3.5 text-sm transition-colors",
              page === id
                ? "bg-accent-soft font-semibold text-accent-text"
                : "text-muted hover:bg-surface-2 hover:text-fg",
            )}
          >
            <Icon size={17} strokeWidth={page === id ? 2.25 : 1.75} />
            <span className="max-sm:sr-only">{label}</span>
          </button>
        ))}
        <span className="mx-1.5 h-6 w-px bg-line" />
        {(
          [
            { id: "light", icon: Sun, label: "Light theme" },
            { id: "dark", icon: Moon, label: "Dark theme" },
          ] as const
        ).map(({ id, icon: Icon, label }) => (
          <button
            key={id}
            onClick={() => setTheme(id)}
            aria-label={label}
            aria-pressed={effective === id}
            className={clsx(
              "flex size-10 items-center justify-center rounded-xl transition-colors",
              effective === id ? "bg-surface-2 text-accent-text shadow-card" : "text-muted hover:text-fg",
            )}
          >
            <Icon size={17} />
          </button>
        ))}
      </div>
    </nav>
  );
}
