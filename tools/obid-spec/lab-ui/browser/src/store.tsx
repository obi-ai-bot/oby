// The screens' one source of what is true: the browser's state (as the contract shapes it) and the
// window's own (is the browser shown, full width). Two modes, one interface:
//   fixtures  every state on screen with no server: the calls change the state here;
//   live      lane A's API (live.ts): its tabs and events, and the page from agent-browser's stream.
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { BrowserApi, BrowserState, Tab } from './api/types';
import { connectLive, type Live } from './live';

/** The window's own state: not the browser's, never sent anywhere. */
export type Ui = {
  /** the browser panel shown beside the conversation */
  open: boolean;
  /** the page across the window, the conversation folded to one line */
  full: boolean;
  /** the log's line after you gave the tab back */
  gaveBack: boolean;
  /** the decision card over the message box: the agent's ask, or the password card */
  card: 'signIn' | 'password' | null;
  /** what the password card has in it: only its length, to draw the dots (the value goes nowhere) */
  typed: number;
  /** a page's field Oby filled: the field's name and how many characters went in */
  filled: { tab: string; field: string; length: number } | null;
  tipClosed: boolean;
};

export type Lab = {
  mode: 'fixtures' | 'live';
  s: BrowserState;
  ui: Ui;
  setUi: (u: Partial<Ui>) => void;
  api: BrowserApi;
  /** live: the stream's latest frame for a tab, as an image URL */
  frame: (tab: string) => string | null;
  /** live: why it isn't live (no API, refused), shown on the page */
  problem: string | null;
  active: Tab | undefined;
};

const Ctx = createContext<Lab | null>(null);
export const useLab = () => {
  const l = useContext(Ctx);
  if (!l) throw new Error('useLab outside LabProvider');
  return l;
};

export const UI0: Ui = { open: true, full: false, gaveBack: false, card: null, typed: 0, filled: null, tipClosed: false };

export function LabProvider({ start, ui: ui0, live, children }: { start: BrowserState; ui: Partial<Ui>; live: string | null; children: ReactNode }) {
  // live: nothing of the fixture's, only what the API says
  const [s, setS] = useState<BrowserState>(live ? { tabs: [], active: null, asks: [], downloads: [], settings: start.settings } : start);
  const [ui, setUiState] = useState<Ui>({ ...UI0, ...ui0 });
  const [problem, setProblem] = useState<string | null>(null);
  const [frames, setFrames] = useState<Record<string, string>>({});
  const conn = useRef<Live | null>(null);
  const setUi = (u: Partial<Ui>) => setUiState((o) => ({ ...o, ...u }));

  useEffect(() => {
    if (!live) return;
    const c = connectLive(live, {
      state: (next) => setS(next),
      frame: (tab, url) => setFrames((f) => ({ ...f, [tab]: url })),
      problem: setProblem,
    });
    conn.current = c;
    return () => c.close();
  }, [live]);

  const api = useMemo<BrowserApi>(() => {
    if (live) {
      return {
        take: (t) => conn.current!.take(t),
        giveBack: async (t) => { await conn.current!.giveBack(t); setUi({ gaveBack: true }); },
        secret: async (t, field, value) => { setUi({ card: null, typed: 0 }); await conn.current!.secret(t, field, value); setUi({ filled: { tab: t, field, length: value.length } }); },
        saveCopy: (d) => conn.current!.saveCopy(d),
        setSettings: (x) => conn.current!.setSettings(x),
        select: async (t) => setS((o) => ({ ...o, active: t })),
      };
    }
    const tab = (id: string, f: (t: Tab) => Tab) => setS((o) => ({ ...o, tabs: o.tabs.map((t) => (t.id === id ? f(t) : t)) }));
    return {
      take: async (id) => tab(id, (t) => ({ ...t, holder: 'you', hold: (t.hold ?? 0) + 1 })),
      giveBack: async (id) => { tab(id, (t) => ({ ...t, holder: t.owner.kind === 'agent' ? 'agent' : null })); setUi({ gaveBack: true }); },
      // the value is dropped here: only its length is kept, to draw the filled field
      secret: async (id, field, value) => {
        setUi({ filled: { tab: id, field, length: value.length }, card: null, typed: 0 });
        setS((o) => ({ ...o, asks: o.asks.filter((a) => a.tab !== id) }));
      },
      saveCopy: async (id) => setS((o) => ({ ...o, downloads: o.downloads.map((d) => (d.id === id ? { ...d, saved: true } : d)) })),
      setSettings: async (x) => setS((o) => ({ ...o, settings: { ...o.settings, ...x } })),
      select: async (id) => setS((o) => ({ ...o, active: id })),
    };
  }, [live]);

  const lab: Lab = {
    mode: live ? 'live' : 'fixtures', s, ui, setUi, api, problem,
    frame: (t) => frames[t] ?? null,
    active: s.tabs.find((t) => t.id === s.active),
  };
  return <Ctx.Provider value={lab}>{children}</Ctx.Provider>;
}
