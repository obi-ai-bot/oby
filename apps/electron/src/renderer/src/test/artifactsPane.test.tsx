// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Artifact } from '../obid';
import { ArtifactCard } from '../artifacts/Card';
import { ArtifactPane } from '../artifacts/Pane';
import { closeArtifact } from '../artifacts/store';

// An artifact in its conversation and beside it (docs/contracts/artifacts-app.md, ARTIFACTSAPP-4,
// Sangam's board 1b): its card says what it is, a click opens it beside the conversation, and the
// pane hands main only the artifact, the version and where its preview goes.
const ID = '3f0c2a1e-1111-4222-8333-944455556666';
const page: Artifact = { id: ID, title: 'page', kind: 'html', conversation: 'c1', job: 'j1', created: '2026-11-21T09:00:00Z', versions: [
  { version: 1, created: '2026-11-21T09:00:00Z', files: ['index.html', 'style.css'], bytes: 72, job: 'j1' },
  { version: 2, created: '2026-11-21T09:05:00Z', files: ['index.html', 'style.css'], bytes: 80, job: 'j1' },
] };

let root: Root | undefined;
const preview = vi.fn(async () => ({ shown: true as const }));
beforeEach(() => { vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true); (window as unknown as { app: unknown }).app = { artifactPreview: preview }; });
afterEach(async () => { await act(async () => { closeArtifact(); root?.unmount(); }); root = undefined; document.body.innerHTML = ''; preview.mockClear(); });

describe('ARTIFACTSAPP-4', () => {
  it('artifactsapp_4_the_card_says_what_it_is_and_a_click_opens_it_beside_the_conversation_on_its_latest_version', async () => {
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<><ArtifactCard artifact={page} agent="claude" /><ArtifactPane width={480} /></>));
    const card = host.querySelector('button.art-card')!;
    expect(card.querySelector('.art-card-title')!.textContent).toBe('page');
    expect(card.querySelector('.art-card-meta')!.textContent).toBe('Oby artifact · Page · v2 · made by Claude Code');
    expect(host.querySelector('[data-artifact-view]')).toBeNull();
    await act(async () => { (card as HTMLButtonElement).click(); });
    expect(card.getAttribute('aria-pressed')).toBe('true');
    expect(host.querySelector('.art-strip-title b')!.textContent).toBe('page');
    expect(host.querySelector('.art-strip-title span')!.textContent).toBe('Oby artifact · Page · v2');
    expect(host.querySelector(`[data-artifact-view="${ID}"]`)).not.toBeNull();
    // main gets the artifact, its latest version and the preview's box: nothing else of the page
    expect(preview).toHaveBeenCalledWith({ id: ID, version: 2, bounds: { x: 0, y: 0, width: 0, height: 0 } });
    // closing it takes the preview away
    await act(async () => { (host.querySelector('[aria-label="Close the artifact"]') as HTMLButtonElement).click(); });
    expect(host.querySelector('[data-artifact-view]')).toBeNull();
    expect(preview).toHaveBeenLastCalledWith(null);
  });

  it('artifactsapp_4_an_error_from_main_shows_in_the_pane', async () => {
    preview.mockImplementationOnce(async () => ({ error: 'That artifact isn’t there any more.' }) as never);
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<><ArtifactCard artifact={page} /><ArtifactPane width={480} /></>));
    expect(host.querySelector('.art-card-meta')!.textContent).toBe('Oby artifact · Page · v2');
    await act(async () => { (host.querySelector('button.art-card') as HTMLButtonElement).click(); });
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('That artifact isn’t there any more.');
  });
});

describe('ARTIFACTSAPP-15', () => {
  const show = async (artifact: Artifact) => {
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<><ArtifactCard artifact={artifact} agent="claude" /><ArtifactPane width={480} /></>));
    return host;
  };

  it('artifactsapp_15_the_card_draws_the_frame_its_title_and_what_it_is_with_a_chevron_at_14', async () => {
    const card = (await show(page)).querySelector('button.art-card')!;
    const frame = card.querySelector('svg.lucide-frame')!;
    expect([frame.getAttribute('width'), frame.getAttribute('height')]).toEqual(['16', '16']);
    expect(card.querySelector('.lucide-shapes')).toBeNull();
    expect(card.querySelector('.art-card-title')!.textContent).toBe('page');
    expect(card.querySelector('.art-card-meta')!.textContent).toBe('Oby artifact · Page · v2 · made by Claude Code');
    const chevron = card.querySelector('svg.lucide-chevron-right')!;
    expect([chevron.getAttribute('width'), chevron.getAttribute('height')]).toEqual(['14', '14']);
  });

  it('artifactsapp_15_each_kind_says_the_boards_word', async () => {
    for (const [kind, word] of [['html', 'Page'], ['markdown', 'Document'], ['image', 'Image'], ['pdf', 'PDF'], ['canvas', 'Canvas'], ['design', 'Canvas']]) {
      const host = await show({ ...page, kind });
      expect(host.querySelector('.art-card-meta')!.textContent).toBe(`Oby artifact · ${word} · v2 · made by Claude Code`);
      await act(async () => { (host.querySelector('button.art-card') as HTMLButtonElement).click(); });
      expect(host.querySelector('.art-strip-title span')!.textContent).toBe(`Oby artifact · ${word} · v2`);
      await act(async () => { closeArtifact(); root!.unmount(); });
      host.remove();
    }
  });
});

describe('ARTIFACTSAPP-18', () => {
  const v1: Artifact = { ...page, versions: page.versions.slice(0, 1) };
  const withThumb = (artifactThumb?: (id: string, version: number) => Promise<unknown>) => {
    (window as unknown as { app: unknown }).app = { artifactPreview: preview, ...(artifactThumb ? { artifactThumb } : {}) };
  };
  const show = async (artifact: Artifact) => {
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<ArtifactCard artifact={artifact} />));
    return host;
  };
  const blocks = (host: HTMLElement) => {
    const t = host.querySelector('.art-thumb')!;
    return { a: t.querySelectorAll('.art-thumb-a').length, b: t.querySelectorAll('.art-thumb-b span').length, img: t.querySelectorAll('img').length };
  };

  it('artifactsapp_18_the_card_shows_the_versions_picture_once_main_sends_it', async () => {
    withThumb(async () => ({ src: 'data:image/png;base64,AAAA' }));
    const thumb = (await show(page)).querySelector('.art-thumb')!;
    const imgs = thumb.querySelectorAll('img.art-thumb-img');
    expect(imgs).toHaveLength(1);
    expect(imgs[0].getAttribute('src')).toBe('data:image/png;base64,AAAA');
    expect(imgs[0].getAttribute('alt')).toBe('');
    expect(thumb.children).toHaveLength(1);
    expect(thumb.querySelector('.art-thumb-a, .art-thumb-b')).toBeNull();
  });

  it('artifactsapp_18_the_card_keeps_the_boards_blocks_until_then_and_on_none', async () => {
    let answer!: (a: unknown) => void;
    withThumb(() => new Promise((done) => { answer = done; }));
    const host = await show(page);
    expect(blocks(host), 'pending').toEqual({ a: 1, b: 2, img: 0 });
    await act(async () => answer({ none: true }));
    expect(blocks(host), 'none').toEqual({ a: 1, b: 2, img: 0 });
    await act(async () => root!.unmount());
    host.remove();
    for (const thumb of [undefined, async () => null, async () => { throw new Error('gone'); }]) {
      withThumb(thumb);
      const h = await show(page);
      expect(blocks(h)).toEqual({ a: 1, b: 2, img: 0 });
      await act(async () => root!.unmount());
      h.remove();
    }
  });

  it('artifactsapp_18_a_new_version_asks_for_its_own_picture', async () => {
    const answers: ((a: unknown) => void)[] = [];
    const thumb = vi.fn((_id: string, _version: number) => new Promise((done) => { answers.push(done); }));
    withThumb(thumb);
    const host = await show(v1);
    expect(thumb).toHaveBeenCalledTimes(1);
    expect(thumb).toHaveBeenLastCalledWith(ID, 1);
    await act(async () => root!.render(<ArtifactCard artifact={page} />));
    expect(thumb).toHaveBeenCalledTimes(2);
    expect(thumb).toHaveBeenLastCalledWith(ID, 2);
    // version 1's picture, come late, isn't version 2's
    await act(async () => answers[0]({ src: 'data:image/png;base64,V1' }));
    expect(blocks(host)).toEqual({ a: 1, b: 2, img: 0 });
    await act(async () => answers[1]({ src: 'data:image/png;base64,V2' }));
    expect(host.querySelector('img.art-thumb-img')!.getAttribute('src')).toBe('data:image/png;base64,V2');
  });
});

describe('ARTIFACTSAPP-20', () => {
  it('artifactsapp_20_the_pane_has_board_1bs_reload_on_preview', async () => {
    const reload = vi.fn(async () => null);
    (window as unknown as { app: unknown }).app = { artifactPreview: preview, artifactReload: reload };
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<><ArtifactCard artifact={page} /><ArtifactPane width={480} /></>));
    await act(async () => { (host.querySelector('button.art-card') as HTMLButtonElement).click(); });
    const button = host.querySelector<HTMLButtonElement>('button[aria-label="Reload"]')!;
    expect(button.className).toBe('icon-button is-28');
    expect(button.textContent).toBe('');
    const icon = button.querySelector('svg.lucide-rotate-cw')!;
    expect([icon.getAttribute('width'), icon.getAttribute('stroke-width')]).toEqual(['15', '1.6']);
    const strip = [...host.querySelector('[data-artifact-strip]')!.children];
    expect(strip.indexOf(button)).toBe(strip.indexOf(host.querySelector('.art-tabs')!) - 1);
    await act(async () => button.click());
    expect(reload).toHaveBeenCalledTimes(1);
    // on Code, none
    await act(async () => { (host.querySelectorAll<HTMLButtonElement>('.art-tab')[1]).click(); });
    expect(host.querySelector('button[aria-label="Reload"]')).toBeNull();
    await act(async () => { (host.querySelectorAll<HTMLButtonElement>('.art-tab')[0]).click(); });
    expect(host.querySelector('button[aria-label="Reload"]')).not.toBeNull();
    // with no Reload on the bridge, none
    await act(async () => { closeArtifact(); root!.unmount(); });
    (window as unknown as { app: unknown }).app = { artifactPreview: preview };
    root = createRoot(host);
    await act(async () => root!.render(<><ArtifactCard artifact={page} /><ArtifactPane width={480} /></>));
    await act(async () => { (host.querySelector('button.art-card') as HTMLButtonElement).click(); });
    expect(host.querySelector('.art-tabs')).not.toBeNull();
    expect(host.querySelector('button[aria-label="Reload"]')).toBeNull();
  });
});

describe('ARTIFACTSAPP-24', () => {
  // the real live.ts on main's bridge (liveBridge) and a stand-in obid that takes artifact-turn-v1: the
  // pane's preview answers the page's error
  const C = 'c1';
  const status = { runner: { version: '0.2.4', host: 'mac' }, accounts: [{ id: 'p', agent: 'claude', name: 'Personal', signedIn: true }], models: {}, maxRunning: 6, running: 0, capabilities: ['artifact-turn-v1'] };
  const convo = { id: C, title: 'Billing', pinned: false, created: page.created, lastActivity: page.created, cursor: '', folders: [] };
  async function app(pageError?: string) {
    vi.resetModules();
    const calls: { method: string; path: string; body?: unknown }[] = [];
    const call = vi.fn(async (method: string, path: string, body?: unknown) => {
      calls.push({ method, path, body });
      if (method === 'POST' && path.startsWith('/v1/attachments')) return { status: 200, text: JSON.stringify({ path: 'uploads/9/preview-error.txt', name: 'preview-error.txt' }) };
      if (method === 'POST' && path === '/v1/plans') {
        const said = (body as { said: string }).said;
        return { status: 200, text: JSON.stringify(said === 'Which one?' ? { kind: 'question', question: { text: 'Which one?', options: ['This', 'That'] }, said, jobs: [], conversation: C } : { kind: 'reply', text: 'On it.', jobs: [], conversation: C }) };
      }
      if (method === 'GET' && path.startsWith('/v1/conversation')) return { status: 200, text: '[]' };
      return { status: 404, text: '{"error":"Not found."}' };
    });
    const { liveBridge } = await import('./liveBridge');
    const bridge = liveBridge(call);
    const shows = vi.fn(async () => ({ shown: true as const, ...(pageError === undefined ? {} : { pageError }) }));
    (window as { app?: unknown }).app = { obid: { call, ...bridge.obid }, artifactPreview: shows, onFullScreen: () => () => {}, onSphere: () => () => {} };
    vi.stubGlobal('location', { search: '' });
    const live = await import('../live');
    const store = await import('../artifacts/store');
    const { ArtifactPane: Pane } = await import('../artifacts/Pane');
    live.connect();
    bridge.sync({ status, conversations: [convo] });
    const host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<Pane width={480} />));
    await act(async () => store.openArtifact(page));
    return { calls, live, store, host };
  }
  const fix = (host: HTMLElement) => [...host.querySelectorAll('button')].find((b) => b.textContent === 'Ask to fix') as HTMLButtonElement | undefined;
  const sent = (calls: { method: string; path: string; body?: unknown }[]) => calls.filter((c) => c.method === 'POST' && (c.path === '/v1/plans' || c.path.startsWith('/v1/attachments')));

  it('artifactsapp_24_the_pane_shows_the_pages_error_as_text_with_ask_to_fix', async () => {
    const error = '<img src=x onerror=alert(1)>\nThe page asked for cdn.example.com: previews have no network.';
    const { calls, live, store, host } = await app(error);
    const strip = host.querySelector('.art-page-error')!;
    expect(strip).not.toBeNull();
    // after the refused line, before the view
    expect(strip.nextElementSibling).toBe(host.querySelector('[data-artifact-view]'));
    expect(strip.querySelector('p')!.textContent).toBe('This page stopped with an error.');
    expect(strip.querySelector('pre')!.textContent).toBe(error);
    expect(strip.querySelector('img')).toBeNull();
    // nothing goes without the click
    await act(async () => { await new Promise((done) => setTimeout(done, 50)); });
    expect(sent(calls)).toEqual([]);
    await act(async () => fix(host)!.click());
    await vi.waitFor(() => expect(sent(calls)).toHaveLength(2));
    const [upload, plan] = sent(calls);
    expect(upload.path).toBe('/v1/attachments?name=preview-error.txt');
    expect(new TextDecoder().decode(upload.body as Uint8Array)).toBe(error);
    expect(plan.body).toMatchObject({ said: 'The preview shows an error, in the attached file. Fix the artifact so it draws.', conversation: C, attachments: ['uploads/9/preview-error.txt'], artifact: { id: ID, version: 2 } });
    // while Oby's question waits in the conversation, the button waits too
    live.ask('Which one?', C);
    await vi.waitFor(() => expect(fix(host)!.disabled).toBe(true));
    await act(async () => { store.closeArtifact(); root!.unmount(); });
    host.remove();

    // with no pageError, no strip
    const none = await app();
    expect(none.host.querySelector('[data-artifact-view]')).not.toBeNull();
    expect(none.host.querySelector('.art-page-error')).toBeNull();
    expect(fix(none.host)).toBeUndefined();
    expect(sent(none.calls)).toEqual([]);
    await act(async () => { none.store.closeArtifact(); root!.unmount(); });
    root = undefined;
  });
});
