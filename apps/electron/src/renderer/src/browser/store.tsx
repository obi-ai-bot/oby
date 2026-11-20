// The browser panel's one source of what is true: the browser's state (as obid's contract shapes
// it) and the window's own (is the panel shown, full width). Moved from the lab
// (tools/obid-spec/lab-ui/browser/src/store.tsx), the same interface in two modes:
//   demo  the design's boards on the demo data (?everything&browser=<n>, demo.tsx): the calls
//         change the state here;
//   obid  obid's /v1/browser through main, which holds the token and signs the owner's acts
//         (live.ts, BROWSERAPP-8 to 10): offered once obid answers GET /v1/browser.
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { BROWSER_DEFAULT_SETTINGS } from '../../../shared/obid-wire';
import { everything, onObid } from '../lean';
import type { BrowserApi, BrowserState, Tab } from './view';
import { demoBoard } from './demo';
import { connectLive, type Live } from './live';
import { closeChanges, useChangesOpen } from '../ChangesPanel';

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
  /** on obid: the stream's latest frame for a tab, as an image URL, and its size in the page's px */
  frame: (tab: string) => { url: string; width: number; height: number } | null;
};

/** The panel's width the first time, and its narrowest (BROWSERAPP-3). */
export const PANEL_DEFAULT = 640, PANEL_MIN = 360;

export const UI0: Ui = { open: true, full: false, width: PANEL_DEFAULT, gaveBack: false, card: null, typed: 0, filled: null, tipClosed: false };
const NONE = async () => undefined;
const EMPTY: BrowserState = { tabs: [], active: null, asks: [], downloads: [], settings: BROWSER_DEFAULT_SETTINGS };

// outside a BrowserProvider (a screen drawn on its own, in a test): no browser offered
const OFF: Browser = {
  offered: false, mode: 'obid', s: EMPTY, ui: { ...UI0, open: false }, setUi: () => undefined, peek: null, active: undefined, frame: () => null,
  api: { take: NONE, giveBack: NONE, secret: NONE, answer: NONE, saveCopy: NONE, reveal: NONE, setSettings: NONE, select: NONE, stopMac: NONE, answerMac: NONE },
};
const Ctx = createContext<Browser>(OFF);
export const useBrowser = () => useContext(Ctx);

/** ?everything&browser=<n>: board n of the design, on the demo data. */
export const demoNumber = () => (everything ? Number(new URLSearchParams(location.search).get('browser')) || null : null);

export function BrowserProvider({ children, board = demoNumber(), live = onObid }: { children: ReactNode; board?: number | null; live?: boolean }) {
  const demo = board ? demoBoard(board) : null;
  const onLive = !demo && live && !!window.app?.browser;
  const [s, setS] = useState<BrowserState>(demo?.state ?? EMPTY);
  const [ui, setUiState] = useState<Ui>({ ...UI0, ...(demo ? demo.ui : { open: false }) });
  const setUi = (u: Partial<Ui>) => setUiState((o) => ({ ...o, ...u }));
  const [offered, setOffered] = useState(false);
  const [frames, setFrames] = useState<Record<string, { url: string; width: number; height: number }>>({});
  const conn = useRef<Live | null>(null);
  // the latest state, for calls made from the api below (made once per mode)
  const latest = useRef(s);
  latest.current = s;
  // one panel at the window's right at a time (BROWSERAPP-3): a task's Changes opening puts the
  // browser away, and the browser opening closes Changes
  const changes = useChangesOpen();
  useEffect(() => { if (changes) setUiState((o) => (o.open ? { ...o, open: false, full: false } : o)); }, [changes]);
  useEffect(() => { if (ui.open) closeChanges(); }, [ui.open]);

  useEffect(() => {
    const app = window.app;
    if (!onLive || !app?.browser) return;
    const browser = app.browser;
    const c = connectLive({
      offered: setOffered,
      // the tab shown stays the one you chose while it's there
      state: (next) => setS((o) => ({ ...next, active: o.active && next.tabs.some((t) => t.id === o.active) ? o.active : next.active })),
      frame: (tab, url, size) => setFrames((f) => ({ ...f, [tab]: { url, ...size } })),
      problem: (why) => { if (why) console.warn(`The browser: ${why}`); },
    }, { call: (m, p, b) => app.obid.call(m, p, b), follow: (fn) => browser.follow(fn), watch: (tab, fn) => browser.watch(tab, fn), fill: (tab, field, value, ask) => browser.fill(tab, field, value, ask) });
    conn.current = c;
    return () => { c.close(); conn.current = null; };
  }, [onLive]);

  const api = useMemo<BrowserApi>(() => {
    if (onLive) {
      // the owner's acts, signed by main on their own click (shared/words.ts)
      return {
        take: (t) => conn.current?.take(t) ?? Promise.resolve(),
        giveBack: async (t) => { await conn.current?.giveBack(t); setUi({ gaveBack: true }); },
        // the password or code goes to main, which fills it on your click and keeps nothing (BROWSERAPP-12)
        secret: async (t, field, value, ask) => { setUi({ card: null, typed: 0 }); const at = await conn.current?.fill(t, field, value, ask); if (at === true) setUi({ filled: { tab: t, field, length: value.length } }); },
        answer: (a, choice) => conn.current?.answer(a, choice) ?? Promise.resolve(),
        saveCopy: (d) => conn.current?.saveCopy(d) ?? Promise.resolve(),
        reveal: async (d) => { const name = latest.current.downloads.find((x) => x.id === d)?.savedAs; if (name) await window.app?.browser?.reveal(name); },
        setSettings: (x) => conn.current?.setSettings(x) ?? Promise.resolve(),
        // desktop control's state and Stop come with its own contract (agent-desktop): nothing on obid yet
        stopMac: NONE, answerMac: NONE,
        select: async (t) => setS((o) => ({ ...o, active: t })),
        input: (t, e) => conn.current?.input(t, e) ?? Promise.resolve(),
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
      answer: async (id) => setS((o) => ({ ...o, asks: o.asks.filter((a) => a.id !== id) })),
      saveCopy: async (id) => setS((o) => ({ ...o, downloads: o.downloads.map((d) => (d.id === id ? { ...d, saved: true } : d)) })),
      reveal: NONE,
      stopMac: async () => setS((o) => ({ ...o, mac: null })),
      answerMac: async () => setS((o) => ({ ...o, macAsk: null })),
      setSettings: async (x) => setS((o) => ({ ...o, settings: { ...o.settings, ...x } })),
      select: async (id) => setS((o) => ({ ...o, active: id })),
    };
  }, [onLive]);

  const browser: Browser = {
    offered: !!demo || (onLive && offered), mode: onLive ? 'obid' : 'demo', s, ui, setUi, api, peek: demo?.peek ?? null,
    active: s.tabs.find((t) => t.id === s.active),
    frame: (t) => frames[t] ?? null,
  };
  return <Ctx.Provider value={browser}>{children}</Ctx.Provider>;
}
