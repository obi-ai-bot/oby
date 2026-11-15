// @vitest-environment happy-dom
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// A render error anywhere in the window (docs/contracts/engine.md, ENGINE-20): the page's root
// boundary (RenderError.tsx) tells main, which keeps a crash line and reloads the window once.
const the = vi.hoisted(() => ({ userData: '', marker: 'a-prop-only-the-component-holds', badText: 'a message that can’t be shown' }));
vi.mock('electron', () => ({
  app: { getPath: () => the.userData, getVersion: () => '1.0.0', on: () => {} },
  dialog: {}, ipcMain: {}, shell: {}, safeStorage: {}, BrowserWindow: {},
}));
// Home, with a component under it that throws while it renders, holding a prop
vi.mock('../Home', () => {
  function Bomb({ secret }: { secret: string }): never { throw new TypeError(`cannot read the job (${secret.length})`); }
  return { default: () => <Bomb secret={the.marker} /> };
});
// a dev build always opens on setting up, with Oby in a frame that follows the Mac's pointer
// (main's): Home here instead; and the control center, a window of its own, isn't this test's
vi.mock('../FirstRun', () => ({ default: () => null, sizeWindow: () => {}, useHandover: () => null, useOnboarding: () => null }));
vi.mock('../dev/ControlCenter', () => ({}));
// a message's Markdown that can't be rendered
vi.mock('../textTokens', async (real) => {
  const actual = await real<typeof import('../textTokens')>();
  return { ...actual, tokenize: (s: string) => { if (s.includes(the.badText)) throw new Error('unrenderable'); return actual.tokenize(s); } };
});

beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.resetModules(); document.body.innerHTML = ''; });

describe('ENGINE-20', () => {
  it('engine_20_a_render_error_under_home_sends_one_render_crash_with_its_message_and_no_props', async () => {
    const renderError = vi.fn();
    vi.stubGlobal('app', { renderError });
    const root = document.createElement('div');
    root.id = 'root';
    document.body.append(root);
    await act(async () => { await import('../main'); });
    expect(renderError).toHaveBeenCalledTimes(1);
    const [name, message, stack] = renderError.mock.calls[0];
    expect(name).toBe('TypeError');
    expect(message).toBe(`cannot read the job (${the.marker.length})`);
    expect(stack).toEqual(expect.stringContaining('Bomb'));
    for (const arg of renderError.mock.calls[0]) expect(String(arg)).not.toContain(the.marker);
    expect(root.innerHTML).toBe('');
  });

  it('engine_20_main_records_a_render_crash_and_reloads_once_and_a_second_does_not_reload', async () => {
    the.userData = mkdtempSync(join(tmpdir(), 'oby-render-error-'));
    try {
      const records = await import('../../../main/runMarker');
      const lines = () => readFileSync(join(the.userData, 'crashes.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
      const page = () => { const p = { reloads: 0, reload: () => { p.reloads++; }, isDestroyed: () => false }; return p; };
      const p = page();
      const stack = '\n    at Bomb (http://localhost:5173/src/Home.tsx:3:9)\n    at Home';
      records.renderFailed({ name: 'TypeError', message: 'cannot read the job', componentStack: stack }, p);
      expect(lines()).toEqual([{ kind: 'render', reason: 'TypeError', message: 'cannot read the job', componentStack: 'Bomb\nHome', time: expect.stringMatching(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/) }]);
      expect(p.reloads).toBe(1);
      // the reloaded page loaded, and fails again: recorded, not reloaded
      records.windowLoaded();
      records.renderFailed({ name: 'TypeError', message: 'cannot read the job', componentStack: stack }, p);
      expect(lines()).toHaveLength(2);
      expect(p.reloads).toBe(1);
      // each field cut; a stack's URLs and paths never kept
      records.renderFailed({ name: 'E'.repeat(500), message: 'm'.repeat(5000), componentStack: '\n    in Pane (at /Users/someone/app/Pane.tsx:1)\n\n  at Side (file:///Applications/Oby.app/x.js:2:3)' }, null);
      const cut = lines()[2];
      expect(cut.reason).toHaveLength(100);
      expect(cut.message).toHaveLength(1000);
      expect(cut.componentStack).toBe('Pane\nSide');
      records.renderFailed({ name: 'Error', message: 'm', componentStack: 'at XY\n'.repeat(2000) }, null);
      expect(lines()[3].componentStack).toHaveLength(4000);
      // a fresh run: a closed window, or none, is recorded and not reloaded
      vi.resetModules();
      const fresh = await import('../../../main/runMarker');
      const closed = { ...page(), isDestroyed: () => true };
      fresh.renderFailed({ name: 'TypeError', message: 'm', componentStack: '' }, null);
      fresh.renderFailed({ name: 'TypeError', message: 'm', componentStack: '' }, closed);
      expect(lines()).toHaveLength(6);
      expect(closed.reloads).toBe(0);
    } finally {
      rmSync(the.userData, { recursive: true, force: true });
    }
  });

  it('engine_20_a_bad_message_is_caught_by_its_own_boundary_without_a_report_or_reload', async () => {
    const renderError = vi.fn();
    vi.stubGlobal('app', { renderError });
    const { default: RenderError } = await import('../RenderError');
    const { default: Said } = await import('../Said');
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(<RenderError><Said text={the.badText} /></RenderError>));
    expect(host.innerHTML).toBe(`<p>${the.badText}</p>`);
    expect(renderError).not.toHaveBeenCalled();
    await act(async () => root.unmount());
  });
});
