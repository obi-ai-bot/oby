// @vitest-environment happy-dom
import { Window } from 'happy-dom';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import bridge from '../../../../resources/preview-bridge/bridge.js?raw';

// The preview's bridge (docs/contracts/artifacts-app.md, ARTIFACTSAPP-22), after LlamaCoder's: what a
// page in the preview tells its host page. Its text runs in a window of its own, the host page (`parent`)
// stood in for, so each post it makes is kept.

type Post = { oby: string; message?: string };
const raf = vi.fn();
function page(html = '') {
  const win = new Window({ url: 'http://k3j9-preview.localhost:1/' });
  win.document.body.innerHTML = html;
  const posts: Post[] = [];
  const logged: unknown[][] = [];
  const console = { error: (...args: unknown[]) => { logged.push(args); } };
  Object.assign(win, { requestAnimationFrame: raf });
  new Function('window', 'document', 'parent', 'console', 'requestAnimationFrame', bridge)(win, win.document, { postMessage: (m: Post) => posts.push(m) }, console, raf);
  const load = () => win.dispatchEvent(new win.Event('load'));
  return { win: win as Window & Record<string, unknown>, posts, logged, console, load };
}
// what the probe's computed style says: Tailwind's classes applied or not
const styled = (win: Window, applied: () => boolean) => vi.spyOn(win, 'getComputedStyle').mockImplementation(() => (applied() ? { display: 'grid', paddingTop: '24px' } : { display: 'block', paddingTop: '0px' }) as never);
const TAILWIND = '<script src="/__csb_vendor/tailwindcss-browser/index.global.js"></script>';

beforeEach(() => { vi.useFakeTimers(); raf.mockClear(); vi.stubGlobal('requestAnimationFrame', raf); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

it('artifactsapp_22_the_bridge_says_drawn_only_once_fonts_tailwind_and_react_are_ready', async () => {
  // a plain page: nothing before its load, then drawn, once
  const plain = page('<h1>Hi</h1>');
  await vi.advanceTimersByTimeAsync(1_000);
  expect(plain.posts).toEqual([]);
  plain.load();
  await vi.advanceTimersByTimeAsync(100);
  expect(plain.posts).toEqual([{ oby: 'drawn' }]);
  await vi.advanceTimersByTimeAsync(10_000);
  expect(plain.posts).toHaveLength(1);

  // its fonts: not before they're ready
  const fonts = page('<h1>Hi</h1>');
  let ready!: () => void;
  Object.defineProperty(fonts.win.document, 'fonts', { value: { ready: new Promise<void>((done) => { ready = done; }) } });
  fonts.load();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(fonts.posts).toEqual([]);
  ready();
  await vi.advanceTimersByTimeAsync(100);
  expect(fonts.posts).toEqual([{ oby: 'drawn' }]);

  // Tailwind's tag: not until the probe's classes apply, the probe then gone
  let applied = false;
  const tailwind = page(TAILWIND + '<h1 class="p-4">Hi</h1>');
  styled(tailwind.win, () => applied);
  tailwind.load();
  await vi.advanceTimersByTimeAsync(2_000);
  expect(tailwind.posts).toEqual([]);
  const probe = tailwind.win.document.querySelector('.grid.rounded-md.bg-zinc-50.p-6') as unknown as HTMLElement;
  expect(probe?.style.left).toBe('-9999px');
  applied = true;
  await vi.advanceTimersByTimeAsync(200);
  expect(tailwind.posts).toEqual([{ oby: 'drawn' }]);
  expect(tailwind.win.document.querySelector('.grid.p-6')).toBeNull();
  // or at 5 s, without them
  const never = page(TAILWIND);
  styled(never.win, () => false);
  never.load();
  await vi.advanceTimersByTimeAsync(4_800);
  expect(never.posts).toEqual([]);
  await vi.advanceTimersByTimeAsync(400);
  expect(never.posts).toEqual([{ oby: 'drawn' }]);

  // a React page: not until React says so, a call before the load counting too
  const react = page('<div id="root"></div>');
  react.win.__obyReact = true;
  react.load();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(react.posts).toEqual([]);
  (react.win.__obyRendered as () => void)();
  await vi.advanceTimersByTimeAsync(100);
  expect(react.posts).toEqual([{ oby: 'drawn' }]);
  const early = page('<div id="root"></div>');
  early.win.__obyReact = true;
  (early.win.__obyRendered as () => void)();
  early.load();
  await vi.advanceTimersByTimeAsync(100);
  expect(early.posts).toEqual([{ oby: 'drawn' }]);

  // a page marked as an error says its error in drawn's place
  const marked = page('<pre data-oby-error>Failed to resolve module specifier "recharts"</pre>');
  marked.load();
  await vi.advanceTimersByTimeAsync(100);
  expect(marked.posts).toEqual([{ oby: 'error', message: 'Failed to resolve module specifier "recharts"' }]);

  // every wait on timers, never a frame
  expect(raf).not.toHaveBeenCalled();
});

it('artifactsapp_22_the_bridge_posts_errors_refused_loads_and_console_errors', async () => {
  const p = page('<script src="https://cdn.example.com/x.js"></script><img src="/missing.png">');
  const { win } = p;
  // a throw: its stack, or its message
  const thrown = new Error('boom');
  thrown.stack = 'Error: boom\n    at App (App.js:3:9)';
  win.dispatchEvent(new win.ErrorEvent('error', { error: thrown, message: 'Uncaught Error: boom' }));
  win.dispatchEvent(new win.ErrorEvent('error', { message: 'Script error.' }));
  // a rejection: its stack, its message or itself
  for (const reason of [{ stack: 'Error: nope\n    at run' }, { message: 'nope' }, 'plain']) {
    const e = new win.Event('unhandledrejection');
    Object.assign(e, { reason });
    win.dispatchEvent(e);
  }
  // an element that failed to load: its error doesn't bubble, so only a listener in the capture phase hears it
  win.document.querySelector('script')!.dispatchEvent(new win.Event('error'));
  win.document.querySelector('img')!.dispatchEvent(new win.Event('error'));
  // console.error: still the original's, and posted
  p.console.error('Warning:', { key: 1 }, 'twice');
  expect(p.logged).toEqual([['Warning:', { key: 1 }, 'twice']]);
  expect(p.posts).toEqual([
    { oby: 'error', message: 'Error: boom\n    at App (App.js:3:9)' },
    { oby: 'error', message: 'Script error.' },
    { oby: 'error', message: 'Error: nope\n    at run' },
    { oby: 'error', message: 'nope' },
    { oby: 'error', message: 'plain' },
    { oby: 'error', message: 'Failed to load script https://cdn.example.com/x.js' },
    { oby: 'error', message: 'Failed to load img http://k3j9-preview.localhost:1/missing.png' },
    { oby: 'console-error', message: 'Warning: {"key":1} twice' },
  ]);
  // at most 4,000 characters
  p.posts.length = 0;
  p.console.error('x'.repeat(10_000));
  win.dispatchEvent(new win.ErrorEvent('error', { message: 'y'.repeat(10_000) }));
  expect(p.posts.map((m) => [m.oby, m.message!.length])).toEqual([['console-error', 4_000], ['error', 4_000]]);
  // Recharts' harmless ResizeObserver loop says nothing
  p.posts.length = 0;
  win.dispatchEvent(new win.ErrorEvent('error', { message: 'ResizeObserver loop completed with undelivered notifications.' }));
  win.dispatchEvent(new win.ErrorEvent('error', { message: 'ResizeObserver loop limit exceeded' }));
  expect(p.posts).toEqual([]);
});
