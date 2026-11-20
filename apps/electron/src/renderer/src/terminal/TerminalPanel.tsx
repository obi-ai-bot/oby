import { useEffect, useRef } from 'react';
import { Plus, X } from 'lucide-react';
import { useChangesOpen } from '../ChangesPanel';
import { useBrowser } from '../browser/store';
import { makeScreen, type Screen } from './xterm';
import {
  addTerminal, bridge, closeTab, closeTerminalPanel, follow, hideTerminal, opened, read, showTab, unfollow, useTerminals,
  type Place, type Tab,
} from './store';
import './terminal.css';

// The terminal panel (docs/contracts/terminal.md), as T3 Code's terminal drawer: the owner's own shell
// in a task's or a conversation's working copy, beside the page at the window's right, where Changes
// and the browser show (one at a time). Its tabs are that place's shells: + another, × on a tab ends
// it, × on the panel ends them all. Esc is the shell's, as in any terminal.

// how a terminal is drawn (tests stand one in)
let make: () => Screen = makeScreen;
export const drawWith = (f: () => Screen) => { make = f; };

export function TerminalView({ k, tab, place, id, visible }: { k: string; tab: Tab; place: Place; id: string; visible: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const screen = useRef<Screen | null>(null);
  // its shell, once main started it
  const term = useRef<string | null>(null);

  useEffect(() => {
    const b = bridge(), s = make();
    screen.current = s;
    s.open(box.current!);
    let live = true, stopReading = () => {};
    // what's typed goes to main, which lets through only what it saw typed (TERMINAL-5)
    s.onData((data) => { if (term.current) b?.write(term.current, data); });
    // the shell starts at the size the box holds, measured first, so its first prompt fits
    const first = s.measure();
    if (first) s.resize(first.cols, first.rows);
    const at = s.size();
    if (!b) { opened(k, tab.key, { error: 'No terminal here: open Oby’s app.' }); return () => { s.dispose(); }; }
    b.open(place, id, at.cols, at.rows).then((answer) => {
      // closed while it opened: its shell ends at once, never left running
      if (!live) { if ('term' in answer) b.close(answer.term); return; }
      opened(k, tab.key, answer);
      if (!('term' in answer)) return;
      term.current = answer.term;
      stopReading = read(answer.term, (data) => s.write(data));
      // resized while it opened: the shell gets the size the box has now
      const now = s.size();
      if (now.cols !== at.cols || now.rows !== at.rows) b.resize(answer.term, now.cols, now.rows);
      s.focus();
    }, (e: unknown) => { if (live) opened(k, tab.key, { error: e instanceof Error ? e.message : String(e) }); });
    return () => {
      live = false;
      stopReading();
      // gone from the page: its shell ends (TERMINAL-4)
      if (term.current) b.close(term.current);
      term.current = null;
      s.dispose();
      screen.current = null;
    };
  }, []);

  // its box resized (the window, the panel, shown again): the screen and the shell get the new size,
  // once a frame; never while hidden, when the box has no size to give
  useEffect(() => {
    const el = box.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    let frame = 0;
    const fit = () => {
      frame = 0;
      const s = screen.current, size = s?.measure();
      if (!s || !size) return;
      const was = s.size();
      if (was.cols === size.cols && was.rows === size.rows) return;
      s.resize(size.cols, size.rows);
      if (term.current) bridge()?.resize(term.current, size.cols, size.rows);
    };
    const ro = new ResizeObserver(() => { if (!frame) frame = requestAnimationFrame(fit); });
    ro.observe(el);
    return () => { ro.disconnect(); if (frame) cancelAnimationFrame(frame); };
  }, []);

  // shown: typing goes to it
  useEffect(() => { if (visible) screen.current?.focus(); }, [visible]);

  return (
    <div className="terminal-view" hidden={!visible} data-tab={tab.key}>
      {tab.error && <p className="terminal-error" role="alert">{tab.error}</p>}
      <div className="terminal-screen" ref={box} hidden={!!tab.error} />
    </div>
  );
}

export default function TerminalPanel() {
  const { shown, sessions } = useTerminals();
  const changes = useChangesOpen();
  const browser = useBrowser();
  useEffect(() => { follow(); return unfollow; }, []);
  // one panel at the window's right at a time (BROWSERAPP-3): Changes or the browser opening puts the
  // terminal away (its shells keep going), and the terminal opening puts the browser away
  useEffect(() => { if (changes) hideTerminal(); }, [changes]);
  useEffect(() => { if (browser.ui.open) hideTerminal(); }, [browser.ui.open]);
  useEffect(() => { if (shown && browser.ui.open) browser.setUi({ open: false, full: false }); }, [shown]);

  const places = Object.entries(sessions);
  if (!places.length) return null;
  const s = shown ? sessions[shown] : undefined;
  return (
    <aside className="terminal-panel" role="complementary" aria-label="Terminal" hidden={!s}>
      {s && (
        <header className="terminal-head">
          <div className="terminal-title">
            <h2>Terminal</h2>
            <span className="terminal-place">{s.title}{s.tabs.find((t) => t.key === s.active)?.name ? ` · ${s.tabs.find((t) => t.key === s.active)!.name}` : ''}</span>
          </div>
          <button type="button" className="terminal-icon" aria-label="New terminal" title="New terminal" onClick={addTerminal}><Plus size={16} strokeWidth={1.8} /></button>
          <button type="button" className="terminal-icon" aria-label="Close Terminal" title="Close Terminal" onClick={closeTerminalPanel}><X size={16} strokeWidth={1.8} /></button>
        </header>
      )}
      {s && s.tabs.length > 1 && (
        <div className="terminal-tabs" role="tablist" aria-label="Terminals">
          {s.tabs.map((t) => (
            <div key={t.key} className={`terminal-tab${t.key === s.active ? ' is-active' : ''}`}>
              <button type="button" role="tab" aria-selected={t.key === s.active} onClick={() => showTab(t.key)}>{`Terminal ${t.n}`}</button>
              <button type="button" className="terminal-tab-close" aria-label={`Close Terminal ${t.n}`} onClick={() => closeTab(shown!, t.key)}><X size={12} strokeWidth={1.8} /></button>
            </div>
          ))}
        </div>
      )}
      <div className="terminal-body">
        {places.flatMap(([k, p]) => p.tabs.map((t) => (
          <TerminalView key={t.key} k={k} tab={t} place={p.place} id={p.id} visible={k === shown && t.key === p.active} />
        )))}
      </div>
    </aside>
  );
}
