// The computer picker's one source of what is true (docs/contracts/computers-app.md): obid's list of
// the window's computers, as its ComputersApi gives it (obid.ts, or demo.ts on the demo data), and
// the window's own state (the menu, a sheet, a switch under way). One store for the window: the
// button lives in the side panel and the states show on Home, so both read it here.
import { useEffect, useSyncExternalStore } from 'react';
import { everything, onObid } from '../lean';
import { Refusal, obidComputers, type ComputersApi } from './obid';
import { demoApi, demoScreen } from './demo';
import { THIS_MAC, type AddStep, type Computer, type ComputerList, type RouteKind } from './view';

export type StepState = 'later' | 'doing' | 'done';

/** The one sheet open over the window, if any. */
export type Sheet =
  | { kind: 'add'; how: 'ssh' | 'code'; host?: string; link?: string; code?: string }
  | { kind: 'adding'; host: string; steps: Record<AddStep, StepState>; trouble?: string }
  | { kind: 'code' }
  | { kind: 'remove'; id: string };

/** The window's own state: never sent anywhere. */
export type Ui = {
  menu: boolean;
  sheet: Sheet | null;
  /** the row whose ⋯ is open */
  more: string | null;
  /** the row whose name is a field (Rename…) */
  renaming: string | null;
  /** a switch under way: where to, and the computer it leaves */
  switching: { to: string; from: string } | null;
};

export type Computers = {
  /** whether this window offers the picker at all */
  offered: boolean;
  list: ComputerList;
  /** when each computer started reconnecting, by the window's clock (the line waits 2 s) */
  since: Record<string, number>;
  ui: Ui;
};

const UI0: Ui = { menu: false, sheet: null, more: null, renaming: null, switching: null };
const OFF: Computers = { offered: false, list: { current: 'this', computers: [] }, since: {}, ui: UI0 };

let state: Computers = OFF;
let api: ComputersApi | null = null;
let stop: (() => void) | null = null;
let started = false;
const listeners = new Set<() => void>();

function set(next: Computers) {
  state = next;
  // the window dims its side panel and message box while it isn't on a computer that answers
  // (computers.css): a switch under way, or the computer in use lost
  const dim = state.ui.switching ? 'switching' : current()?.state === 'lost' ? 'lost' : null;
  if (typeof document !== 'undefined') {
    if (dim) document.documentElement.dataset.computer = dim;
    else delete document.documentElement.dataset.computer;
  }
  for (const l of listeners) l();
}
const setUi = (u: Partial<Ui>) => set({ ...state, ui: { ...state.ui, ...u } });

function take(list: ComputerList) {
  const since: Record<string, number> = {};
  for (const c of list.computers) if (c.state === 'reconnecting') since[c.id] = state.since[c.id] ?? Date.now();
  set({ ...state, list, since });
}

/** Starts the picker on an api (obid's, or the demo's), with what the window has open. */
export async function startComputers(on: ComputersApi | null, ui: Partial<Ui> = {}) {
  stop?.();
  api = on; started = true;
  if (!on) { set(OFF); return; }
  const list = await on.list();
  set({ offered: true, list, since: {}, ui: { ...UI0, ...ui } });
  take(list);
  stop = on.follow(take);
}

/** For tests: back to nothing offered. */
export function resetComputers() {
  stop?.(); stop = null; api = null; started = false;
  set(OFF);
}

function startOnce() {
  if (started) return;
  started = true;
  if (everything) {
    const { list, ui } = demoScreen(new URLSearchParams(location.search).get('computers'));
    void startComputers(demoApi(list), ui);
  } else if (onObid) {
    // whatever obid answers, or doesn't: nothing offered (This Mac only), never an error unhandled
    void obidComputers().then((on) => startComputers(on)).catch(() => startComputers(null));
  }
}

export function useComputers(): Computers {
  useEffect(startOnce, []);
  return useSyncExternalStore((l) => { listeners.add(l); return () => listeners.delete(l); }, () => state);
}

// ---- reading it ----

/** The computer the window is on: undefined for This Mac. */
export function current(s: Computers = state): Computer | undefined {
  return s.list.current === 'this' ? undefined : s.list.computers.find((c) => c.id === s.list.current);
}
export const currentName = (s: Computers = state) => current(s)?.name ?? THIS_MAC;
/** The message box's words to Oby (COMPUTERSAPP-13, screen 06): on another computer, its name; This Mac's as before. */
export function messageOby(s: Computers = state) {
  const on = s.offered ? current(s) : undefined;
  return on ? `Message Oby on ${on.name}…` : 'Message Oby…';
}
/** Whether the picker's own words stand for the window's connection now (screens 08, 09): a switch under way, or the computer in use unreachable. Then the engine's line waits: obid's stream to it fails while it's lost, and the window says so once, in the design's words. */
export const computerSays = (s: Computers = state) => s.offered && (!!s.ui.switching || current(s)?.state === 'lost');
export const nameOf = (id: 'this' | string, s: Computers = state) => (id === 'this' ? THIS_MAC : s.list.computers.find((c) => c.id === id)?.name ?? id);

// ---- the owner's clicks ----

export const toggleMenu = () => setUi({ menu: !state.ui.menu, more: null, renaming: null });
export const closeMenu = () => setUi({ menu: false, more: null, renaming: null });
export const showMore = (id: string | null) => setUi({ more: id, renaming: null });
export const startRename = (id: string) => setUi({ more: null, renaming: id });
export const stopRename = () => setUi({ renaming: null });
export const openSheet = (sheet: Sheet) => setUi({ menu: false, more: null, renaming: null, sheet });
export const closeSheet = () => setUi({ sheet: null });

/** A row picked, or Back to This Mac: the window switches, saying so for the moment it takes. */
export async function use(id: 'this' | string) {
  closeMenu();
  if (!api || id === state.list.current) return;
  setUi({ switching: { to: nameOf(id), from: currentName() } });
  try {
    await api.use(id);
  } finally {
    setUi({ switching: null });
  }
}

/** Over SSH: the sheet ticks off obid's steps; added, the window switches to it. */
export async function addOverSsh(host: string) {
  if (!api) return;
  const steps: Record<AddStep, StepState> = { ssh: 'doing', pair: 'later', routes: 'later' };
  openSheet({ kind: 'adding', host, steps: { ...steps } });
  try {
    const id = await api.addOverSsh(host, (p) => {
      steps[p.step] = p.state;
      if (state.ui.sheet?.kind === 'adding') setUi({ sheet: { kind: 'adding', host, steps: { ...steps } } });
    });
    if (state.ui.sheet?.kind !== 'adding') return;  // cancelled
    closeSheet();
    await use(id);
  } catch (e) {
    if (state.ui.sheet?.kind === 'adding') setUi({ sheet: { kind: 'adding', host, steps: { ...steps }, trouble: e instanceof Refusal ? e.say : (e as Error).message } });
  }
}

export const readLink = (link: string) => api!.readLink(link);

/** With a link (and its code, for a link of version 1): added, the window switches to it. A refusal is thrown back to the sheet, which stays. */
export async function addWithCode(link: string, code?: string) {
  const id = await api!.addWithCode(link, code);
  closeSheet();
  await use(id);
}

export const invite = () => api!.invite();
export const setReach = (kind: RouteKind, on: boolean) => api!.setReach(kind, on);
export const retry = (id: string) => api?.retry(id);

/** Rename…'s field kept (Enter, or a click elsewhere); once Esc ended the renaming, nothing changes. */
export async function rename(id: string, name: string) {
  if (state.ui.renaming !== id) return;
  stopRename();
  const n = name.trim();
  if (api && n && n !== nameOf(id)) await api.rename(id, n);
}

/** Remove: the computer in use goes back to This Mac first. */
export async function remove(id: string) {
  closeSheet();
  if (!api) return;
  if (state.list.current === id) await use('this');
  await api.remove(id);
}
