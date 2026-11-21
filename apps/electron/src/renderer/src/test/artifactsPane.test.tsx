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
