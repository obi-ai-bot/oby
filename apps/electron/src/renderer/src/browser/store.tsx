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
import { OWN_PARTITION, OWN_THREAD, scratchPartition, type BrowserApi, type BrowserState, type Tab } from './view';
import { demoBoard } from './demo';
import { connectLive, shown, type Live, type PageState } from './live';
import { NotAnAddress, resolveAddress } from './address';
import { focusPage } from './Webviews';
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
};

/** The panel's width the first time, and its narrowest (BROWSERAPP-3). */
export const PANEL_DEFAULT = 640, PANEL_MIN = 360;

export const UI0: Ui = { open: true, full: false, width: PANEL_DEFAULT, gaveBack: false, card: null, typed: 0, filled: null, tipClosed: false };
const NONE = async () => undefined;
// the message box Give back returns the keyboard to: the conversation's (ChatView.tsx:279), else
// Home's (Home.tsx:408)
const MESSAGE_BOXES = ['.chat-layer:not(.is-covered):not(.is-leaving) > .composer [contenteditable="true"]', '.composer .composer-input'];
const EMPTY: BrowserState = { tabs: [], active: null, asks: [], downloads: [], settings: BROWSER_DEFAULT_SETTINGS };

// outside a BrowserProvider (a screen drawn on its own, in a test): no browser offered
const OFF: Browser = {
  offered: false, mode: 'obid', s: EMPTY, ui: { ...UI0, open: false }, setUi: () => undefined, peek: null, active: undefined,
  api: { take: NONE, giveBack: NONE, secret: NONE, answer: NONE, saveCopy: NONE, reveal: NONE, setSettings: NONE, select: NONE, stopMac: NONE, answerMac: NONE, nav: NONE, open: async () => null, newTab: NONE, close: NONE },
};

/**
 * An owner's own tab (BROWSERAPP-45 to 48), the window's alone: what it first opened (none while
 * blank, before its webview mounts) and main's last word on its page (`browser-nav-state`, keyed by
 * OWN_THREAD). Never sent to obid.
 */
type Own = { id: string; src: string | null; partition: string; page: PageState | null };
let owns = 0;
const ownId = () => `own-${Date.now().toString(36)}-${++owns}`;
/** an owner's own tab as the panel draws it: a Y, its page's host for a title, its address as main says it */
function ownTab(o: Own): Tab {
  const at = o.page?.url || o.src || '';
  const failed = o.page && !o.page.loading ? o.page.failed : null;
  return {
    id: o.id, owner: { kind: 'you' }, partition: o.partition, src: o.src ?? undefined, href: at || undefined,
    title: at ? shown(at).split('/')[0] : 'New tab', url: at ? shown(at) : '', secure: at.startsWith('https:'),
    state: failed ? 'failed' : 'ready', failedAt: failed ?? undefined, holder: null,
    canGoBack: o.page?.canGoBack ?? false, canGoForward: o.page?.canGoForward ?? false, pageLoading: o.page?.loading ?? false,
  };
}
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
  // the owner's own tabs, after obid's in the row
  const [own, setOwn] = useState<Own[]>([]);
  const ownNow = useRef(own);
  ownNow.current = own;
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
      state: (next) => setS((o) => ({ ...next, active: o.active && (next.tabs.some((t) => t.id === o.active) || ownNow.current.some((t) => t.id === o.active)) ? o.active : next.active })),
      // main's word on an owner's own tab's page: its address, behind, ahead, loading, failed
      own: (p) => setOwn((o) => o.map((t) => (t.id === p.tabId ? { ...t, page: p } : t))),
      problem: (why) => { if (why) console.warn(`The browser: ${why}`); },
    }, { call: (m, p, b) => app.obid.call(m, p, b), follow: (fn) => browser.follow(fn), fill: (tab, field, value, ask) => browser.fill(tab, field, value, ask), navState: (fn) => browser.navState(fn) });
    conn.current = c;
    return () => { c.close(); conn.current = null; };
  }, [onLive]);

  const api = useMemo<BrowserApi>(() => {
    if (onLive) {
      // the owner's acts, signed by main on their own click (shared/words.ts)
      return {
        // once obid takes the tab, the keyboard is on its page; once it gives it back, in the message
        // box (BROWSERAPP-40, 41), as T3 Code's Take control and Release control move it
        take: async (t) => {
          const job = latest.current.tabs.find((x) => x.id === t)?.job;
          if ((await conn.current?.take(t)) && job) focusPage(job, t);
        },
        giveBack: async (t) => {
          if (await conn.current?.giveBack(t)) MESSAGE_BOXES.map((css) => document.querySelector<HTMLElement>(css)).find(Boolean)?.focus();
          setUi({ gaveBack: true });
        },
        // the password or code goes to main, which fills it on your click and keeps nothing (BROWSERAPP-12)
        // the card and its strip stay until main has filled, so board 8's strip shows the whole fill
        secret: async (t, field, value, ask) => { const at = await conn.current?.fill(t, field, value, ask); setUi(at === true ? { card: null, typed: 0, filled: { tab: t, field, length: value.length } } : { card: null, typed: 0 }); },
        answer: (a, choice) => conn.current?.answer(a, choice) ?? Promise.resolve(),
        saveCopy: (d) => conn.current?.saveCopy(d) ?? Promise.resolve(),
        reveal: async (d) => { const name = latest.current.downloads.find((x) => x.id === d)?.savedAs; if (name) await window.app?.browser?.reveal(name); },
        setSettings: (x) => conn.current?.setSettings(x) ?? Promise.resolve(),
        // desktop control's state and Stop come with its own contract (agent-desktop): nothing on obid yet
        stopMac: NONE, answerMac: NONE,
        select: async (t) => setS((o) => ({ ...o, active: t })),
        // Back, Forward, Reload, Stop loading: main moves the tab's own page, unsigned (BROWSERAPP-30 to 35)
        nav: async (t, op) => {
          const job = ownNow.current.some((x) => x.id === t) ? OWN_THREAD : latest.current.tabs.find((x) => x.id === t)?.job;
          if (job === undefined) return;
          const why = await window.app?.browser?.nav(job, t, op);
          if (why) console.warn(`The browser: ${why}`);
        },
        // the address bar's words in the owner's own tab, made an address as T3 Code's are: a blank
        // tab's webview mounts at it, a tab with a page goes there through main (BROWSERAPP-45, 46)
        open: async (words, t, server) => {
          let url: string;
          try { url = resolveAddress(words); } catch (e) { return e instanceof NotAnAddress ? e.message : String(e); }
          const tab = t === undefined ? undefined : ownNow.current.find((x) => x.id === t);
          if (tab?.src) return (await window.app?.browser?.go?.(tab.id, url)) ?? null;
          const id = tab?.id ?? ownId();
          // a server from the list opens in a throwaway partition of its tab's own, never the owner's (BROWSERAPP-47)
          const partition = server ? scratchPartition(id) : OWN_PARTITION;
          setOwn((o) => (tab ? o.map((x) => (x.id === id ? { ...x, src: url, partition } : x)) : [...o, { id, src: url, partition, page: null }]));
          setS((o) => ({ ...o, active: id }));
          return null;
        },
        newTab: async () => {
          const id = ownId();
          setOwn((o) => [...o, { id, src: null, partition: OWN_PARTITION, page: null }]);
          setS((o) => ({ ...o, active: id }));
        },
        // its webview goes with it; the tab shown next is the one before it in the row, else the one after (BROWSERAPP-48)
        close: async (t) => {
          const row = [...latest.current.tabs.map((x) => x.id), ...ownNow.current.map((x) => x.id)];
          const at = row.indexOf(t);
          const next = at < 0 ? null : row[at - 1] ?? row[at + 1] ?? null;
          setOwn(ownNow.current.filter((x) => x.id !== t));
          setS((o) => (o.active === t ? { ...o, active: next } : o));
        },
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
      nav: NONE,
      // the owner's own tabs are pages of the window's, on obid only
      open: async () => null, newTab: NONE, close: NONE,
    };
  }, [onLive]);

  const shownState = own.length ? { ...s, tabs: [...s.tabs, ...own.map(ownTab)] } : s;
  const browser: Browser = {
    offered: !!demo || (onLive && offered), mode: onLive ? 'obid' : 'demo', s: shownState, ui, setUi, api, peek: demo?.peek ?? null,
    active: shownState.tabs.find((t) => t.id === s.active),
  };
  return <Ctx.Provider value={browser}>{children}</Ctx.Provider>;
}
