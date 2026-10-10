import { create } from "zustand";

/** O tema escolhido; `system` segue o claro ou o escuro do sistema. */
export type ThemePreference = "system" | "light" | "dark";
export type Theme = "light" | "dark";

export const THEME_PREFERENCES: ThemePreference[] = ["system", "light", "dark"];
const THEME_KEY = "jayv.theme";
const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

interface ThemeState {
  preference: ThemePreference;
  /** O que está na tela agora. */
  theme: Theme;
}

function savedPreference(): ThemePreference {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
  } catch {
    // Sem armazenamento, vale o sistema.
  }
  return "system";
}

// Fora de uma janela (os testes), o sistema não diz nada: vale o claro.
const systemDark = () => typeof window !== "undefined" && typeof window.matchMedia === "function" && darkQuery().matches;

const resolve = (preference: ThemePreference): Theme =>
  preference === "system" ? (systemDark() ? "dark" : "light") : preference;

function apply(theme: Theme) {
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
}

const initial = savedPreference();
export const useTheme = create<ThemeState>(() => ({ preference: initial, theme: resolve(initial) }));

/** Pinta o tema salvo antes da primeira tela e acompanha o sistema enquanto a
 * escolha for `system`. */
export function connectTheme() {
  apply(useTheme.getState().theme);
  const query = darkQuery();
  const follow = () => {
    const { preference } = useTheme.getState();
    if (preference !== "system") return;
    const theme = resolve(preference);
    useTheme.setState({ theme });
    apply(theme);
  };
  query.addEventListener("change", follow);
  return () => query.removeEventListener("change", follow);
}

export function setThemePreference(preference: ThemePreference) {
  try {
    localStorage.setItem(THEME_KEY, preference);
  } catch {
    // A escolha vale só até fechar o app.
  }
  const theme = resolve(preference);
  useTheme.setState({ preference, theme });
  apply(theme);
}
