// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { starterRule } from '../../../shared/policy';
import type { PolicyAnswer, Rule } from '../../../shared/sphere';

// Who may ask whom, in Settings › Obysphere (docs/contracts/sphere-policy.md, POLICY-5 and 6): the
// page's half, with main's bridge (window.app.spherePolicy, sphereSetPolicy) stood in for.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));

let root: Root | null = null;
let host: HTMLElement;

afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  delete (window as { app?: unknown }).app;
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = '';
});

const answer = (version: number, rules: Rule[], admin: boolean): PolicyAnswer => ({
  sphere: 'demo', policy: { version, updated_at: '2026-11-14T00:00:00Z', rules }, admin, refused: null, why: null,
});

// main's bridge, the read answered at once; a write answered when the test says
function bridge(read: PolicyAnswer | null) {
  let written!: (a: PolicyAnswer | null) => void;
  const write = new Promise<PolicyAnswer | null>((r) => { written = r; });
  const spherePolicy = vi.fn(async () => read);
  const sphereSetPolicy = vi.fn((_version: number) => write);
  return { app: { spherePolicy, sphereSetPolicy }, spherePolicy, sphereSetPolicy, write: async (a: PolicyAnswer | null) => { await act(async () => { written(a); await write; }); } };
}

async function show(app: object) {
  (window as { app?: unknown }).app = app;
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search: '' });
  const { SphereRules } = await import('../SphereSettings');
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<SphereRules />));
}

const group = () => [...host.querySelectorAll('section')].find((s) => s.querySelector('h2')?.textContent === 'Who may ask whom');
const letAsk = () => [...host.querySelectorAll('button')].find((b) => b.textContent === 'Let members ask');

describe('POLICY-5', () => {
  it('policy_5_an_admin_turns_on_members_ask', async () => {
    const b = bridge(answer(0, [], true));
    await show(b.app);
    expect(b.spherePolicy).toHaveBeenCalledTimes(1);
    expect(group()!.textContent).toContain('Nobody may ask yet');
    expect(letAsk()).toBeDefined();
    await act(async () => letAsk()!.click());
    expect(b.sphereSetPolicy).toHaveBeenCalledTimes(1);
    expect(b.sphereSetPolicy).toHaveBeenCalledWith(0);
    await b.write(answer(1, [starterRule()], true));
    expect(group()!.textContent).toContain('Every member may ask every other member’s Oby');
    expect(group()!.textContent).toContain('Questions and reviews, in every repository.');
    expect(group()!.textContent).not.toContain('Nobody may ask yet');
    expect(letAsk()).toBeUndefined();
  });

  it('policy_5: a refused click says why, keeps the rules shown and the button', async () => {
    const other: Rule = { id: 'leads', from_owner: 'ana', to_owner: '*', actions: ['review_request'], repos: ['acme/app'] };
    const b = bridge(answer(3, [other], true));
    await show(b.app);
    await act(async () => letAsk()!.click());
    expect(b.sphereSetPolicy).toHaveBeenCalledWith(3);
    await b.write({ sphere: 'demo', policy: null, admin: false, refused: 'not_admin', why: 'Only the Obysphere’s admins can change its rules.' });
    expect(group()!.querySelector('.set-problem')!.textContent).toBe('Only the Obysphere’s admins can change its rules.');
    expect(group()!.textContent).toContain('ana may ask every other member’s Oby');
    expect(group()!.textContent).toContain('Reviews, in acme/app.');
    expect(letAsk()).toBeDefined();
    expect(document.querySelector('[role="alertdialog"], dialog')).toBeNull();
  });

  it('policy_5: a changed answer shows the rules as they are now, with why', async () => {
    const b = bridge(answer(0, [], true));
    await show(b.app);
    await act(async () => letAsk()!.click());
    await b.write({ ...answer(2, [starterRule()], true), refused: 'changed', why: 'The rules changed meanwhile: these are the rules as they are now.' });
    expect(group()!.textContent).toContain('Every member may ask every other member’s Oby');
    expect(group()!.querySelector('.set-problem')!.textContent).toBe('The rules changed meanwhile: these are the rules as they are now.');
    expect(letAsk()).toBeUndefined();
  });

  it('policy_5: an unreadable answer says so, and nothing shows before the first answer', async () => {
    let read!: (a: PolicyAnswer | null) => void;
    const reading = new Promise<PolicyAnswer | null>((r) => { read = r; });
    await show({ spherePolicy: () => reading, sphereSetPolicy: vi.fn() });
    expect(host.innerHTML).toBe('');
    await act(async () => { read(null); await reading; });
    expect(group()!.querySelector('.set-problem')!.textContent).toBe('Oby couldn’t read the Obysphere’s rules.');
  });

  it('policy_5: on a live Obysphere the group is between Members and What your Oby tells', async () => {
    const b = bridge(answer(0, [], true));
    const call = vi.fn(async (_method: string, path: string) => {
      if (path === '/v1/obid/sphere') return { status: 200, text: JSON.stringify({ state: 'joined', url: 'https://demo.example', sphere: 'demo', owner: 'owner-a', obiId: 'obi-1' }) };
      if (path === '/v1/obid/sphere/members') return { status: 200, text: '[]' };
      if (path.startsWith('/v1/obid/sharing')) return { status: 200, text: JSON.stringify({ rules: [] }) };
      return { status: 404, text: '{}' };
    });
    (window as { app?: unknown }).app = { ...b.app, obid: { call }, onSphere: () => () => {} };
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('location', { search: '' });
    const { useSpheres } = await import('../spheres');
    const { default: SphereSettings } = await import('../SphereSettings');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    function Page() {
      const live = useSpheres().find((s) => s.live);
      return live ? <SphereSettings id={live.id} leaving={false} onClose={() => {}} onGone={() => {}} /> : null;
    }
    await act(async () => root!.render(<Page />));
    await act(async () => { await vi.waitFor(() => expect(group()).toBeDefined()); });
    const titles = [...host.querySelectorAll('section h2')].map((h) => h.textContent);
    expect(titles).toEqual(['Members · 1', 'Who may ask whom', 'What your Oby tells']);
  });
});

describe('POLICY-6', () => {
  it('policy_6_a_member_reads_only', async () => {
    const b = bridge(answer(1, [starterRule()], false));
    await show(b.app);
    expect(group()!.textContent).toContain('Every member may ask every other member’s Oby');
    expect(group()!.textContent).toContain('Only its admins can change these.');
    expect(group()!.querySelector('button')).toBeNull();
    await act(async () => root!.unmount());
    vi.resetModules();

    const none = bridge(answer(0, [], false));
    await show(none.app);
    expect(group()!.textContent).toContain('Nobody may ask yet');
    expect(group()!.querySelector('button, input, select')).toBeNull();
    expect(b.sphereSetPolicy).not.toHaveBeenCalled();
    expect(none.sphereSetPolicy).not.toHaveBeenCalled();
  });

  it('policy_6: a rule Oby can’t put in words shows under its id', async () => {
    const classed: Rule = { ...starterRule(), id: 'leads-only', from_classes: ['lead'] };
    const b = bridge(answer(1, [classed], false));
    await show(b.app);
    expect(group()!.textContent).toContain('leads-only');
    expect(group()!.textContent).toContain('Oby can’t put this rule in words yet.');
    expect(group()!.textContent).not.toContain('Every member may ask');
  });

  it('policy_6: a rule reads its actions in its order, and names its owners and repositories', async () => {
    const { ruleWords } = await import('../spheres');
    expect(ruleWords({ id: 'r', from_owner: 'ana', to_owner: 'bo', actions: ['question', 'handoff', 'run_request'], repos: ['acme/*', 'me/app'] }))
      .toEqual({ label: 'ana may ask bo’s Oby', about: 'Questions, handoffs and runs, in acme/*, me/app.' });
    expect(ruleWords({ id: 'r', from_owner: '*', to_owner: '*', actions: ['share_offer', 'introduction'], repos: ['*'] }))
      .toEqual({ label: 'Every member may ask every other member’s Oby', about: 'Share offers and introductions, in every repository.' });
    for (const odd of [{ actions: ['question', 'toString'] }, { actions: [] }, { repos: [] }, { to_owner: 3 }, { to_classes: ['lead'] }]) {
      expect(ruleWords({ ...starterRule(), ...odd }).about).toBe('Oby can’t put this rule in words yet.');
    }
  });

  it('policy_6: an owner id is text, never markup', async () => {
    const b = bridge(answer(1, [{ id: '<i>r</i>', from_owner: '<b>x</b>', to_owner: '*', actions: ['question'], repos: ['<u>a</u>/b'] }], false));
    await show(b.app);
    expect(group()!.textContent).toContain('<b>x</b> may ask every other member’s Oby');
    expect(group()!.textContent).toContain('Questions, in <u>a</u>/b.');
    expect(group()!.querySelector('b, i, u')).toBeNull();
  });
});
