// @vitest-environment happy-dom
// docs/contracts/phone-pairing.md, PAIRING-12: Settings has no Remote access page on obid while the
// iPhone app is off the table; ?everything still shows it.
import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
  window.history.replaceState(null, '', '/');
});

async function pages(search: string, obid: boolean) {
  window.history.replaceState(null, '', `/${search}`);
  if (obid) vi.stubGlobal('app', { ...(window as unknown as { app?: object }).app, obid: {} });
  const { SETTINGS_PAGES } = await import('../SettingsView');
  return SETTINGS_PAGES.map((p) => p.id);
}

describe('PAIRING-12', () => {
  it('pairing_12_remote_access_is_hidden_on_obid', async () => {
    expect(await pages('', true)).not.toContain('remote');
  });
  it('pairing_12_everything_still_shows_remote_access', async () => {
    expect(await pages('?everything', false)).toContain('remote');
  });
});
