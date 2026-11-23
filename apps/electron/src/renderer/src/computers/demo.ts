// The picker on the demo data (?everything, and ?everything&computers=<screen> for one of the
// design's screens, docs/design/computer-picker/): a ComputersApi that stands in for obid, keeping
// its list here and answering as the design says obid will, a moment later each time.
import { Refusal, type ComputersApi } from './obid';
import type { Computer, ComputerList, Invitation, RouteKind } from './view';
import type { Ui } from './store';

export const STUDIO: Computer = { id: 'studio', name: 'Studio', os: 'mac', state: 'idle' };
export const DEVBOX: Computer = { id: 'devbox', name: 'devbox', os: 'linux', state: 'idle' };
const STUDIO_LAN = '192.168.1.30', STUDIO_TAILNET = '100.101.3.4';
/** the design's link: version 1, devbox, one pinned route */
export const DEMO_LINK = 'oby-computer:eyJ2IjoxLCJuYW1lIjoiZGV2Ym94Iiwicm91dGVzIjpbeyJ1cmwiOiJodHRwczovLzE5Mi4xNjguMS4zMDo4NDQzIn1dfQ';
/** a one link (COMPUTERS-29), devbox's, with a dummy secret: what Studio's window hands another Mac */
export const DEMO_ONE_LINK = 'oby-computer:AkJCQkJCQkJCQkJCQkJCQkJ_OpHCXQ5EuAxuofeT0msQfzqRwl0ORLgMbqH3k9JrEATAqAEoIPs';
/** this Mac's own one link in the demo, Studio's two addresses in it */
const STUDIO_ONE_LINK = 'oby-computer:AkJCQkJCQkJCQkJCQkJCQkJ_OpHCXQ5EuAxuofeT0msQfzqRwl0ORLgMbqH3k9JrEATAqAEeIPsEZGUDBCD7';
export const DEMO_FINGERPRINT = '7F3A91C25D0E44B80C6EA1F793D26B10';
export const DEMO_CODE = '482913';
const NOT_A_LINK = 'That isn’t a link from Oby.';

/** obid's words when the code isn't the one shown (COMPUTERS-13, proposed). */
export const codeRefused = (name: string) => `That code doesn’t match. Check it on ${name}.`;

const connected = (c: Computer, kind: RouteKind = 'lan'): Computer => ({ ...c, state: 'connected', route: { kind, address: kind === 'lan' ? STUDIO_LAN : undefined }, from: undefined, tried: undefined });
const idle = (c: Computer): Computer => ({ ...c, state: 'idle', route: undefined, from: undefined, tried: undefined });

/** The demo's obid: its list, and how long each answer takes (0 in a test that drives the clock). */
export function demoApi(start: ComputerList, pace = 700): ComputersApi {
  let list = start;
  const followers = new Set<(l: ComputerList) => void>();
  const later = (ms = pace) => new Promise<void>((r) => setTimeout(r, ms));
  const set = (next: ComputerList) => { list = next; for (const f of followers) f(list); };
  const change = (id: string, f: (c: Computer) => Computer) => set({ ...list, computers: list.computers.map((c) => (c.id === id ? f(c) : c)) });
  return {
    list: async () => list,
    follow: (on) => { followers.add(on); return () => followers.delete(on); },
    addOverSsh: async (host, progress) => {
      progress({ step: 'ssh', state: 'doing' }); await later(); progress({ step: 'ssh', state: 'done' });
      progress({ step: 'pair', state: 'doing' }); await later(); progress({ step: 'pair', state: 'done' });
      progress({ step: 'routes', state: 'doing' }); await later(); progress({ step: 'routes', state: 'done' });
      // the name its Oby gives: the demo's Studio answers as Studio, anything else by its host
      const at = host.split('@').pop()!.trim();
      const known = at.toLowerCase() === 'studio' ? STUDIO : at.toLowerCase() === 'devbox' ? DEVBOX : null;
      const id = known?.id ?? at.toLowerCase().replace(/[^a-z0-9]+/g, '-');
      if (!list.computers.some((c) => c.id === id)) set({ ...list, computers: [...list.computers, known ?? { id, name: at, os: 'mac', state: 'idle' }] });
      return id;
    },
    readLink: async (link) => {
      await later(pace / 2);
      if (link.trim() === DEMO_ONE_LINK) return { name: 'devbox', fingerprint: DEMO_FINGERPRINT, needsCode: false };
      const name = nameIn(link);
      if (!name) throw new Refusal(NOT_A_LINK);
      return { name, fingerprint: DEMO_FINGERPRINT, needsCode: true };
    },
    addWithCode: async (link, code) => {
      await later();
      const one = link.trim() === DEMO_ONE_LINK;
      const name = one ? 'devbox' : nameIn(link) ?? '';
      if (!one && (code ?? '').replace(/\D/g, '') !== DEMO_CODE) throw new Refusal(codeRefused(name));
      const id = name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
      if (!list.computers.some((c) => c.id === id)) set({ ...list, computers: [...list.computers, { id, name, os: name === 'devbox' ? 'linux' : 'mac', state: 'idle' }] });
      return id;
    },
    invite: async () => demoInvitation(),
    setReach: async () => undefined,
    use: async (id) => {
      await later();
      set({ current: id, computers: list.computers.map((c) => (c.id === id ? connected(c) : idle(c))) });
    },
    retry: async (id) => {
      change(id, (c) => ({ ...c, state: 'reconnecting', tried: undefined, from: undefined }));
      await later(pace * 2);
      change(id, (c) => connected(c));
    },
    rename: async (id, name) => change(id, (c) => ({ ...c, name })),
    remove: async (id) => { await later(pace / 2); set({ ...list, computers: list.computers.filter((c) => c.id !== id) }); },
  };
}

// the demo reads a link as obid would: `oby-computer:` and base64url JSON with a name
function nameIn(link: string): string | null {
  const m = /^oby-computer:([A-Za-z0-9_-]+)$/.exec(link.trim());
  if (!m) return null;
  try {
    const json = JSON.parse(atob(m[1].replace(/-/g, '+').replace(/_/g, '/'))) as { v?: number; name?: unknown };
    return json.v === 1 && typeof json.name === 'string' && json.name ? json.name : null;
  } catch {
    return null;
  }
}

export function demoInvitation(now = Date.now()): Invitation {
  return {
    name: 'Studio', link: STUDIO_ONE_LINK, fingerprint: DEMO_FINGERPRINT, expiresAt: now + 10 * 60_000,
    reach: [{ kind: 'lan', address: STUDIO_LAN, on: true }, { kind: 'tailnet', address: STUDIO_TAILNET, on: true }],
  };
}

/** The design's screens, by number: the list as obid would have it, and what the window has open. */
export function demoScreen(screen: string | null): { list: ComputerList; ui: Partial<Ui> } {
  const only: ComputerList = { current: 'this', computers: [] };
  const onStudio = (s: Partial<Computer>): ComputerList => ({ current: 'studio', computers: [{ ...connected(STUDIO), ...s }, DEVBOX] });
  switch (screen) {
    case '01': return { list: only, ui: { menu: true } };
    case '02': return { list: only, ui: { sheet: { kind: 'add', how: 'ssh', host: 'studio' } } };
    case '03': return { list: only, ui: { sheet: { kind: 'adding', host: 'studio', steps: { ssh: 'done', pair: 'doing', routes: 'later' } } } };
    case '03b': return { list: only, ui: { sheet: { kind: 'adding', host: 'studio', steps: { ssh: 'doing', pair: 'later', routes: 'later' }, trouble: 'studio’s host key isn’t one you’ve accepted. Check it, then run `ssh studio` once in a terminal to accept it.' } } };
    case '04': return { list: only, ui: { sheet: { kind: 'add', how: 'code', link: DEMO_ONE_LINK } } };
    case '05': return { list: only, ui: { sheet: { kind: 'code' } } };
    case '06': return { list: onStudio({}), ui: { menu: true } };
    case '07': return { list: onStudio({ state: 'reconnecting', from: 'lan', route: { kind: 'ssh' } }), ui: {} };
    case '08': return {
      list: onStudio({ state: 'lost', route: undefined, tried: [
        { route: 'lan', what: STUDIO_LAN, why: 'no answer' }, { route: 'tailnet', what: STUDIO_TAILNET, why: 'no answer' }, { route: 'ssh', why: 'studio didn’t answer over SSH.' },
      ] }),
      ui: {},
    };
    case '09': return { list: { current: 'this', computers: [STUDIO, DEVBOX] }, ui: { switching: { to: 'This Mac', from: 'Studio' } } };
    case '10': return { list: onStudio({}), ui: { sheet: { kind: 'remove', id: 'studio' } } };
    default: return { list: only, ui: {} };
  }
}
