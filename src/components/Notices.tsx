import { X } from "lucide-react";
import clsx from "clsx";
import { useAppStore } from "../lib/store";

export default function Notices() {
  const notices = useAppStore((s) => s.notices);
  const dismiss = useAppStore((s) => s.dismiss);

  return (
    <div className="pointer-events-none fixed right-4 bottom-4 flex w-80 flex-col gap-2">
      {notices.map((n) => (
        <div
          key={n.id}
          className={clsx(
            "pointer-events-auto flex items-start gap-2 rounded-lg border px-3 py-2 text-sm shadow-lg",
            n.tone === "error"
              ? "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-200"
              : "border-zinc-200 bg-white text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200",
          )}
        >
          <span className="selectable flex-1 break-words">{n.text}</span>
          <button onClick={() => dismiss(n.id)} className="opacity-60 hover:opacity-100" aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
