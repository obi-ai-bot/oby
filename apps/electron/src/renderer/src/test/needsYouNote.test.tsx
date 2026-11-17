// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Chat } from '../chats';

// Another conversation needs you while the window is in front (docs/contracts/alerts.md, ALERTS-20):
// a quiet note in the window, never for the one open, only for what newly needs you.

const page = vi.hoisted(() => ({ chats: [] as Chat[] }));
vi.mock('../chats', async (original) => ({ ...await original<typeof import('../chats')>(), useChats: () => page.chats, chatOfTask: () => undefined }));
vi.mock('../tasks', async (original) => ({ ...await original<typeof import('../tasks')>(), useTasks: () => [], tasksWith: () => [] }));
vi.mock('../profiles', async (original) => ({ ...await original<typeof import('../profiles')>(), useProfiles: () => ({ list: undefined, kept: undefined }), profileChats: (c: Chat[]) => c }));

const chat = (id: string, title: string, needs = false) => ({ id, title, said: [], needs }) as Chat;
let root: Root, host: HTMLDivElement, focused = true;
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  focused = true;
  vi.spyOn(document, 'hasFocus').mockImplementation(() => focused);
  host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

async function shown(chats: Chat[], open: string | null, onOpen = vi.fn()) {
  page.chats = chats;
  const NeedsYouNote = (await import('../NeedsYouNote')).default;
  await act(async () => root.render(<NeedsYouNote open={open} onOpen={onOpen} />));
  return onOpen;
}
const note = () => host.querySelector('.needs-note');

describe('ALERTS-20: another conversation needs you, in the window', () => {
  it('alerts_20_a_conversation_that_newly_needs_you_shows_a_note_with_open', async () => {
    await shown([chat('a', 'Fix the login'), chat('b', 'Release notes')], 'a');
    expect(note()).toBeNull();
    const onOpen = await shown([chat('a', 'Fix the login'), chat('b', 'Release notes', true)], 'a');
    expect(note()?.textContent).toContain('Needs you');
    expect(note()?.textContent).toContain('Release notes');
    await act(async () => Array.from(note()!.querySelectorAll('button')).find((b) => b.textContent === 'Open')!.click());
    expect(onOpen).toHaveBeenCalledWith('b');
    expect(note()).toBeNull();
  });

  it('alerts_20_the_open_conversation_and_what_needed_you_before_say_nothing', async () => {
    // what needs you as the window opens is a baseline
    await shown([chat('a', 'A', true), chat('b', 'B')], 'b');
    expect(note()).toBeNull();
    // the open one newly needing you: you're looking at it
    await shown([chat('a', 'A', true), chat('b', 'B', true)], 'b');
    expect(note()).toBeNull();
  });

  it('alerts_20_nothing_while_the_window_isnt_in_front_the_macs_alert_says_it', async () => {
    await shown([chat('a', 'A'), chat('b', 'B')], 'a');
    focused = false;
    await shown([chat('a', 'A'), chat('b', 'B', true)], 'a');
    expect(note()).toBeNull();
  });

  it('alerts_20_the_note_goes_once_its_conversation_no_longer_needs_you', async () => {
    await shown([chat('a', 'A'), chat('b', 'B')], 'a');
    await shown([chat('a', 'A'), chat('b', 'B', true)], 'a');
    expect(note()).not.toBeNull();
    await shown([chat('a', 'A'), chat('b', 'B')], 'a');
    expect(note()).toBeNull();
  });

  it('alerts_20_the_pure_rules', async () => {
    const { needingOf, newlyNeeding } = await import('../NeedsYouNote');
    expect(needingOf([chat('a', 'A', true), chat('b', 'B')], ['b', 'gone'])).toEqual(['a', 'b']);
    expect(newlyNeeding(null, ['a'], null)).toEqual([]);
    expect(newlyNeeding(new Set(['a']), ['a', 'b', 'c'], 'c')).toEqual(['b']);
  });
});
