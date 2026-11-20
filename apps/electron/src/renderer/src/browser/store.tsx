// The browser panel's one source of what is true: the browser's state (as obid's contract shapes
// it) and the window's own (is the panel shown, full width). Moved from the lab
// (tools/obid-spec/lab-ui/browser/src/store.tsx), the same interface in two modes:
//   demo  the design's boards on the demo data (?everything&browser=<n>, demo.tsx): the calls
//         change the state here;
//   obid  obid's /v1/browser through main, which holds the token and signs the owner's acts
//         (BROWSERAPP-8 on): until then the panel isn't offered on obid.
import { createContext, useContext, useMemo, useState, type ReactNode } from 'react';
import { BROWSER_DEFAULT_SETTINGS } from '../../../shared/obid-wire';
import { everything } from '../lean';
import type { BrowserApi, BrowserState, Tab } from './view';
import { demoBoard } from './demo';

/** The window's own state: not the browser's, never sent anywhere. */
export type Ui = {
  /** the browser panel shown beside the conversation */
  open: boolean;
  /** the page across the window, the conversation folded to one line */
  full: boolean;
  /** the panel's width as the owner left it (it gives way so the conversation keeps 420px) */
  width: number;
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

export type Browser = {
  /** whether this window offers the browser at all (the globe, the panel) */
  offered: boolean;
  mode: 'demo' | 'obid';
  s: BrowserState;
  ui: Ui;
  setUi: (u: Partial<Ui>) => void;
  api: BrowserApi;
  /** the demo's words for the conversation folded to one line, while the page is full width */
  peek: { agent: Tab['owner']; title: string; line: string } | null;
  active: Tab | undefined;
};

/** The panel's width the first time, and its narrowest (BROWSERAPP-3). */
export const PANEL_DEFAULT = 640, PANEL_MIN = 360;

export const UI0: Ui = { open: true, full: false, width: PANEL_DEFAULT, gaveBack: false, card: null, typed: 0, filled: null, tipClosed: false };
const EMPTY: BrowserState = { tabs: [], active: null, asks: [], downloads: [], settings: BROWSER_DEFAULT_SETTINGS };

const NONE = async () => undefined;
// outside a BrowserProvider (a screen drawn on its own, in a test): no browser offered
const OFF: Browser = {
  offered: false, mode: 'obid', s: EMPTY, ui: { ...UI0, open: false }, setUi: () => undefined, peek: null, active: undefined,
  api: { take: NONE, giveBack: NONE, secret: NONE, saveCopy: NONE, setSettings: NONE, select: NONE },
};
const Ctx = createContext<Browser>(OFF);
export const useBrowser = () => useContext(Ctx);

/** ?everything&browser=<n>: board n of the design, on the demo data. */
export const demoNumber = () => (everything ? Number(new URLSearchParams(location.search).get('browser')) || null : null);

export function BrowserProvider({ children, board = demoNumber() }: { children: ReactNode; board?: number | null }) {
  const demo = board ? demoBoard(board) : null;
  const [s, setS] = useState<BrowserState>(demo?.state ?? EMPTY);
  const [ui, setUiState] = useState<Ui>({ ...UI0, ...(demo ? demo.ui : { open: false }) });
  const setUi = (u: Partial<Ui>) => setUiState((o) => ({ ...o, ...u }));

  const api = useMemo<BrowserApi>(() => {
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
  }, []);

  const browser: Browser = {
    offered: !!demo, mode: 'demo', s, ui, setUi, api, peek: demo?.peek ?? null,
    active: s.tabs.find((t) => t.id === s.active),
  };
  return <Ctx.Provider value={browser}>{children}</Ctx.Provider>;
}
