// @vitest-environment happy-dom
import { act, useImperativeHandle, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { liveBridge } from './liveBridge';

// Asking a teammate's Oby by @ in the message box (docs/contracts/ask-an-oby.md, "Asking by @ in the
// message box", ASK-37 to 39): the real Composer, spheres.ts and live.ts, main's bridge
// (window.app.obid.call) stood in for as obid, joined to an Obysphere.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
// the box's words: a plain field holding them, telling of an @ being typed as the real editor does
const editor = vi.hoisted(() => ({ mention: [] as string[] }));
vi.mock('../MessageEditor', () => ({
  default: function Field({ label, onText, onMention, handle }: { label: string; onText: (t: string) => void; onMention?: (q: string | null) => void; handle: React.Ref<unknown> }) {
    const field = useRef<HTMLTextAreaElement>(null);
    useImperativeHandle(handle, () => ({
      text: () => field.current?.value ?? '', clear: () => { if (field.current) field.current.value = ''; },
      setText: (t: string) => { if (field.current) field.current.value = t; }, focus: () => {}, blur: () => {},
      mention: (name: string) => { editor.mention.push(name); },
    }));
    return <textarea ref={field} aria-label={label} data-box="" onChange={(e) => { const v = e.currentTarget.value; onText(v); onMention?.(/(?:^|\s)@([\w./-]*)$/.exec(v)?.[1] ?? null); }} />;
  },
}));

let root: Root | null = null;
vi.setConfig({ testTimeout: 120_000 });
beforeAll(async () => { await import('../Composer'); await import('../live'); await import('../spheres'); }, 300_000);
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
  localStorage.clear();
  editor.mention = [];
});

type Member = { obi: string; owner: string; accepts: string[]; lookalike?: string[] };
const at = '2026-11-18T10:00:00Z';
const STATUS = (capabilities: string[]) => ({ runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities });
const convo = { id: 'c1', title: 'Billing', pinned: false, created: at, lastActivity: at, cursor: '', folders: [] };
const JOINED = { state: 'joined', url: 'http://127.0.0.1:8787', sphere: 'e2e', owner: 'wellington', obiId: 'obi-wellington' };

type Props = Partial<Parameters<typeof import('../Composer').default>[0]>;
async function box(members: Member[] | null, props: Props = {}, capabilities = ['composer-v1', 'ask-mentions-v1']) {
  // spheres.ts reads obid as it loads: a fresh one per test, on this test's obid
  vi.resetModules();
  const calls: { method: string; path: string; body?: unknown }[] = [];
  const call = vi.fn(async (method: string, path: string, body?: unknown) => {
    calls.push({ method, path, body });
    if (path === '/v1/obid/sphere') return { status: 200, text: JSON.stringify(members ? JOINED : { state: 'none', url: null, sphere: null }) };
    if (path === '/v1/obid/sphere/members') return members ? { status: 200, text: JSON.stringify(members) } : { status: 409, text: '{"error":"obid hasn\'t joined a Sphere."}' };
    if (method === 'POST' && path === '/v1/plans') return { status: 200, text: JSON.stringify({ kind: 'reply', text: 'Asked.', jobs: [], conversation: 'c1' }) };
    if (method === 'GET' && path.startsWith('/v1/conversation')) return { status: 200, text: '[]' };
    return { status: 404, text: '{"error":"Not found."}' };
  });
  const bridge = liveBridge(call);
  (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, onFullScreen: () => () => {}, onSphere: () => () => {} };
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { default: Composer } = await import('../Composer');
  const live = await import('../live');
  await import('../spheres');
  const { askableNow } = await import('../askable');
  live.connect();
  bridge.sync({ status: STATUS(capabilities), conversations: [convo] });
  // spheres.ts reads the membership and its members as it loads
  await vi.waitFor(() => expect(calls.some((c) => c.path === (members ? '/v1/obid/sphere/members' : '/v1/obid/sphere'))).toBe(true));
  if (members?.length) await vi.waitFor(() => expect(askableNow().length).toBeGreaterThan(0));
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  // a conversation's box sends as tellChat does on obid
  const onSend = (text: string, att: Parameters<typeof live.ask>[4]) => live.ask(text, 'c1', undefined, undefined, att);
  await act(async () => root!.render(<Composer label="Reply to Oby" talk draft="conversation:c1" conversation="c1" onSend={onSend} {...props} />));
  return { calls, live };
}
const button = (label: RegExp) => [...document.querySelectorAll('button')].find((b) => label.test(b.getAttribute('aria-label') ?? b.textContent ?? ''));
const click = (b: Element) => act(async () => { b.dispatchEvent(new MouseEvent('click', { bubbles: true })); });
const type = (text: string) => act(async () => {
  const field = document.querySelector<HTMLTextAreaElement>('[data-box]')!;
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, text);
  field.dispatchEvent(new Event('input', { bubbles: true }));
});
const listed = () => [...document.querySelectorAll('.mention-panel [role="option"] .add-item-text')].map((o) => o.textContent);
const send = () => act(async () => { document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });

const DANA: Member = { obi: 'obi-dana', owner: 'dana', accepts: ['question', 'review_request'] };
const SAM: Member = { obi: 'obi-sam', owner: 'sam', accepts: ['review_request'] };

describe('asking by @ in the message box', () => {
  it('ask_37_at_lists_the_live_members', async () => {
    await box([DANA, SAM, { obi: 'obi-dima', owner: 'dima', accepts: ['question'] }]);
    await type('@d');
    expect(listed()).toEqual(['dana’s Oby', 'dima’s Oby']);
    await type('@da');
    expect(listed()).toEqual(['dana’s Oby']);
    await click(document.querySelector('.mention-panel [role="option"]')!);
    expect(editor.mention).toEqual(['dana']);
    // sam's card takes no questions: not listed
    await type('@s');
    expect(listed()).toEqual([]);
    // + › Add other Obys reads the same members
    await click(button(/^Add to the message$/)!);
    await click(button(/Add other Obys/)!);
    expect([...document.querySelectorAll('.add-list [role="option"] .add-item-text')].map((o) => o.textContent)).toEqual(['dana’s Oby', 'dima’s Oby']);
  });

  it('ask_37_out_of_a_sphere_at_lists_no_one', async () => {
    await box(null);
    await type('@d');
    expect(listed()).toEqual([]);
    await click(button(/^Add to the message$/)!);
    expect(button(/Add other Obys/)).toBeUndefined();
  });

  it('ask_38_a_lookalike_shows_as_such', async () => {
    await box([
      { obi: 'obi-dima', owner: 'dima', accepts: ['question'], lookalike: ['dirna'] },
      { obi: 'obi-dirna', owner: 'dirna', accepts: ['question'], lookalike: ['dima'] },
      DANA,
    ]);
    await type('@d');
    expect(listed()).toEqual(['dima’s ObyNot dirna’s Oby', 'dirna’s ObyNot dima’s Oby', 'dana’s Oby']);
  });

  it('ask_39_a_mention_goes_as_ask', async () => {
    const { calls } = await box([DANA, { obi: 'obi-sam', owner: 'sam', accepts: ['question'] }]);
    await type('@dana @Sam are you on billing? @nobody');
    await send();
    const plans = () => calls.filter((c) => c.method === 'POST' && c.path === '/v1/plans');
    await vi.waitFor(() => expect(plans()).toHaveLength(1));
    // each member once, by owner id; someone who isn't a member is just words
    expect(plans()[0].body).toMatchObject({ said: '@dana @Sam are you on billing? @nobody', conversation: 'c1', ask: ['dana', 'sam'] });
  });

  it('ask_39_words_with_no_mention_ask_no_one', async () => {
    const { calls } = await box([DANA]);
    await type('are you on billing, dana?');
    await send();
    await vi.waitFor(() => expect(calls.filter((c) => c.path === '/v1/plans')).toHaveLength(1));
    expect(calls.find((c) => c.path === '/v1/plans')!.body).not.toHaveProperty('ask');
  });

  it('ask_39_an_obid_without_ask_mentions_gets_no_ask', async () => {
    const { calls } = await box([DANA], {}, ['composer-v1']);
    await type('@dana are you on billing?');
    await send();
    await vi.waitFor(() => expect(calls.filter((c) => c.path === '/v1/plans')).toHaveLength(1));
    expect(calls.find((c) => c.path === '/v1/plans')!.body).not.toHaveProperty('ask');
  });
});
