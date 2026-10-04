import { useCallback, useEffect, useState } from 'react';

/**
 * Dark by default; light when a photographer's eyes or their room want it.
 *
 * The theme is a `data-theme` attribute on the root element rather than a class,
 * because the stylesheet's override blocks read it as exactly that, and it is
 * persisted in localStorage inside a try/catch for the same reason the tour is:
 * storage that refuses the write only costs a repeat visit to the default.
 * "auto" follows the operating system and is the shipped default, so a fresh
 * install never has to choose.
 */

export type ThemeChoice = 'auto' | 'dark' | 'light';

const THEME_KEY = 'aura.theme';

const NEXT: Record<ThemeChoice, ThemeChoice> = {
  auto: 'dark',
  dark: 'light',
  light: 'auto',
};

const LABEL: Record<ThemeChoice, string> = {
  auto: 'Theme: follow the system',
  dark: 'Theme: dark',
  light: 'Theme: light',
};

function readStoredTheme(): ThemeChoice {
  try {
    const stored = window.localStorage.getItem(THEME_KEY);
    return stored === 'dark' || stored === 'light' ? stored : 'auto';
  } catch {
    return 'auto';
  }
}

export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement;
  if (choice === 'auto') {
    root.removeAttribute('data-theme');
  } else {
    root.dataset.theme = choice;
  }
}

export function ThemeToggle(): JSX.Element {
  const [choice, setChoice] = useState<ThemeChoice>(readStoredTheme);

  useEffect(() => {
    applyTheme(choice);
    try {
      if (choice === 'auto') {
        window.localStorage.removeItem(THEME_KEY);
      } else {
        window.localStorage.setItem(THEME_KEY, choice);
      }
    } catch {
      // The attribute is already applied; persistence is the convenience.
    }
  }, [choice]);

  const cycle = useCallback(() => {
    setChoice((current) => NEXT[current]);
  }, []);

  return (
    <button type="button" className="theme-toggle" title={LABEL[choice]} onClick={cycle}>
      {choice === 'auto' ? '◐ Auto' : choice === 'dark' ? '☾ Dark' : '☀ Light'}
    </button>
  );
}
