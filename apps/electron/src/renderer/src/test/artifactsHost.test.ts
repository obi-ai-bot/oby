// @vitest-environment happy-dom
import { expect, it, vi } from 'vitest';
import { RENDERED } from '../artifacts/react';

// The preview's host page's "drawn" (docs/contracts/artifacts-app.md, ARTIFACTSAPP-18): what main
// waits on before a card's picture. Sandpack's static client is a stand-in that gives the frame its
// address, as the real one does once its relay is up, and a window of its own (happy-dom loads no frame).

// Each message the stand-in was given goes in `dispatched`.
const dispatched = vi.hoisted(() => [] as unknown[]);
vi.mock('oby-sandpack-static', () => ({
  SandpackStatic: class {
    constructor(frame: HTMLIFrameElement) { Object.defineProperty(frame, 'contentWindow', { value: {} }); frame.setAttribute('src', 'about:blank'); }
    destroy(): void {}
    dispatch(message: unknown): void { dispatched.push(message); }
  },
}));
await import('../artifacts/host');

type Files = Record<string, { text: string }>;
const shown = (files: Files) => { window.obyShow!({ kind: 'html', title: 'Card', files, bundler: 'http://x-preview.localhost' }); return document.querySelector<HTMLIFrameElement>('iframe[data-artifact-preview]')!; };
const settled = async (p: Promise<void>) => Promise.race([p.then(() => true), new Promise<boolean>((r) => setTimeout(() => r(false), 50))]);
const loaded = (frame: HTMLIFrameElement) => frame.dispatchEvent(new Event('load'));
const posted = (source: unknown) => window.dispatchEvent(new MessageEvent('message', { data: RENDERED, source: source as Window }));

it('artifactsapp_18_a_react_artifacts_picture_waits_for_its_first_render', async () => {
  const react = shown({ 'App.tsx': { text: 'export default function App() { return <h1>Hi</h1>; }' } });
  expect(react.contentWindow).not.toBeNull();
  loaded(react);
  posted(window);
  expect(await settled(window.obyDrawn!)).toBe(false);
  posted(react.contentWindow);
  expect(await settled(window.obyDrawn!)).toBe(true);

  const page = shown({ 'index.html': { text: '<h1>Hi</h1>' }, 'App.tsx': { text: 'export default 1;' } });
  loaded(page);
  expect(await settled(window.obyDrawn!)).toBe(true);

  const broken = shown({ 'App.tsx': { text: 'export default function App() { return <h1>; }' } });
  loaded(broken);
  expect(await settled(window.obyDrawn!)).toBe(true);
});

it('artifactsapp_20_reload_runs_the_version_on_view_again', () => {
  shown({ 'index.html': { text: '<h1>Hi</h1>' } });
  dispatched.length = 0;
  window.obyReload!();
  expect(dispatched).toEqual([{ type: 'refresh' }]);
});
