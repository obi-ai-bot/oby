import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { EVERY, FEED, next, onFeed, settings, type Event, type Update } from '../../../shared/updates';

// docs/contracts/updates.md: each test is named after its scenario
const idle: Update = { state: 'idle' };
const found = { version: '0.2.0', date: '2026-10-27T12:00:00.000Z' };
// the events in turn, from idle; what About shows after each, and what main did
function run(auto: boolean, events: Event[], from: Update = idle) {
  const actions: string[] = [];
  let u = from;
  for (const e of events) {
    const [v, a] = next(u, e, auto);
    u = v;
    if (a) actions.push(a);
  }
  return { u, actions };
}

describe('updates', () => {
  it('updates_1_automatic_looks_at_launch_and_every_6_hours_downloads_and_installs_at_quit', () => {
    expect(EVERY).toBe(6 * 60 * 60 * 1000);
    const { u, actions } = run(true, [{ kind: 'start' }, { kind: 'found', ...found }, { kind: 'downloaded', ...found }]);
    expect(actions).toEqual(['check']);
    expect(u).toEqual({ state: 'ready', ...found });
    expect(settings(true)).toMatchObject({ autoDownload: true, autoInstallOnAppQuit: true });
    expect(run(true, [{ kind: 'tick' }], { state: 'current' }).actions).toEqual(['check']);
  });

  it('updates_2_off_never_looks_by_itself_and_update_and_restart_downloads_then_restarts', () => {
    expect(run(false, [{ kind: 'start' }, { kind: 'tick' }]).actions).toEqual([]);
    expect(settings(false)).toMatchObject({ autoDownload: false, autoInstallOnAppQuit: false });
    const looked = run(false, [{ kind: 'check' }, { kind: 'found', ...found }]);
    expect(looked.u).toEqual({ state: 'available', ...found });
    const { u, actions } = run(false, [{ kind: 'install' }, { kind: 'downloaded', ...found }], looked.u);
    expect(actions).toEqual(['download', 'restart']);
    expect(u).toEqual({ state: 'ready', ...found });
  });

  it('updates_3_nothing_newer_is_up_to_date', () => {
    expect(run(false, [{ kind: 'check' }, { kind: 'none' }]).u).toEqual({ state: 'current' });
    expect(run(false, [{ kind: 'check' }], { state: 'current' }).actions).toEqual(['check']);
  });

  it('updates_4_a_failure_installs_nothing_and_offers_check_again', () => {
    for (const from of [{ state: 'checking' }, { state: 'downloading', restart: true, ...found }, { state: 'ready', ...found }] as Update[]) {
      expect(next(from, { kind: 'failed' }, true)).toEqual([idle, null]);
    }
    // with auto on, the next 6 hours look again
    expect(run(true, [{ kind: 'tick' }]).actions).toEqual(['check']);
  });

  it('updates_5_https_feed_on_downloads_oby_dev_never_a_lower_version_or_prerelease', () => {
    expect(FEED).toBe('https://downloads.oby.dev/electron');
    for (const auto of [true, false]) expect(settings(auto)).toMatchObject({ allowDowngrade: false, allowPrerelease: false });
    // the file electron-updater reads in the bundle names the same feed
    const yml = readFileSync(new URL('../../../../build/app-update.yml', import.meta.url), 'utf8');
    expect(yml).toMatch(/^provider: generic$/m);
    expect(yml).toMatch(new RegExp(`^url: ${FEED}$`, 'm'));
    expect(yml).toMatch(/^updaterCacheDirName: \S+$/m);
  });

  it('updates_9_a_file_off_the_feeds_host_or_not_https_is_refused', () => {
    const zip = (url: string) => ({ files: [{ url }] });
    expect(() => onFeed(zip('Oby-Electron-0.2.0-arm64-mac.zip'))).not.toThrow();
    expect(() => onFeed(zip('https://downloads.oby.dev/electron/Oby-Electron-0.2.0-x64-mac.zip'))).not.toThrow();
    for (const url of [
      'https://evil.example/Oby.zip', 'http://downloads.oby.dev/electron/Oby.zip', '//evil.example/Oby.zip',
      'https://downloads.oby.dev.evil.example/Oby.zip', 'https://downloads.oby.dev:8443/Oby.zip', 'file:///tmp/Oby.zip',
    ]) expect(() => onFeed(zip(url)), url).toThrow(/refused/);
    // the legacy path too
    expect(() => onFeed({ files: [], path: 'https://evil.example/Oby.zip' })).toThrow(/refused/);
  });

  it('updates_6_update_and_restart_on_a_downloaded_update_restarts_now', () => {
    expect(run(true, [{ kind: 'install' }], { state: 'ready', ...found }).actions).toEqual(['restart']);
    // clicked while it downloads by itself: restarts once it's downloaded and checked
    const { u, actions } = run(true, [{ kind: 'install' }, { kind: 'downloaded', ...found }], { state: 'downloading', restart: false, ...found });
    expect(actions).toEqual(['restart']);
    expect(u.state).toBe('ready');
  });

  it('updates_7_turning_automatic_on_looks_now_or_downloads_what_was_found_off_stops', () => {
    expect(run(true, [{ kind: 'auto', on: true }]).actions).toEqual(['check']);
    const { u, actions } = run(true, [{ kind: 'auto', on: true }], { state: 'available', ...found });
    expect(actions).toEqual(['download']);
    expect(u).toEqual({ state: 'downloading', restart: false, ...found });
    expect(run(false, [{ kind: 'auto', on: false }, { kind: 'tick' }], { state: 'current' }).actions).toEqual([]);
  });

  it('updates_8_nothing_starts_twice_while_it_looks_or_downloads', () => {
    for (const from of [{ state: 'checking' }, { state: 'downloading', restart: false, ...found }, { state: 'ready', ...found }] as Update[]) {
      for (const e of [{ kind: 'check' }, { kind: 'tick' }, { kind: 'start' }, { kind: 'auto', on: true }] as Event[]) {
        expect(next(from, e, true)).toEqual([from, null]);
      }
    }
    // a second Update and Restart while one downloads for it
    const downloading: Update = { state: 'downloading', restart: true, ...found };
    expect(next(downloading, { kind: 'install' }, false)).toEqual([downloading, null]);
  });
});
