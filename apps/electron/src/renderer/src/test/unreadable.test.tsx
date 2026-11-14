// @vitest-environment happy-dom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Settings › General's notice of the files obid kept aside (docs/contracts/home.md, HOME-14).

afterEach(() => { vi.unstubAllGlobals(); vi.resetModules(); document.body.innerHTML = ''; });

async function notice(files: { path: string }[]) {
  vi.stubGlobal('location', { search: '' });
  const { UnreadableFiles } = await import('../SettingsView');
  return renderToStaticMarkup(<UnreadableFiles files={files} />);
}

// Settings › General as a browser shows it on the mock (npm run web), with `search` in its address
async function general(search: string) {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('location', { search });
  const { default: SettingsView } = await import('../SettingsView');
  const host = document.createElement('div');
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(<SettingsView page="general" leaving={false} onClose={() => {}} onGone={() => {}} />));
  const html = host.innerHTML;
  await act(async () => root.unmount());
  return html;
}

describe('the files Oby couldn’t read', () => {
  it('HOME-14: with none, nothing is drawn', async () => {
    expect(await notice([])).toBe('');
  });

  it('HOME-14: each file kept aside is listed by its path, under the title, with no button', async () => {
    const html = await notice([
      { path: '/home/unreadable/jobs/a-1731571200.json' },
      { path: '/home/unreadable/conversations/b-1731574800.json' },
    ]);
    expect(html).toContain('Files Oby couldn’t read');
    expect(html).toContain('<span class="tok tok-path">/home/unreadable/jobs/a-1731571200.json</span>');
    expect(html).toContain('<span class="tok tok-path">/home/unreadable/conversations/b-1731574800.json</span>');
    expect(html).toContain('Oby kept these files aside and didn’t load them. Nothing was deleted.');
    expect(html).not.toContain('<button');
  });

  it('HOME-14: on the mock, Settings › General shows the notice with ?everything only, above Open at login', async () => {
    const plain = await general('');
    expect(plain).toContain('Open at login');
    expect(plain).not.toContain('read</h2>');
    expect(plain).not.toContain('tok-path');
    vi.resetModules();
    const all = await general('?everything');
    const title = all.indexOf('Files Oby couldn’t read');
    expect(title).toBeGreaterThan(-1);
    expect(title).toBeLessThan(all.indexOf('Open at login'));
    expect(all.match(/class="tok tok-path"/g)).toHaveLength(2);
    expect(all.slice(title, all.indexOf('Nothing was deleted.'))).not.toContain('<button');
  });
});
