import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';

export type Editor = 'vscode' | 'cursor' | 'webstorm' | 'none';
export type Theme = 'dark' | 'light';
export const EDITORS: Record<Editor, string> = { vscode: 'VS Code', cursor: 'Cursor', webstorm: 'WebStorm', none: 'None (copy the path)' };

function stored<T extends string>(key: string, fallback: T): T {
  try {
    return (localStorage.getItem(`brain:${key}`) as T | null) ?? fallback;
  } catch {
    return fallback;
  }
}

function store(key: string, value: string): void {
  try { localStorage.setItem(`brain:${key}`, value); } catch { /* private mode: this session only */ }
}

interface Prefs { editor: Editor; setEditor(e: Editor): void; theme: Theme; setTheme(t: Theme): void }
const PrefsContext = createContext<Prefs | null>(null);

/** Per-browser preferences: the editor file links open in, and the theme. */
export function PrefsProvider({ children }: { children: ReactNode }) {
  const [editor, setEditor] = useState<Editor>(() => stored('editor', 'vscode'));
  const [theme, setTheme] = useState<Theme>(() => stored('theme', 'dark'));
  useEffect(() => store('editor', editor), [editor]);
  useEffect(() => {
    store('theme', theme);
    document.documentElement.classList.toggle('dark', theme === 'dark');
  }, [theme]);
  const value = useMemo(() => ({ editor, setEditor, theme, setTheme }), [editor, theme]);
  return <PrefsContext.Provider value={value}>{children}</PrefsContext.Provider>;
}

export function usePrefs(): Prefs {
  const ctx = useContext(PrefsContext);
  if (!ctx) throw new Error('usePrefs outside PrefsProvider');
  return ctx;
}

export function editorUrl(editor: Editor, root: string, file: string, line?: number): string | null {
  const abs = `${root}/${file}`;
  if (editor === 'vscode') return `vscode://file${abs}${line ? `:${line}` : ''}`;
  if (editor === 'cursor') return `cursor://file${abs}${line ? `:${line}` : ''}`;
  if (editor === 'webstorm') return `webstorm://open?file=${encodeURIComponent(abs)}${line ? `&line=${line}` : ''}`;
  return null;
}
