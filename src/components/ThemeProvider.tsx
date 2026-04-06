import { createContext, useContext, useEffect, useState } from 'react';

// ── Theme definitions ───────────────────────────────────────────────────────

export type ThemeMode = 'dark' | 'light' | 'system';

export interface ThemePalette {
  id: string;
  label: string;
  mode: 'dark' | 'light';
  colors: Record<string, string>;
}

export const THEMES: ThemePalette[] = [
  // ── Light themes ────────────────────────────────────────────────────────
  {
    id: 'default-light',
    label: 'Default Light',
    mode: 'light',
    colors: {
      '--bg-primary': '250 250 249',      // zinc-50
      '--bg-secondary': '255 255 255',
      '--bg-tertiary': '244 244 245',     // zinc-100
      '--bg-elevated': '255 255 255',
      '--bg-input': '250 250 249',
      '--text-primary': '24 24 27',        // zinc-900
      '--text-secondary': '113 113 122',   // zinc-500
      '--text-tertiary': '161 161 170',    // zinc-400
      '--border-primary': '228 228 231',   // zinc-200
      '--border-secondary': '244 244 245', // zinc-100
      '--accent': '124 58 237',            // violet-600
      '--accent-soft': '237 233 254',      // violet-50
      '--accent-text': '109 40 217',       // violet-700
    },
  },
  {
    id: 'github-light',
    label: 'GitHub Light',
    mode: 'light',
    colors: {
      '--bg-primary': '255 255 255',
      '--bg-secondary': '246 248 250',
      '--bg-tertiary': '234 238 242',
      '--bg-elevated': '255 255 255',
      '--bg-input': '246 248 250',
      '--text-primary': '31 35 40',
      '--text-secondary': '101 109 118',
      '--text-tertiary': '140 149 159',
      '--border-primary': '216 222 228',
      '--border-secondary': '234 238 242',
      '--accent': '31 111 200',
      '--accent-soft': '218 236 255',
      '--accent-text': '9 105 218',
    },
  },
  {
    id: 'solarized-light',
    label: 'Solarized Light',
    mode: 'light',
    colors: {
      '--bg-primary': '253 246 227',
      '--bg-secondary': '238 232 213',
      '--bg-tertiary': '227 221 202',
      '--bg-elevated': '253 246 227',
      '--bg-input': '238 232 213',
      '--text-primary': '88 110 117',
      '--text-secondary': '101 123 131',
      '--text-tertiary': '147 161 161',
      '--border-primary': '213 207 187',
      '--border-secondary': '227 221 202',
      '--accent': '38 139 210',
      '--accent-soft': '219 240 255',
      '--accent-text': '38 139 210',
    },
  },
  // ── Dark themes ─────────────────────────────────────────────────────────
  {
    id: 'default-dark',
    label: 'Default Dark',
    mode: 'dark',
    colors: {
      '--bg-primary': '9 9 11',            // zinc-950
      '--bg-secondary': '24 24 27',        // zinc-900
      '--bg-tertiary': '39 39 42',         // zinc-800
      '--bg-elevated': '24 24 27',
      '--bg-input': '24 24 27',
      '--text-primary': '244 244 245',     // zinc-100
      '--text-secondary': '161 161 170',   // zinc-400
      '--text-tertiary': '113 113 122',    // zinc-500
      '--border-primary': '39 39 42',      // zinc-800
      '--border-secondary': '24 24 27',    // zinc-900
      '--accent': '167 139 250',           // violet-400
      '--accent-soft': '30 20 60',
      '--accent-text': '196 181 253',      // violet-300
    },
  },
  {
    id: 'ayu-dark',
    label: 'Ayu Dark',
    mode: 'dark',
    colors: {
      '--bg-primary': '10 14 20',
      '--bg-secondary': '15 20 28',
      '--bg-tertiary': '22 29 39',
      '--bg-elevated': '18 23 32',
      '--bg-input': '15 20 28',
      '--text-primary': '204 204 204',
      '--text-secondary': '128 138 156',
      '--text-tertiary': '95 105 122',
      '--border-primary': '30 38 50',
      '--border-secondary': '22 29 39',
      '--accent': '255 180 84',
      '--accent-soft': '40 30 15',
      '--accent-text': '255 180 84',
    },
  },
  {
    id: 'github-dark',
    label: 'GitHub Dark',
    mode: 'dark',
    colors: {
      '--bg-primary': '13 17 23',
      '--bg-secondary': '22 27 34',
      '--bg-tertiary': '33 38 45',
      '--bg-elevated': '22 27 34',
      '--bg-input': '22 27 34',
      '--text-primary': '230 237 243',
      '--text-secondary': '139 148 158',
      '--text-tertiary': '110 118 129',
      '--border-primary': '48 54 61',
      '--border-secondary': '33 38 45',
      '--accent': '88 166 255',
      '--accent-soft': '18 30 50',
      '--accent-text': '121 192 255',
    },
  },
  {
    id: 'monokai',
    label: 'Monokai Pro',
    mode: 'dark',
    colors: {
      '--bg-primary': '45 42 46',
      '--bg-secondary': '55 52 56',
      '--bg-tertiary': '65 62 66',
      '--bg-elevated': '55 52 56',
      '--bg-input': '55 52 56',
      '--text-primary': '252 252 240',
      '--text-secondary': '169 163 155',
      '--text-tertiary': '130 125 118',
      '--border-primary': '73 70 74',
      '--border-secondary': '60 57 61',
      '--accent': '169 220 118',
      '--accent-soft': '30 42 22',
      '--accent-text': '169 220 118',
    },
  },
  {
    id: 'solarized-dark',
    label: 'Solarized Dark',
    mode: 'dark',
    colors: {
      '--bg-primary': '0 43 54',
      '--bg-secondary': '7 54 66',
      '--bg-tertiary': '14 65 78',
      '--bg-elevated': '7 54 66',
      '--bg-input': '7 54 66',
      '--text-primary': '147 161 161',
      '--text-secondary': '101 123 131',
      '--text-tertiary': '88 110 117',
      '--border-primary': '14 65 78',
      '--border-secondary': '7 54 66',
      '--accent': '38 139 210',
      '--accent-soft': '10 55 75',
      '--accent-text': '108 182 230',
    },
  },
];

// ── Context ─────────────────────────────────────────────────────────────────

interface ThemeContextValue {
  /** The raw preference: a specific theme id or 'system' */
  themePreference: string;
  /** The resolved (active) theme palette */
  activeTheme: ThemePalette;
  /** Whether the current theme is dark mode */
  isDark: boolean;
  /** Set a specific theme id, or 'system' to follow OS */
  setTheme: (id: string) => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  themePreference: 'system',
  activeTheme: THEMES[0],
  isDark: false,
  setTheme: () => {},
});

export function useTheme() {
  return useContext(ThemeContext);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function getSystemDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function resolveTheme(preference: string): ThemePalette {
  if (preference === 'system') {
    return getSystemDark()
      ? THEMES.find(t => t.id === 'default-dark')!
      : THEMES.find(t => t.id === 'default-light')!;
  }
  return THEMES.find(t => t.id === preference) ?? THEMES[0];
}

function applyTheme(palette: ThemePalette) {
  const root = document.documentElement;

  // Toggle dark class for Tailwind
  root.classList.toggle('dark', palette.mode === 'dark');

  // Apply CSS custom properties
  for (const [key, value] of Object.entries(palette.colors)) {
    root.style.setProperty(key, value);
  }
}

// ── Provider ────────────────────────────────────────────────────────────────

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [preference, setPreference] = useState<string>(
    () => localStorage.getItem('theme-preference') ?? 'system',
  );

  const activeTheme = resolveTheme(preference);
  const isDark = activeTheme.mode === 'dark';

  useEffect(() => {
    applyTheme(activeTheme);
    localStorage.setItem('theme-preference', preference);
  }, [preference, activeTheme]);

  // Listen for OS theme changes when in 'system' mode
  useEffect(() => {
    if (preference !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => applyTheme(resolveTheme('system'));
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [preference]);

  const setTheme = (id: string) => setPreference(id);

  return (
    <ThemeContext.Provider value={{ themePreference: preference, activeTheme, isDark, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}
