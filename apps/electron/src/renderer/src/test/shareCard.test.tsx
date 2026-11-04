import { beforeAll, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { InboxItem } from '../../../shared/obid-wire';
import type { Ask } from '../mock';

// The card that asks which repositories to share, as it renders (docs/contracts/peer-thread.md,
// PEER-38, PEER-42): a click mustn't share private work unread.
vi.mock('react-dom', async (original) => ({ ...await original<typeof import('react-dom')>(), createPortal: (node: unknown) => node }));
vi.mock('../ChatView', () => ({ clock: () => '9:06 AM', ThreadTask: () => null, default: () => null }));
vi.hoisted(() => {
  const g = globalThis as Record<string, unknown>;
  g.location = { search: '' };
  g.window = {};
  g.localStorage = { getItem: () => null, setItem: () => {}, removeItem: () => {} };
  g.document = { createElement: () => ({}), body: {} };
});

const OFFERED = ['me/private-taxes', 'acme/app'];
let home: (chosen: string[]) => string;
let settings: (chosen: string[]) => string;
beforeAll(async () => {
  const { AskBlock } = await import('../TaskCard');
  const { ShareModal } = await import('../SphereSettings');
  home = (chosen) => {
    const ask: Ask = { kind: 'confirm', headline: 'Which repositories?', text: '', options: ['Confirm', 'Cancel'], repos: { offered: OFFERED, chosen } };
    return renderToStaticMarkup(<AskBlock task={{ id: 'c1', job: { ask } }} />);
  };
  settings = (chosen) => {
    const card = { id: 'c1', title: 'Which repositories?', text: '', confirm: { action: 'share_status', target: '', payload: { offered: OFFERED, repos: chosen } } } as InboxItem;
    return renderToStaticMarkup(<ShareModal card={card} onClose={() => {}} onAnswered={() => {}} />);
  };
});

const boxes = (html: string) => [...html.matchAll(/<input type="checkbox"[^>]*>/g)].map((m) => m[0].includes('checked'));
const confirmButton = (html: string) => /<button[^>]*>Confirm<\/button>/.exec(html)?.[0] ?? '';

describe('the card, as it renders', () => {
  it('peer_38: every repository is listed whole, nothing ticked, and Confirm is off, on the stack and in Settings', () => {
    for (const html of [home([]), settings([])]) {
      expect(boxes(html)).toEqual([false, false]);
      expect(html).toContain('me/private-taxes');
      expect(html).toContain('acme/app');
      expect(confirmButton(html)).toContain('disabled');
      // no way to tick them all at once
      expect(html).not.toMatch(/>(Select )?all</i);
    }
  });
  it('peer_38: what obid has ticked is ticked, and only that, with Confirm on', () => {
    for (const html of [home(['acme/app']), settings(['acme/app'])]) {
      expect(boxes(html)).toEqual([false, true]);
      expect(confirmButton(html)).not.toContain('disabled');
    }
  });
  it('peer_42: a name shows as text', () => {
    const { offered } = { offered: ['a/<b>x</b>'] };
    const ask: Ask = { kind: 'confirm', headline: 'h', text: '', options: ['Confirm'], repos: { offered, chosen: [] } };
    return import('../TaskCard').then(({ AskBlock }) => {
      expect(renderToStaticMarkup(<AskBlock task={{ id: 'c1', job: { ask } }} />)).toContain('a/&lt;b&gt;x&lt;/b&gt;');
    });
  });
  it('a Confirm that asks nothing to tick is as it was', () => {
    return import('../TaskCard').then(({ AskBlock }) => {
      const ask: Ask = { kind: 'confirm', headline: 'h', text: '', options: ['Confirm', 'Cancel'] };
      const html = renderToStaticMarkup(<AskBlock task={{ id: 'c1', job: { ask } }} />);
      expect(boxes(html)).toEqual([]);
      expect(confirmButton(html)).not.toContain('disabled');
    });
  });
});
