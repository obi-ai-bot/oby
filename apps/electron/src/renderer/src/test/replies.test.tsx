// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Place } from '../Reply';

// Reading a reply (docs/contracts/chat-first.md, CHATFIRST-37 to 41), in the page as the owner uses
// it: the words as Said renders them, the controls clicked. main's side of a path (which are files,
// opening one) is stood in for by window.app; main itself is replyFiles.test.ts.
const the = vi.hoisted(() => ({ files: new Set<string>(), asked: [] as string[][], opened: [] as string[][], copied: [] as string[] }));
let Said: typeof import('../Said').default;
let Reply: typeof import('../Reply');
let host: HTMLDivElement, root: Root;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  window.app = {
    replyFiles: async (place: string, id: string, refs: string[]) => { the.asked.push([place, id, ...refs]); return refs.map((r) => the.files.has(`${place}:${id}:${r}`)); },
    openReplyFile: async (place: string, id: string, ref: string) => { the.opened.push([place, id, ref]); return true; },
    editorName: async () => 'Cursor',
  } as never;
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (t: string) => { the.copied.push(t); } } });
  Said = (await import('../Said')).default;
  Reply = await import('../Reply');
});
beforeEach(() => {
  the.files = new Set(); the.asked = []; the.opened = []; the.copied = [];
  Reply.forgetFiles();
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

const render = async (el: React.ReactNode) => { await act(async () => root.render(el)); await act(async () => { await new Promise((r) => setTimeout(r, 5)); }); };
const at = (place: Place | null, text: string, streaming = false) => <Reply.ReplyPlace.Provider value={place}><Said text={text} streaming={streaming} /></Reply.ReplyPlace.Provider>;
const click = (el: Element) => act(async () => { (el as HTMLElement).click(); await Promise.resolve(); });

describe('CHATFIRST-37: copy a reply', () => {
  it('chatfirst_37_a_replys_copy_puts_its_words_as_written_on_the_clipboard', async () => {
    const words = 'Done. See **the fix** in `src/x.ts`:\n\n- one\n- two';
    await render(<Reply.CopyButton text={words} className="msg-copy" />);
    const button = host.querySelector('button.reply-copy')!;
    expect(button.getAttribute('aria-label')).toBe('Copy');
    await click(button);
    expect(the.copied).toEqual([words]);
    expect(button.getAttribute('aria-label')).toBe('Copied');
  });
});

describe('CHATFIRST-38: copy a code block, in colour', () => {
  it('chatfirst_38_a_code_block_has_its_copy_and_copies_only_its_code', async () => {
    await render(at(null, 'Run this:\n\n```bash\nnpm test && npm run typecheck\n```\n\nThen push.'));
    const block = host.querySelector('.code-block')!;
    expect(block.querySelector('pre.ss-code')!.textContent).toBe('npm test && npm run typecheck');
    const copy = block.querySelector('button.code-copy')!;
    expect(copy.getAttribute('aria-label')).toBe('Copy code');
    await click(copy);
    expect(the.copied).toEqual(['npm test && npm run typecheck']);
  });

  it('chatfirst_38_its_code_is_highlighted_by_its_language_into_elements_never_html', async () => {
    await render(at(null, '```ts\nconst answer: number = 42; // the one\n```'));
    const pre = host.querySelector('pre.ss-code')!;
    expect(pre.querySelector('.hl-keyword')!.textContent).toBe('const');
    expect(pre.querySelector('.hl-number')!.textContent).toBe('42');
    expect(pre.querySelector('.hl-comment')!.textContent).toBe('// the one');
    expect(pre.textContent).toBe('const answer: number = 42; // the one');
    // HTML in the code is its text: nothing of it becomes an element
    await render(at(null, '```html\n<img src=x onerror="alert(1)"><script>alert(2)</script>\n```'));
    expect(host.querySelector('img, script')).toBeNull();
    expect(host.querySelector('pre.ss-code')!.textContent).toBe('<img src=x onerror="alert(1)"><script>alert(2)</script>');
    expect(host.querySelector('pre.ss-code .hl-tag')).not.toBeNull();
  });

  it('chatfirst_38_an_unknown_language_or_none_is_the_text_it_is', async () => {
    await render(at(null, '```klingon\nqapla x = 1\n```\n\n```\nplain words\n```'));
    const pres = host.querySelectorAll('pre.ss-code');
    expect([...pres].map((p) => [p.textContent, p.querySelector('.hl')])).toEqual([['qapla x = 1', null], ['plain words', null]]);
  });
});

describe('CHATFIRST-39: a path in a reply opens the file', () => {
  it('chatfirst_39_a_path_of_the_tasks_worktree_is_a_link_that_opens_it_at_its_line', async () => {
    the.files = new Set(['task:j1:src/x.ts:42', 'task:j1:src/y.ts']);
    await render(at({ place: 'task', id: 'j1' }, 'Fixed src/x.ts:42, see `src/y.ts` and docs/nope.md.'));
    const links = [...host.querySelectorAll('a.tok-file')];
    expect(links.map((a) => a.textContent)).toEqual(['src/x.ts:42', 'src/y.ts']);
    expect(links[0].getAttribute('title')).toBe('Open in Cursor');
    // asked once, all at once, for where the reply is
    expect(the.asked).toEqual([['task', 'j1', 'src/x.ts:42', 'src/y.ts', 'docs/nope.md']]);
    await click(links[0]);
    expect(the.opened).toEqual([['task', 'j1', 'src/x.ts:42']]);
    // one that isn't a file there stays the coloured text it was
    const nope = [...host.querySelectorAll('.tok-path')].find((e) => e.textContent === 'docs/nope.md')!;
    expect(nope.tagName).toBe('SPAN');
  });

  it('chatfirst_39_a_conversations_paths_are_asked_for_in_that_conversation', async () => {
    the.files = new Set(['conversation:c1:docs/plan.md']);
    await render(at({ place: 'conversation', id: 'c1' }, 'The plan is docs/plan.md.'));
    expect(the.asked).toEqual([['conversation', 'c1', 'docs/plan.md']]);
    await click(host.querySelector('a.tok-file')!);
    expect(the.opened).toEqual([['conversation', 'c1', 'docs/plan.md']]);
  });
});

describe('CHATFIRST-40: nothing else in a reply opens', () => {
  it('chatfirst_40_words_with_no_place_or_still_coming_in_ask_nothing_and_link_nothing', async () => {
    the.files = new Set(['task:j1:src/x.ts']);
    // another Oby's words (no place), and words still coming in: no path is asked for
    await render(at(null, 'see src/x.ts'));
    await render(at({ place: 'task', id: 'j1' }, 'see src/x.ts', true));
    expect(the.asked).toEqual([]);
    expect(host.querySelector('a.tok-file')).toBeNull();
    // in, it is
    await render(at({ place: 'task', id: 'j1' }, 'see src/x.ts'));
    expect(host.querySelector('a.tok-file')!.textContent).toBe('src/x.ts');
  });

  it('chatfirst_40_a_link_or_scheme_in_a_reply_is_never_a_file_link', async () => {
    the.files = new Set(['task:j1:src/x.ts']);
    await render(at({ place: 'task', id: 'j1' }, '[src/x.ts](file:///etc/hosts) [open](vscode://file/etc/hosts) [x](src/x.ts) http://localhost/src/x.ts'));
    expect(host.querySelector('a.tok-file')).toBeNull();
    expect(host.querySelector('a[href^="file:"], a[href^="vscode:"], a[href="src/x.ts"]')).toBeNull();
    expect(the.opened).toEqual([]);
  });
});

describe('CHATFIRST-41: a picture shows full size', () => {
  it('chatfirst_41_a_click_shows_it_full_size_and_esc_closes_only_it', async () => {
    const back = vi.fn();
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') back(); };
    window.addEventListener('keydown', onKey);
    try {
      await render(<Reply.Photo className="msg-photo" src="data:image/png;base64,AA==" alt="screenshot.png" />);
      expect(document.querySelector('.image-view')).toBeNull();
      await click(host.querySelector('button.photo-open')!);
      const view = document.querySelector('.image-view')!;
      expect(view.getAttribute('role')).toBe('dialog');
      expect(view.querySelector('img')!.getAttribute('src')).toBe('data:image/png;base64,AA==');
      expect(document.activeElement?.getAttribute('aria-label')).toBe('Close');
      await act(async () => { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); });
      expect(document.querySelector('.image-view')).toBeNull();
      // the conversation under it didn't take the Esc as Back
      expect(back).not.toHaveBeenCalled();
      // and a click outside the picture closes it too
      await click(host.querySelector('button.photo-open')!);
      await click(document.querySelector('.image-view')!);
      expect(document.querySelector('.image-view')).toBeNull();
    } finally {
      window.removeEventListener('keydown', onKey);
    }
  });
});

// T3 Code's MessageCopyButton test "explains a denied clipboard write and lets the same message be copied
// again", held to CHATFIRST-37: a clipboard that refuses changes nothing
describe('T3: copy a reply', () => {
  it('t3_copy_a_refused_clipboard_claims_nothing_and_the_next_press_copies', async () => {
    const kept = navigator.clipboard;
    let refuse = true;
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (t: string) => { if (refuse) throw new DOMException('Write permission denied.', 'NotAllowedError'); the.copied.push(t); } } });
    try {
      await render(<Reply.CopyButton text="the words" className="msg-copy" />);
      const button = host.querySelector('button.reply-copy')!;
      await click(button);
      expect(button.getAttribute('aria-label')).toBe('Copy');
      expect(button.classList.contains('is-copied')).toBe(false);
      refuse = false;
      await click(button);
      expect(the.copied).toEqual(['the words']);
      expect(button.getAttribute('aria-label')).toBe('Copied');
    } finally {
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: kept });
    }
  });
});
