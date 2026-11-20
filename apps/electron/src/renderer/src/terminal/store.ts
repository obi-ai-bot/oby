import { useSyncExternalStore } from 'react';
import { closeChanges } from '../ChangesPanel';

// The terminals the window shows (docs/contracts/terminal.md), as T3 Code's thread terminals: each
// task or conversation has its own, kept while the panel shows another or is put away for Changes
// or the browser (one panel at the window's right at a time, BROWSERAPP-3), and ended when the
// panel's × closes them, their shell exits, or main ends them (the task removed, Oby quitting).
// main runs the shells (main/terminal.ts); the page sends it only where, a size and what's typed.

export type Place = 'task' | 'conversation';
export type Opened = { term: string; cwd: string; name: string } | { error: string };
// main's half, through the preload (window.app.terminal)
export type Bridge = {
  open(place: Place, id: string, cols: number, rows: number): Promise<Opened>;
  write(term: string, data: string): void;
  resize(term: string, cols: number, rows: number): void;
  close(term: string): void;
  onData(fn: (term: string, data: string) => void): () => void;
  onExit(fn: (term: string, code: number, signal: number | null) => void): () => void;
};
export const bridge = (): Bridge | undefined => (window as unknown as { app?: { terminal?: Bridge } }).app?.terminal;

// One tab: its number in its place, and once main answered, its shell (term) or why there's none
export type Tab = { key: number; n: number; term?: string; name?: string; error?: string };
export type Session = { place: Place; id: string; title: string; tabs: Tab[]; active: number };
type State = { shown: string | null; sessions: Record<string, Session> };

let state: State = { shown: null, sessions: {} };
let keys = 1;
const listeners = new Set<() => void>();
const set = (next: State) => { state = next; listeners.forEach((l) => l()); };
export const placeKey = (place: Place, id: string) => `${place}:${id}`;
export const useTerminals = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => state, () => state);
export const terminalState = () => state;

const withSession = (k: string, f: (s: Session) => Session | null): State => {
  const s = state.sessions[k];
  if (!s) return state;
  const next = f(s);
  const sessions = { ...state.sessions };
  if (next) sessions[k] = next; else delete sessions[k];
  return { shown: next || state.shown !== k ? state.shown : null, sessions };
};
const newTab = (s: Session): Tab => ({ key: keys++, n: Math.max(0, ...s.tabs.map((t) => t.n)) + 1 });

// Terminal, from a task's or a conversation's menu, on the owner's click (main opens a shell only
// right after one it saw, TERMINAL-1): the panel shows that place's terminals, a new one when it
// has none; Changes closes (the browser is put away by the panel)
export function openTerminal(place: Place, id: string, title: string) {
  const k = placeKey(place, id);
  const s = state.sessions[k] ?? { place, id, title, tabs: [], active: 0 };
  const tab = s.tabs.length ? null : newTab(s);
  set({ shown: k, sessions: { ...state.sessions, [k]: { ...s, title, ...(tab ? { tabs: [tab], active: tab.key } : {}) } } });
  closeChanges();
}
// another terminal in the shown place (+)
export function addTerminal() {
  const k = state.shown;
  if (!k) return;
  set(withSession(k, (s) => { const tab = newTab(s); return { ...s, tabs: [...s.tabs, tab], active: tab.key }; }));
}
export function showTab(key: number) {
  const k = state.shown;
  if (k) set(withSession(k, (s) => ({ ...s, active: key })));
}
// main answered a tab's open
export function opened(k: string, key: number, answer: Opened) {
  set(withSession(k, (s) => ({ ...s, tabs: s.tabs.map((t) => (t.key === key ? { ...t, ...answer } : t)) })));
}
// a tab closed (×): its view goes, and with it its shell (TerminalView ends it); the last one
// closes its place's panel
export function closeTab(k: string, key: number) {
  set(withSession(k, (s) => {
    const tabs = s.tabs.filter((t) => t.key !== key);
    if (!tabs.length) return null;
    const at = s.tabs.findIndex((t) => t.key === key);
    return { ...s, tabs, active: s.active === key ? tabs[Math.min(at, tabs.length - 1)].key : s.active };
  }));
}
// its shell ended by itself (exit, or main ended it): its tab goes, as T3 Code's does
export function exited(term: string) {
  for (const [k, s] of Object.entries(state.sessions)) {
    const tab = s.tabs.find((t) => t.term === term);
    if (tab) { set(withSession(k, (x) => { const tabs = x.tabs.filter((t) => t.key !== tab.key); return tabs.length ? { ...x, tabs, active: x.active === tab.key ? tabs[0].key : x.active } : null; })); return; }
  }
}
// × on the panel: every terminal of the place it shows goes, its shell with it, and the panel
// closes (TERMINAL-4)
export function closeTerminalPanel() {
  const k = state.shown;
  if (!k) return;
  const sessions = { ...state.sessions };
  delete sessions[k];
  set({ shown: null, sessions });
}
// put away for Changes or the browser: the shells keep going, shown again from the menu
export function hideTerminal() { if (state.shown) set({ ...state, shown: null }); }
// (tests) as at launch
export function resetTerminals() { state = { shown: null, sessions: {} }; keys = 1; listeners.forEach((l) => l()); }

// Output that came before its tab was listening (main answers the open first, but nothing is lost
// if it doesn't), handed over when it listens; and each shell's listener
const early = new Map<string, string>();
const readers = new Map<string, (data: string) => void>();
let following: (() => void) | null = null;
export function follow() {
  const b = bridge();
  if (following || !b) return;
  const offData = b.onData((term, data) => { const r = readers.get(term); if (r) r(data); else early.set(term, (early.get(term) ?? '') + data); });
  const offExit = b.onExit((term) => { early.delete(term); readers.delete(term); exited(term); });
  following = () => { offData(); offExit(); following = null; };
}
export function read(term: string, f: (data: string) => void) {
  readers.set(term, f);
  const before = early.get(term);
  if (before) { early.delete(term); f(before); }
  return () => { if (readers.get(term) === f) readers.delete(term); };
}
export function unfollow() { following?.(); early.clear(); readers.clear(); }
