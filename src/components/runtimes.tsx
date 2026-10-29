import type { ReactNode } from "react";
import { Cloud, Code2, Database, DatabaseZap, Feather, Layers3, Mail, Package, Radio, Server, Table2, TableProperties, Zap } from "lucide-react";
import type { Runtime } from "../lib/api";

/** Icon and colour used for each runtime across the app. */
export const runtimeTiles: Record<Runtime, { className: string; text: string; icon: (size: number) => ReactNode }> = {
  apache: {
    className: "bg-rose-500/10 text-rose-500",
    text: "text-rose-500",
    icon: (size) => <Feather size={size} strokeWidth={1.75} />,
  },
  nginx: {
    className: "bg-emerald-600/10 text-emerald-600",
    text: "text-emerald-600",
    icon: (size) => <Server size={size} strokeWidth={1.75} />,
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
  mysql: {
    className: "bg-blue-600/10 text-blue-600 dark:text-blue-400",
    text: "text-blue-600 dark:text-blue-400",
    icon: (size) => <DatabaseZap size={size} strokeWidth={1.75} />,
  },
  mailpit: {
    className: "bg-sky-500/10 text-sky-500",
    text: "text-sky-500",
    icon: (size) => <Mail size={size} strokeWidth={1.75} />,
  },
  adminer: {
    className: "bg-teal-500/10 text-teal-500",
    text: "text-teal-500",
    icon: (size) => <Table2 size={size} strokeWidth={1.75} />,
  },
  redis: {
    className: "bg-red-500/10 text-red-500",
    text: "text-red-500",
    icon: (size) => <Zap size={size} strokeWidth={1.75} />,
  },
  ngrok: {
    className: "bg-violet-500/10 text-violet-500",
    text: "text-violet-500",
    icon: (size) => <Radio size={size} strokeWidth={1.75} />,
  },
  cloudflared: {
    className: "bg-orange-400/15 text-orange-500",
    text: "text-orange-500",
    icon: (size) => <Cloud size={size} strokeWidth={1.75} />,
  },
  composer: {
    className: "bg-yellow-700/10 text-yellow-700",
    text: "text-yellow-700",
    icon: (size) => <Package size={size} strokeWidth={1.75} />,
  },
  memcached: {
    className: "bg-teal-500/10 text-teal-600",
    text: "text-teal-600",
    icon: (size) => <Layers3 size={size} strokeWidth={1.75} />,
  },
  phpredisadmin: {
    className: "bg-red-500/10 text-red-600 dark:text-red-400",
    text: "text-red-600 dark:text-red-400",
    icon: (size) => <TableProperties size={size} strokeWidth={1.75} />,
  },
  phpmyadmin: {
    className: "bg-orange-500/10 text-orange-500",
    text: "text-orange-500",
    icon: (size) => <TableProperties size={size} strokeWidth={1.75} />,
  },
};
