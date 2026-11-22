// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('electron', () => ({ app: { getPath: () => '/nowhere' }, session: { fromPartition: () => ({}) }, webContents: { fromId: () => null } }));
const { FIELD_LENGTH, RELOAD_MS, focusField, typeInto } = await import('../../../main/browserGuest');
type FillPage = Parameters<typeof typeInto>[0];

// Fill in in the tab's own page (main/browserGuest.ts typeInto, BROWSERAPP-43): the page loads
// again, the guest gets the keyboard, the ask's field is focused by the one script that names it,
// and the value is typed in with Input.insertText (or the guest's own insertText), never part of any
// script; only how many characters the field holds is read back.

const K = { threadId: 'j1', tabId: 't1' };
const PW = 'fill-test-value-9031';

function fakePage(o: { loads?: boolean; lengths?: number[] } = {}) {
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
        if (method !== 'Runtime.evaluate') return {};
        return { result: { value: expression === FIELD_LENGTH ? lengths.shift() ?? 0 : expression === focusField('#pw') } };
      },
    },
    reload: () => { steps.push('reload'); if (o.loads !== false) setTimeout(() => loaded?.(), 0); },
    focus: () => { steps.push('focus'); },
    insertText: async (text: string) => { steps.push(`insertText ${text === PW ? 'the value' : text}`); },
    once: (_e, f) => { loaded = f; },
    off: () => { loaded = null; },
  };
  const host = { started: 0, ended: 0, filled: [] as string[] };
  const relay = { fillStarts: () => { host.started++; }, fillEnds: () => { host.ended++; }, filled: (_k: unknown, v: string) => { host.filled.push(v); } };
  return { page, steps, frames, host, relay };
}

describe('BROWSERAPP-43: Fill in focuses only the ask\'s own field, and the value is in no script', () => {
  afterEach(() => { document.body.innerHTML = ''; delete (globalThis as { pwned?: boolean }).pwned; vi.useRealTimers(); });
  const run = (field: string): unknown => { try { return (0, eval)(focusField(field)); } catch (e) { return e; } };

  it('browserapp_43_focus_field_puts_the_keyboard_on_an_input_or_text_area_only', () => {
    document.body.innerHTML = '<input id="name"><input id="pw" type="password"><textarea id="note"></textarea><div id="box" tabindex="0"></div>';
    expect(run('#pw')).toBe(true);
    expect(document.activeElement?.id).toBe('pw');
    expect(run('#note')).toBe(true);
    for (const field of ['#box', '#missing', 'body']) expect(run(field)).toBe(false);
  });

  it('browserapp_43_a_selector_never_breaks_out_of_its_string', () => {
    document.body.innerHTML = '<input id="pw" type="password">';
    for (const field of ['"); globalThis.pwned = true; ("', "'); globalThis.pwned = true; ('", '#pw`${globalThis.pwned = true}`', '\\"; globalThis.pwned = true; //']) {
      const out = run(field);
      expect(out === false || out instanceof Error).toBe(true);
      expect((globalThis as { pwned?: boolean }).pwned).toBeUndefined();
    }
    expect(focusField('#pw')).not.toMatch(/insertText|value/);
  });

  it('browserapp_43_main_types_the_value_into_the_tabs_own_page', async () => {
    // the page loads again, then the keyboard, the field, the value; only its length is read back
    const a = fakePage();
    expect(await typeInto(a.page, K, 'persist:oby-agent-codex', a.relay, '#pw', PW)).toBeNull();
    expect(a.steps).toEqual(['reload', 'focus', 'focusField', 'Input.insertText', 'length', 'length']);
    expect(a.frames.find((f) => f.method === 'Input.insertText')?.params).toEqual({ text: PW });
    expect(a.frames.filter((f) => f.method.startsWith('Runtime.')).map((f) => f.params?.expression)).toEqual([focusField('#pw'), FIELD_LENGTH, FIELD_LENGTH]);
    expect(JSON.stringify(a.frames.filter((f) => f.method.startsWith('Runtime.')))).not.toContain(PW);
    expect(a.host).toEqual({ started: 1, ended: 0, filled: [PW] });

    // CDP's text didn't land (the length reads 0): typed once more as the guest's own input
    const b = fakePage({ lengths: [0, [...PW].length] });
    expect(await typeInto(b.page, K, 'persist:oby-agent-codex', b.relay, '#pw', PW)).toBeNull();
    expect(b.steps).toEqual(['reload', 'focus', 'focusField', 'Input.insertText', 'length', 'insertText the value', 'length']);

    // a field the page doesn't have types nothing, and the guard Fill in set is lifted
    const c = fakePage();
    expect(await typeInto(c.page, K, 'persist:oby-agent-codex', c.relay, '#nope', PW)).toBe('That field isn’t on the page.');
    expect(c.steps).toEqual(['reload', 'focus', 'focusField']);
    expect(c.host).toEqual({ started: 1, ended: 1, filled: [] });

    // a page that never loads again types nothing
    vi.useFakeTimers();
    const d = fakePage({ loads: false });
    const slow = typeInto(d.page, K, 'persist:oby-agent-codex', d.relay, '#pw', PW);
    await vi.advanceTimersByTimeAsync(RELOAD_MS);
    expect(await slow).toBe('The page didn’t load again in time.');
    expect(d.steps).toEqual(['reload']);
    expect(d.host).toEqual({ started: 1, ended: 1, filled: [] });
    vi.useRealTimers();

    // another Oby's box tab: nothing is sent at all
    const e = fakePage();
    expect(await typeInto(e.page, K, 'oby-box-j1', e.relay, '#pw', PW)).toBe('This page isn’t open in the window.');
    expect(await typeInto(e.page, K, undefined, e.relay, '#pw', PW)).toBe('This page isn’t open in the window.');
    expect([e.steps, e.host]).toEqual([[], { started: 0, ended: 0, filled: [] }]);
  });
});
