// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('electron', () => ({ app: { getPath: () => '/nowhere' }, session: { fromPartition: () => ({}) }, webContents: { fromId: () => null } }));
const { FIELD_LENGTH, RELOAD_MS, focusField, typeInto } = await import('../../../main/browserGuest');
const { relayHost } = await import('../../../main/browserRelay');
type FillPage = Parameters<typeof typeInto>[0];

// Fill in in the tab's own page (main/browserGuest.ts typeInto, BROWSERAPP-43): the page loads
// again, the guest gets the keyboard, the ask's field is focused by the one script that names it,
// and the value is typed in with Input.insertText (or the guest's own insertText), never part of any
// script; only how many characters the field holds is read back.

const K = { threadId: 'j1', tabId: 't1' };
const PW = 'fill-test-value-9031';

function fakePage(o: { loads?: boolean; lengths?: number[]; typing?: (text: string) => void } = {}) {
  const steps: string[] = [];
  const frames: { method: string; params?: Record<string, unknown> }[] = [];
  const lengths = [...(o.lengths ?? [[...PW].length, [...PW].length])];
  let loaded: (() => void) | null = null;
  const page: FillPage = {
    debugger: {
      sendCommand: async (method: string, params?: object) => {
        frames.push({ method, params: params as Record<string, unknown> });
        const expression = (params as { expression?: string } | undefined)?.expression;
        steps.push(method === 'Runtime.evaluate' ? (expression === FIELD_LENGTH ? 'length' : 'focusField') : method);
        if (method === 'Input.insertText') o.typing?.((params as { text: string }).text);
        if (method !== 'Runtime.evaluate') return {};
        if (expression === FIELD_LENGTH) return { result: { value: lengths.shift() ?? 0 } };
        // the field itself, by reference, when the page has it focused; else null
        return { result: [focusField('#pw'), focusField('#code', true)].includes(expression!) ? { type: 'object', subtype: 'node', objectId: 'field-1' } : { type: 'object', subtype: 'null', value: null } };
      },
    },
    reload: () => { steps.push('reload'); if (o.loads !== false) setTimeout(() => loaded?.(), 0); },
    focus: () => { steps.push('focus'); },
    insertText: async (text: string) => { steps.push(`insertText ${text === PW ? 'the value' : text}`); },
    once: (_e, f) => { loaded = f; },
    off: () => { loaded = null; },
  };
  const host = { started: 0, ended: 0, filled: [] as string[], fields: [] as (string | undefined)[] };
  const relay = { fillStarts: () => { host.started++; }, fillEnds: () => { host.ended++; }, filled: (_k: unknown, v: string, f?: string) => { steps.push('filled'); host.filled.push(v); host.fields.push(f); } };
  return { page, steps, frames, host, relay };
}

describe('BROWSERAPP-43: Fill in focuses only the ask\'s own field, and the value is in no script', () => {
  afterEach(() => { document.body.innerHTML = ''; delete (globalThis as { pwned?: boolean }).pwned; vi.useRealTimers(); });
  const run = (field: string, code = false): unknown => { try { return (0, eval)(focusField(field, code)); } catch (e) { return e; } };

  it('browserapp_43_a_password_goes_only_into_a_password_input_a_code_into_a_text_box_too', () => {
    document.body.innerHTML = '<input id="name"><input id="pw" type="password"><textarea id="note"></textarea><div id="box" tabindex="0"></div>';
    // a password: the page's password input, focused, and nothing else the agent's selector names
    expect(run('#pw')).toBe(document.getElementById('pw'));
    expect(document.activeElement?.id).toBe('pw');
    for (const field of ['#name', '#note', '#box', '#missing', 'body', 'input']) expect(run(field), field).toBeNull();
    expect(document.activeElement?.id).toBe('pw');
    // a one-time code: a text input or area too
    expect(run('#name', true)).toBe(document.getElementById('name'));
    expect(run('#note', true)).toBe(document.getElementById('note'));
    for (const field of ['#box', '#missing', 'body']) expect(run(field, true), field).toBeNull();
  });

  it('browserapp_43_a_selector_never_breaks_out_of_its_string', () => {
    document.body.innerHTML = '<input id="pw" type="password">';
    for (const field of ['"); globalThis.pwned = true; ("', "'); globalThis.pwned = true; ('", '#pw`${globalThis.pwned = true}`', '\\"; globalThis.pwned = true; //']) {
      const out = run(field);
      expect(out === null || out instanceof Error).toBe(true);
      expect((globalThis as { pwned?: boolean }).pwned).toBeUndefined();
    }
    expect(focusField('#pw')).not.toMatch(/insertText|value/);
  });

  it('browserapp_43_main_types_the_value_into_the_tabs_own_page', async () => {
    // the page loads again, then the keyboard, the field, the value; only its length is read back
    const a = fakePage();
    expect(await typeInto(a.page, K, 'persist:oby-agent-codex', a.relay, '#pw', PW)).toBeNull();
    expect(a.steps).toEqual(['reload', 'focus', 'focusField', 'filled', 'Input.insertText', 'length', 'length']);
    expect(a.host.fields).toEqual(['field-1']);
    expect(a.frames.find((f) => f.method === 'Input.insertText')?.params).toEqual({ text: PW });
    expect(a.frames.filter((f) => f.method.startsWith('Runtime.')).map((f) => f.params?.expression)).toEqual([focusField('#pw'), FIELD_LENGTH, FIELD_LENGTH]);
    expect(JSON.stringify(a.frames.filter((f) => f.method.startsWith('Runtime.')))).not.toContain(PW);
    expect(a.host).toMatchObject({ started: 1, ended: 0, filled: [PW] });

    // CDP's text didn't land (the length reads 0): typed once more as the guest's own input
    const b = fakePage({ lengths: [0, [...PW].length] });
    expect(await typeInto(b.page, K, 'persist:oby-agent-codex', b.relay, '#pw', PW)).toBeNull();
    expect(b.steps).toEqual(['reload', 'focus', 'focusField', 'filled', 'Input.insertText', 'length', 'insertText the value', 'length']);

    // a one-time code: its field is found as a code's, a text box allowed
    const code = fakePage();
    expect(await typeInto(code.page, K, 'persist:oby-agent-codex', code.relay, '#code', PW, true)).toBeNull();
    expect(code.frames.find((f) => f.method === 'Runtime.evaluate')?.params?.expression).toBe(focusField('#code', true));

    // a field the page doesn't have types nothing, and the guard Fill in set is lifted
    const c = fakePage();
    expect(await typeInto(c.page, K, 'persist:oby-agent-codex', c.relay, '#nope', PW)).toBe('That field isn’t on the page.');
    expect(c.steps).toEqual(['reload', 'focus', 'focusField']);
    expect(c.host).toMatchObject({ started: 1, ended: 1, filled: [] });

    // a page that never loads again types nothing
    vi.useFakeTimers();
    const d = fakePage({ loads: false });
    const slow = typeInto(d.page, K, 'persist:oby-agent-codex', d.relay, '#pw', PW);
    await vi.advanceTimersByTimeAsync(RELOAD_MS);
    expect(await slow).toBe('The page didn’t load again in time.');
    expect(d.steps).toEqual(['reload']);
    expect(d.host).toMatchObject({ started: 1, ended: 1, filled: [] });
    vi.useRealTimers();

    // another Oby's box tab: nothing is sent at all
    const e = fakePage();
    expect(await typeInto(e.page, K, 'oby-box-j1', e.relay, '#pw', PW)).toBe('This page isn’t open in the window.');
    expect(await typeInto(e.page, K, undefined, e.relay, '#pw', PW)).toBe('This page isn’t open in the window.');
    expect(e.steps).toEqual([]);
    expect(e.host).toMatchObject({ started: 0, ended: 0, filled: [] });
  });

  it('browserapp_43_what_the_page_does_with_the_value_while_it_is_typed_never_leaves', async () => {
    // a page whose input handler alerts what the field holds, as it's typed
    const posts: unknown[] = [];
    const relay = relayHost(async (events) => { posts.push(...events); return true; }, { home: '/nowhere' });
    const guest = { send: async () => ({}), reload() {}, stop() {}, focused: () => true, insertText: async () => {}, key() {}, downloadsTo() {}, passwordFields: async () => [], isPassword: async () => true, shows: async () => false };
    relay.attach(K, guest);
    const f = fakePage({ typing: (text) => {
      relay.event(K, 'Page.javascriptDialogOpening', { url: 'http://127.0.0.1/', message: text, type: 'alert', hasBrowserHandler: false });
      relay.event(K, 'Runtime.consoleAPICalled', { type: 'log', args: [{ type: 'string', value: `typed ${text}` }] });
    } });
    expect(await typeInto(f.page, K, 'persist:oby-agent-codex', relay, '#pw', PW)).toBeNull();
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
    expect(posts.length).toBeGreaterThan(0);
    expect(JSON.stringify(posts)).not.toContain(PW);
  });
});
