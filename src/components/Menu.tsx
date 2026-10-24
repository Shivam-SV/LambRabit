import { useEffect, useRef, useState, type ReactNode } from "react";
import { MoreHorizontal, MoreVertical } from "lucide-react";
import clsx from "clsx";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
}

/** A "⋯" button with a small dropdown; `plain` renders a borderless "⋮" for list rows. */
export default function Menu({
  items,
  label = "More actions",
  plain = false,
}: {
  items: MenuItem[];
  label?: string;
  plain?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !root.current?.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);

  return (
    <div ref={root} className="relative">
      <button
        aria-label={label}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className={clsx(
          "flex size-8 items-center justify-center rounded-lg text-muted transition-colors hover:bg-surface-2 hover:text-fg",
          !plain && "border border-line",
        )}
      >
        {plain ? <MoreVertical size={17} /> : <MoreHorizontal size={16} />}
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 z-20 mt-1.5 min-w-48 rounded-xl border border-line bg-surface p-1 shadow-float"
        >
          {items.map((item) => (
            <button
              key={item.label}
              role="menuitem"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-sm hover:bg-surface-2"
            >
              <span className="text-muted">{item.icon}</span>
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
