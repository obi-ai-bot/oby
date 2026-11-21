// The picker's one way to obid (docs/contracts/computers-app.md, "Interfaces"). Every call the
// picker makes goes through a ComputersApi: the demo's (demo.ts) stands in for obid on the design's
// screens; obid's is here, on the Mac's own routes (docs/api/remote.md, "This Mac's computers"):
//   list, follow  GET /v1/computers, and the `computers` and `computer.route` events of this Mac's
//                 own stream (main/live.ts follows it beside the computer in use's)
//   readLink      POST /v1/computers/link {link}
//   addWithCode   POST /v1/computers {link, code}, signed by its sheet's Add computer (shared/words.ts)
//   use           PUT /v1/computers/current {id | "this"}: main hears its answer and moves every
//                 call and the stream to that computer (main/obid.ts, Oby-Computer)
//   retry         POST /v1/computers/{id}/retry
//   rename        PATCH /v1/computers/{id} {name}
//   remove        DELETE /v1/computers/{id}, signed by its dialog's button
// Not on obid yet (the design's, COMPUTERS-n): adding over SSH (its steps, `computer.adding`), this
// computer's own code for another Mac, and its reach switches. Each answers that it isn't in Oby
// yet, in words the owner reads, and his controls stay where his screens have them.
import type { AddProgress, Computer, ComputerList, ComputerState, Invitation, LinkRead, RouteKind, Tried } from './view';
import type { ComputerWire, ComputersWire, Event } from '../../../shared/obid-wire';

/** obid's events about the window's computers, as the page's live store passes them on (live.ts, onComputerEvent). */
export type ComputerEvents = (f: (e: Extract<Event, { kind: 'computers' | 'computerRoute' }>) => void) => () => void;

/** obid's refusal, in its own words (`say`), shown as it says it. */
export class Refusal extends Error {
  constructor(readonly say: string) { super(say); }
}

export interface ComputersApi {
  list(): Promise<ComputerList>;
  /** every change to the list, as obid sends it; a function that stops following */
  follow(on: (list: ComputerList) => void): () => void;
  /** adds the SSH host the person typed; the steps as they happen; the new computer's id, or a Refusal */
  addOverSsh(host: string, progress: (p: AddProgress) => void): Promise<string>;
  /** the name and fingerprint a link holds, or a Refusal: the window never parses a link itself */
  readLink(link: string): Promise<LinkRead>;
  addWithCode(link: string, code: string): Promise<string>;
  /** this computer's code, for another Mac to add it */
  invite(): Promise<Invitation>;
  setReach(kind: RouteKind, on: boolean): Promise<void>;
  /** the window's computer: one of the list's, or "this" */
  use(id: 'this' | string): Promise<void>;
  /** probes every route now */
  retry(id: string): Promise<void>;
  rename(id: string, name: string): Promise<void>;
  remove(id: string): Promise<void>;
}

// Words for what obid doesn't do yet (proposed, Sangam's to change)
export const NOT_YET = {
  ssh: 'Adding a computer over SSH isn’t in Oby yet. Add it with a code instead.',
  code: 'This computer’s code isn’t in Oby yet. Add it from the other computer over SSH, once Oby can.',
} as const;

// obid's route kinds, as the window names them (a loopback route is this Mac's own network)
const KINDS: Record<string, RouteKind> = { lan: 'lan', loopback: 'lan', tailnet: 'tailnet', vpn: 'vpn', public: 'public', ssh: 'ssh' };
const kindOf = (k: string): RouteKind => KINDS[k] ?? 'public';
// obid's states only grow: one this build doesn't know reads as idle
const STATES = new Set<ComputerState>(['connected', 'lost', 'idle']);
const triedOf = (t: { route: string; what?: string; why: string }[]): Tried[] => t.map((r) => ({ route: kindOf(r.route), ...(r.what ? { what: r.what } : {}), why: r.why }));
export function computerOf(c: ComputerWire): Computer {
  const state = STATES.has(c.state as ComputerState) ? c.state as ComputerState : 'idle';
  return {
    id: c.id, name: c.name, os: c.os === 'linux' ? 'linux' : 'mac', state,
    ...(c.route && state === 'connected' ? { route: { kind: kindOf(c.route.kind), ...(c.route.address ? { address: c.route.address } : {}) } } : {}),
    ...(c.tried && state === 'lost' ? { tried: triedOf(c.tried) } : {}),
  };
}
export const listOf = (w: ComputersWire): ComputerList => ({ current: w.current, computers: w.computers.map(computerOf) });

type Call = (method: string, path: string, body?: unknown) => Promise<{ status: number; text: string }>;
// obid's answer, or its refusal in its own words (`error`), as a Refusal
async function asked<T>(call: Call, method: string, path: string, body?: unknown): Promise<T> {
  const { status, text } = await call(method, path, body);
  let value: unknown = null;
  try { value = text ? JSON.parse(text) : null; } catch { /* not JSON: said below */ }
  if (status === 0) throw new Refusal(text);
  if (status < 200 || status >= 300) {
    const said = (value as { error?: unknown } | null)?.error;
    throw new Refusal(typeof said === 'string' ? said : `Oby answered ${status}.`);
  }
  return value as T;
}
const id = encodeURIComponent;

/** obid's ComputersApi on `call` (the bridge's), with `events` its computers' events; null when obid serves no list of computers (an obid before them answers 404). */
export async function computersOn(call: Call, events: ComputerEvents): Promise<ComputersApi | null> {
  const first = await call('GET', '/v1/computers');
  if (first.status !== 200) return null;
  let latest: ComputerList;
  try { latest = listOf(JSON.parse(first.text) as ComputersWire); } catch { return null; }
  const ons = new Set<(l: ComputerList) => void>();
  const tell = (l: ComputerList) => { latest = l; ons.forEach((on) => on(l)); };
  // a route moved, or none answers: the computer as obid says, until its list comes again
  const change = (c: string, f: (x: Computer) => Computer) => tell({ ...latest, computers: latest.computers.map((x) => (x.id === c ? f(x) : x)) });
  events((e) => {
    if (e.kind === 'computers') return tell(listOf(e.list));
    const r = e.route;
    if (r.change === 'moved') change(r.id, (x) => ({ ...x, state: 'connected', route: { kind: kindOf(r.to) }, from: kindOf(r.from), tried: undefined }));
    else change(r.id, (x) => ({ ...x, state: 'lost', route: undefined, tried: triedOf(r.tried) }));
  });
  return {
    list: async () => latest,
    follow: (on) => { ons.add(on); return () => { ons.delete(on); }; },
    addOverSsh: async () => { throw new Refusal(NOT_YET.ssh); },
    readLink: async (link) => {
      const read = await asked<{ name: string; fingerprint: string }>(call, 'POST', '/v1/computers/link', { link });
      return { name: read.name, fingerprint: read.fingerprint };
    },
    addWithCode: async (link, code) => (await asked<ComputerWire>(call, 'POST', '/v1/computers', { link, code })).id,
    invite: async () => { throw new Refusal(NOT_YET.code); },
    setReach: async () => { throw new Refusal(NOT_YET.code); },
    use: async (to) => {
      try {
        tell(listOf(await asked<ComputersWire>(call, 'PUT', '/v1/computers/current', { id: to })));
      } catch (e) {
        // the list changed under it (no_such_computer): obid's list again, then its words
        const again = await call('GET', '/v1/computers');
        if (again.status === 200) { try { tell(listOf(JSON.parse(again.text) as ComputersWire)); } catch { /* the words below */ } }
        throw e;
      }
    },
    retry: async (c) => { const got = computerOf(await asked<ComputerWire>(call, 'POST', `/v1/computers/${id(c)}/retry`)); change(c, () => got); },
    rename: async (c, name) => { const got = computerOf(await asked<ComputerWire>(call, 'PATCH', `/v1/computers/${id(c)}`, { name })); change(c, () => got); },
    remove: async (c) => { await asked(call, 'DELETE', `/v1/computers/${id(c)}`); tell({ ...latest, computers: latest.computers.filter((x) => x.id !== c), ...(latest.current === c ? { current: 'this' } : {}) }); },
  };
}

/** obid's ComputersApi, once obid answers its list; null while obid serves none. */
export async function obidComputers(): Promise<ComputersApi | null> {
  const bridge = window.app?.obid;
  if (!bridge) return null;
  const { onComputerEvent } = await import('../live');
  return computersOn((m, p, b) => bridge.call(m, p, b), onComputerEvent);
}
