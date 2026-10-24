import { useEffect, useState } from "react";
import { create } from "zustand";

export type Theme = "system" | "light" | "dark";

function readTheme(): Theme {
  try {
    const t = localStorage.getItem("theme");
    if (t === "light" || t === "dark") return t;
  } catch {}
  return "system";
}

export const useTheme = create<{ theme: Theme; setTheme: (t: Theme) => void }>((set) => ({
  theme: readTheme(),
  setTheme: (theme) => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    try {
      if (theme === "system") localStorage.removeItem("theme");
      else localStorage.setItem("theme", theme);
    } catch {}
    set({ theme });
  },
}));

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

/** The theme actually on screen, resolving "system" against the OS preference. */
export function useEffectiveTheme(): "light" | "dark" {
  const theme = useTheme((s) => s.theme);
  const [systemDark, setSystemDark] = useState(() => darkQuery().matches);
  useEffect(() => {
    const q = darkQuery();
    const onChange = () => setSystemDark(q.matches);
    q.addEventListener("change", onChange);
    return () => q.removeEventListener("change", onChange);
  }, []);
  if (theme === "system") return systemDark ? "dark" : "light";
  return theme;
}
