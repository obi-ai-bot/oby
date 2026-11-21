import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { useBrowser } from './store';
import { Pointer } from './Panel';
import type { Tab } from './view';

// The agents' pages, each a real <webview> in the window (docs/plans/browser-t3.md, W2), as T3 Code
// hosts them (apps/web/src/browser/ElectronBrowserHost.tsx, BrowserSurfaceSlot.tsx): every agent
// tab's webview is mounted once, here, in a fixed wrapper, keyed and in a fixed order, because
// Electron destroys a moved webview's page. The panel draws a Slot where the page goes; the slot
// publishes its rectangle and the tab's webview sits on it. With no slot the webview stays mounted,
// off screen, so the agent goes on driving it. Main ties each to obid's tab (browser-register) and
// relays it (main/browserRelay.ts).

type Rect = { left: number; top: number; width: number; height: number };
const tabKey = (job: string, tab: string) => `${job}\0${tab}`;

// the slots on screen now, by tab: where each tab's page goes
const rects = new Map<string, Rect>();
const heard = new Set<() => void>();
const changed = () => { for (const f of heard) f(); };
const subscribe = (f: () => void) => { heard.add(f); return () => { heard.delete(f); }; };
const same = (a: Rect | undefined, b: Rect) => !!a && a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
function useRect(key: string): Rect | null {
  return useSyncExternalStore(subscribe, () => rects.get(key) ?? null);
}

/** where a tab's page goes, in the panel or across the window: its webview is drawn on it */
export function Slot({ tab }: { tab: Tab }) {
  const el = useRef<HTMLDivElement>(null);
  const key = tab.job ? tabKey(tab.job, tab.id) : '';
  useLayoutEffect(() => {
    const node = el.current;
    if (!node || !key) return;
    const measure = () => {
      const r = node.getBoundingClientRect();
      const next = { left: Math.round(r.left), top: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) };
      if (same(rects.get(key), next)) return;
      rects.set(key, next);
      changed();
    };
    measure();
    const seen = new ResizeObserver(measure);
    seen.observe(node);
    window.addEventListener('resize', measure);
    // the panel moves without resizing (the side panel docks, Full width): measured each frame it might
    let frame = 0;
    const tick = () => { measure(); frame = requestAnimationFrame(tick); };
    frame = requestAnimationFrame(tick);
    return () => { seen.disconnect(); window.removeEventListener('resize', measure); cancelAnimationFrame(frame); rects.delete(key); changed(); };
  }, [key]);
  return <div ref={el} className="br-slot" data-browser="slot" style={{ position: 'absolute', inset: 0 }} />;
}

// a crashed page comes back at most this often (T3: webviewCrashRecovery.ts)
const CRASHES = 3;
const CRASH_WINDOW_MS = 30_000;
const POINTER_MS = 700;

type WebviewEl = HTMLElement & { getWebContentsId(): number; getURL(): string };

function Hosted({ job, tab }: { job: string; tab: Tab }) {
  const key = tabKey(job, tab.id);
  const rect = useRect(key);
  const ref = useRef<WebviewEl | null>(null);
  // the address and partition it first opened with: the agent navigates from there, never a re-render
  const [first] = useState(() => ({ src: tab.src ?? 'about:blank', partition: tab.partition! }));
  const [mount, setMount] = useState({ n: 0, src: first.src });
  const crashes = useRef<number[]>([]);
  const [pointer, setPointer] = useState<{ x: number; y: number; at: number; click: boolean } | null>(null);
  const browser = window.app?.browser;

  // registered with main once per mount: the webview can attach before an effect runs (it did, 9 ms
  // after the tab event, in the live run), so it's asked at once too, and on its first page's dom-ready
  useLayoutEffect(() => {
    const w = ref.current;
    if (!w || !browser) return;
    let done = false;
    const attached = () => {
      if (done) return;
      let id: number;
      try { id = w.getWebContentsId(); } catch { return; /* not attached yet: did-attach comes */ }
      done = true;
      void browser.register(job, tab.id, id).then((why) => { if (why) console.warn(`The browser: ${why}`); });
    };
    attached();
    const gone = () => {
      // remounted at its last address, a few times at most (T3: 250 ms × 2ⁿ, 3 in 30 s)
      const now = Date.now();
      crashes.current = crashes.current.filter((t) => now - t < CRASH_WINDOW_MS);
      if (crashes.current.length >= CRASHES) return;
      crashes.current.push(now);
      let src = first.src;
      try { src = w.getURL() || first.src; } catch { /* gone already */ }
      setTimeout(() => setMount((m) => ({ n: m.n + 1, src })), 250 * 2 ** (crashes.current.length - 1));
    };
    w.addEventListener('did-attach', attached);
    w.addEventListener('dom-ready', attached);
    w.addEventListener('render-process-gone', gone);
    return () => { w.removeEventListener('did-attach', attached); w.removeEventListener('dom-ready', attached); w.removeEventListener('render-process-gone', gone); };
  }, [mount.n, job, tab.id, browser, first.src]);

  // on screen or not, as main and obid hear it (T3's `shown`)
  useEffect(() => { browser?.shown(job, tab.id, !!rect); }, [!!rect, job, tab.id, browser]);
  useEffect(() => () => { browser?.shown(job, tab.id, false); }, [job, tab.id, browser]);

  // the agent's pointer over its page: drawn, then faded (T3: AgentBrowserCursor)
  useEffect(() => browser?.pointer((p) => {
    if (p.threadId === job && p.tabId === tab.id) setPointer({ x: p.x, y: p.y, at: Date.now(), click: p.phase === 'click' });
  }), [job, tab.id, browser]);
  useEffect(() => {
    if (!pointer) return;
    const t = setTimeout(() => setPointer(null), POINTER_MS);
    return () => clearTimeout(t);
  }, [pointer]);

  // off screen, the page stays mounted and painting at the panel's usual size
  const place = rect ?? { left: -20_000, top: 0, width: 640, height: 720 };
  return (
    <div className="br-hosted" data-browser="webview" data-tab={tab.id} style={{ position: 'fixed', left: place.left, top: place.top, width: place.width, height: place.height }}>
      <webview key={mount.n} ref={(w) => { ref.current = w as WebviewEl | null; }} src={mount.src} partition={first.partition} style={{ width: '100%', height: '100%', display: 'flex' }} />
      {pointer && rect && <Pointer tab={tab} x={pointer.x} y={pointer.y} ring={pointer.click} />}
    </div>
  );
}

/** Every agent tab's webview, mounted once at the app's root (main.tsx), in a fixed order. */
export function BrowserWebviews() {
  const { mode, s } = useBrowser();
  if (mode !== 'obid' || !window.app?.browser?.register) return null;
  const tabs = s.tabs
    .filter((t): t is Tab & { job: string; partition: string } => t.owner.kind === 'agent' && !!t.job && !!t.partition)
    .sort((a, b) => (tabKey(a.job, a.id) < tabKey(b.job, b.id) ? -1 : 1));
  return <>{tabs.map((t) => <Hosted key={tabKey(t.job, t.id)} job={t.job} tab={t} />)}</>;
}
