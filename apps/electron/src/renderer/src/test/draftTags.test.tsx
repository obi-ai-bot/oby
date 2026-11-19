// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The message box puts a draft's tags back with its words, and keeps them as they change
// (docs/contracts/drafts.md, DRAFTS-4 to 8): the real Composer on its box's draft.

let root: Root, host: HTMLDivElement;
beforeEach(() => {
  vi.resetModules();
  vi.useRealTimers();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  localStorage.clear();
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); document.body.innerHTML = ''; vi.unstubAllGlobals(); });

const tags = () => Array.from(host.querySelectorAll('.composer-tag-text')).map((t) => t.textContent);

describe('DRAFTS-4 to 8 in the message box', () => {
  it('drafts_4_a_boxs_tags_and_file_come_back_with_its_words', async () => {
    const { KEY, box, dataOf } = await import('../drafts');
    const data = await dataOf(new File(['hello'], 'notes.txt', { type: 'text/plain' }));
    localStorage.setItem(KEY, JSON.stringify({ [box.conversation('c1')]: { text: 'Look', at: Date.now(), tags: [{ kind: 'place', name: 'Studio Mac' }, { kind: 'file', name: 'notes.txt', type: 'text/plain', data }] } }));
    const Composer = (await import('../Composer')).default;
    await act(async () => root.render(<Composer label="Message" conversation="c1" draft={box.conversation('c1')} onSend={() => {}} />));
    expect(tags()).toEqual(['On Studio Mac', 'notes.txt']);
    // another box shows its own: none
    await act(async () => root.render(<Composer label="Message" conversation="c2" draft={box.conversation('c2')} onSend={() => {}} />));
    expect(tags()).toEqual([]);
    // and back again
    await act(async () => root.render(<Composer label="Message" conversation="c1" draft={box.conversation('c1')} onSend={() => {}} />));
    expect(tags()).toEqual(['On Studio Mac', 'notes.txt']);
  });

  it('drafts_8_a_tag_taken_out_is_taken_out_of_the_draft', async () => {
    const { KEY, box, extras } = await import('../drafts');
    localStorage.setItem(KEY, JSON.stringify({ [box.conversation('c1')]: { text: '', at: Date.now(), tags: [{ kind: 'place', name: 'Studio Mac' }] } }));
    const Composer = (await import('../Composer')).default;
    await act(async () => root.render(<Composer label="Message" conversation="c1" draft={box.conversation('c1')} onSend={() => {}} />));
    const x = host.querySelector<HTMLButtonElement>('.composer-tag button, button[aria-label^="Remove"]');
    expect(x).toBeTruthy();
    await act(async () => { x!.click(); await new Promise((r) => setTimeout(r, 400)); });
    expect(extras(box.conversation('c1')).tags ?? []).toEqual([]);
  });
});
