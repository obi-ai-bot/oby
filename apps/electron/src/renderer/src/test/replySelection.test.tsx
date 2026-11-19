// @vitest-environment happy-dom
import { act, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Reply about this (COMPOSER-23, 24) and a refused task's Install or Sign in under Oby's words
// (ACCOUNTS-37), in a page.

vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
const signIn = vi.hoisted(() => [] as string[]);
vi.mock('../SettingsView', () => ({ signInFromChat: (id: string) => signIn.push(id) }));

let root: Root | null = null;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = null;
  document.body.innerHTML = '';
  signIn.length = 0;
});

// A thread with Oby's reply, the owner's words and another reply of Oby's, the button over it
async function thread() {
  const { default: ReplySelection } = await import('../ReplySelection');
  const quotes: string[] = [];
  const list = createRef<HTMLDivElement>();
  const host = document.body.appendChild(document.createElement('div'));
  root = createRoot(host);
  await act(async () => root!.render(<>
    <div ref={list}>
      <div className="msg is-oby"><div className="msg-bubble"><p id="oby">Use SQLite for the cache, it’s already there.</p></div></div>
      <div className="msg is-you"><p className="msg-bubble" id="you">why not Postgres?</p></div>
      <div className="msg is-oby"><div className="msg-bubble"><p id="oby2">Postgres needs a server.</p></div></div>
    </div>
    <ReplySelection root={list} onQuote={(t) => quotes.push(t)} />
  </>));
  const select = async (from: string, start: number, to: string, end: number) => {
    const range = document.createRange();
    range.setStart(document.getElementById(from)!.firstChild!, start);
    range.setEnd(document.getElementById(to)!.firstChild!, end);
    const s = getSelection()!;
    s.removeAllRanges();
    s.addRange(range);
    await act(async () => { document.dispatchEvent(new Event('selectionchange')); });
  };
  const button = () => document.querySelector<HTMLButtonElement>('.reply-about');
  return { quotes, select, button };
}

describe('Reply about this', () => {
  it('composer_23_words_picked_in_a_reply_show_reply_about_this', async () => {
    const t = await thread();
    await t.select('oby', 4, 'oby', 10);
    expect(t.button()?.textContent).toBe('Reply about this');
    // the owner's own words, or two replies at once: nothing
    await t.select('you', 0, 'you', 3);
    expect(t.button()).toBeNull();
    await t.select('oby', 0, 'oby2', 3);
    expect(t.button()).toBeNull();
  });

  it('composer_24_a_click_quotes_the_words_into_the_box', async () => {
    const t = await thread();
    await t.select('oby', 4, 'oby', 10);
    await act(async () => t.button()!.click());
    expect(t.quotes).toEqual(['SQLite']);
    expect(t.button()).toBeNull();
    expect(getSelection()!.isCollapsed || !getSelection()!.rangeCount).toBe(true);
  });
});

describe('a refused task’s fixes', () => {
  it('accounts_37_install_or_sign_in_under_obys_words', async () => {
    const { default: RefusalFixes } = await import('../RefusalFixes');
    const host = document.body.appendChild(document.createElement('div'));
    root = createRoot(host);
    await act(async () => root!.render(<RefusalFixes fixes={[
      { action: 'install', agent: 'cursor', account: 'u1' },
      { action: 'signIn', agent: 'codex', account: 'u2' },
      { action: 'somethingNew', agent: 'codex', account: 'u3' },
    ]} />));
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent);
    expect(buttons).toEqual(['Install', 'Sign in']);
    // Install shows the vendor's command to copy, as first run does (FIRSTRUN-2)
    await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="Install Cursor"]')!.click());
    expect(host.querySelector('.setup-install-command')?.textContent).toBe('curl https://cursor.com/install -fsS | bash');
    await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="Sign in to Codex"]')!.click());
    expect(signIn).toEqual(['u2']);
  });

  it('accounts_37_nothing_it_doesnt_know', async () => {
    const { default: RefusalFixes } = await import('../RefusalFixes');
    const host = document.body.appendChild(document.createElement('div'));
    root = createRoot(host);
    await act(async () => root!.render(<RefusalFixes fixes={[{ action: 'install', agent: 'gemini', account: 'g' }]} />));
    expect(host.innerHTML).toBe('');
  });
});
