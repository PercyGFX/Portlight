import { useCallback, useEffect, useState } from "react";

export type Theme = "light" | "dark";
/** What the person picked: a theme, or whatever Windows uses. */
export type ThemeChoice = Theme | "system";

const STORAGE_KEY = "portlight.theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

function savedChoice(): ThemeChoice {
  const saved = localStorage.getItem(STORAGE_KEY);
  return saved === "light" || saved === "dark" ? saved : "system";
}

const systemTheme = (): Theme => (window.matchMedia(DARK_QUERY).matches ? "dark" : "light");

export interface ThemeState {
  /** The theme on screen now. */
  theme: Theme;
  choice: ThemeChoice;
  setChoice(choice: ThemeChoice): void;
}

/** Light, dark or "match Windows", remembered between runs. Starts by matching Windows. */
export function useTheme(): ThemeState {
  const [choice, setChoiceState] = useState<ThemeChoice>(savedChoice);
  const [system, setSystem] = useState<Theme>(systemTheme);

  // Follow Windows when it switches between light and dark.
  useEffect(() => {
    const query = window.matchMedia(DARK_QUERY);
    const onChange = () => setSystem(query.matches ? "dark" : "light");
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const theme = choice === "system" ? system : choice;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const setChoice = useCallback((next: ThemeChoice) => {
    if (next === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, next);
    setChoiceState(next);
  }, []);

  return { theme, choice, setChoice };
}
