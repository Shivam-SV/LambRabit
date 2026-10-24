import type { ReactNode } from "react";
import { Code2, Database, Feather } from "lucide-react";
import type { Runtime } from "../lib/api";

/** Icon and colour used for each runtime across the app. */
export const runtimeTiles: Record<Runtime, { className: string; text: string; icon: (size: number) => ReactNode }> = {
  apache: {
    className: "bg-rose-500/10 text-rose-500",
    text: "text-rose-500",
    icon: (size) => <Feather size={size} strokeWidth={1.75} />,
  },
  php: {
    className: "bg-indigo-500/10 text-indigo-500",
    text: "text-indigo-500",
    icon: (size) => <Code2 size={size} strokeWidth={1.75} />,
  },
  mariadb: {
    className: "bg-amber-600/12 text-amber-600",
    text: "text-amber-600",
    icon: (size) => <Database size={size} strokeWidth={1.75} />,
  },
};
