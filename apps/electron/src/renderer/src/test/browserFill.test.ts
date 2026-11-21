// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('electron', () => ({ session: { fromPartition: () => ({}) }, webContents: { fromId: () => null } }));
const { focusField } = await import('../../../main/browserGuest');

// Fill in's one script in the page (main/browserGuest.ts, BROWSERAPP-12): it puts the keyboard on the
// ask's field, and the value is typed after it with Input.insertText, never part of any script.

describe('BROWSERAPP-12: Fill in focuses only the ask\'s own field, and the value is in no script', () => {
  afterEach(() => { document.body.innerHTML = ''; delete (globalThis as { pwned?: boolean }).pwned; });
  const run = (field: string): unknown => { try { return (0, eval)(focusField(field)); } catch (e) { return e; } };

  it('browserapp_12_focus_field_puts_the_keyboard_on_an_input_or_text_area_only', () => {
    document.body.innerHTML = '<input id="name"><input id="pw" type="password"><textarea id="note"></textarea><div id="box" tabindex="0"></div>';
    expect(run('#pw')).toBe(true);
    expect(document.activeElement?.id).toBe('pw');
    expect(run('#note')).toBe(true);
    for (const field of ['#box', '#missing', 'body']) expect(run(field)).toBe(false);
  });

  it('browserapp_12_a_selector_never_breaks_out_of_its_string', () => {
    document.body.innerHTML = '<input id="pw" type="password">';
    for (const field of ['"); globalThis.pwned = true; ("', "'); globalThis.pwned = true; ('", '#pw`${globalThis.pwned = true}`', '\\"; globalThis.pwned = true; //']) {
      const out = run(field);
      expect(out === false || out instanceof Error).toBe(true);
      expect((globalThis as { pwned?: boolean }).pwned).toBeUndefined();
    }
    expect(focusField('#pw')).not.toMatch(/insertText|value/);
  });
});
