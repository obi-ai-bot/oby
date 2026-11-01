import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import type { BrainData, Module, Route, Screen } from '../../generator/types';

export type * from '../../generator/types';

/** The Brain plus the lookups every view needs. */
export interface Brain {
  data: BrainData;
  route: Map<string, Route>;
  module: Map<string, Module>;
  screen: Map<string, Screen>;
  /** Route id → the screens whose data or actions reach it. */
  screensOf: Map<string, Array<{ screen: Screen; api: string; via: string | null; line: number }>>;
}

function index(data: BrainData): Brain {
  const screensOf = new Map<string, Array<{ screen: Screen; api: string; via: string | null; line: number }>>();
  for (const s of data.screens) {
    for (const l of s.live) {
      if (!l.route) continue;
      if (!screensOf.has(l.route)) screensOf.set(l.route, []);
      const list = screensOf.get(l.route)!;
      if (!list.some(x => x.screen.file === s.file)) list.push({ screen: s, api: l.api, via: l.via, line: l.line });
    }
  }
  return {
    data,
    route: new Map(data.routes.map(r => [r.id, r])),
    module: new Map(data.modules.map(m => [m.id, m])),
    screen: new Map(data.screens.map(s => [s.file, s])),
    screensOf,
  };
}

interface BrainState {
  brain: Brain | null;
  error: string | null;
  loading: boolean;
  reload: () => Promise<void>;
  regenerate: () => Promise<void>;
}

const BrainContext = createContext<BrainState | null>(null);

export function BrainProvider({ children }: { children: ReactNode }) {
  const [brain, setBrain] = useState<Brain | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch('/brain.json', { cache: 'no-store' });
      if (!res.ok) throw new Error(await res.text());
      setBrain(index((await res.json()) as BrainData));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const regenerate = useCallback(async () => {
    setLoading(true);
    await fetch('/brain/regenerate', { method: 'POST' }).catch(() => undefined);
    await reload();
  }, [reload]);

  useEffect(() => {
    void reload();
    // The dev server reads the code again when a file changes and tells the page.
    const onUpdate = () => void reload();
    import.meta.hot?.on('brain:update', onUpdate);
    return () => import.meta.hot?.off('brain:update', onUpdate);
  }, [reload]);

  const value = useMemo(() => ({ brain, error, loading, reload, regenerate }), [brain, error, loading, reload, regenerate]);
  return <BrainContext.Provider value={value}>{children}</BrainContext.Provider>;
}

export function useBrainState(): BrainState {
  const ctx = useContext(BrainContext);
  if (!ctx) throw new Error('useBrainState outside BrainProvider');
  return ctx;
}

/** The loaded Brain (views render only after it loads). */
export function useBrain(): Brain {
  const { brain } = useBrainState();
  if (!brain) throw new Error('The Brain isn\'t loaded yet');
  return brain;
}

export function shortFile(file: string): string {
  return file
    .replace(/^core\/crates\/obid\/src\//, 'obid/')
    .replace(/^core\/crates\/obid\/tests\//, 'obid/tests/')
    .replace(/^apps\/electron\/src\/renderer\/src\//, 'app/')
    .replace(/^apps\/electron\/src\//, 'electron/');
}
