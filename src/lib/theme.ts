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
      localStorage.setItem("theme", theme);
    } catch {}
    set({ theme });
  },
}));
