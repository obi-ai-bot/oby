import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// startObid itself, on a fake launchd, lsof and port: nothing here reaches the Mac's.
const OWN = 'com.deskree.oby.electron.obid', SWIFT = 'com.deskree.oby.obid', SCRIPT = 'com.deskree.obid', SESSION = 'com.deskree.oby.electron.obid.session';
const TAKEN = 'Something other than your Oby engine answers';
const mac = vi.hoisted(() => ({
  jobs: {} as Record<string, string>, pids: [] as string[], answers: false, loginItem: 'not-registered',
  calls: [] as string[][], registered: [] as boolean[], refuse: false, lingers: 0, hangs: false, printHangs: false, inApplications: true,
  // the app's own folder and version, and its app.exit calls (runMarker.ts)
  userData: '/nonexistent', version: '1.0.0', exits: [] as number[],
}));

vi.mock('electron', () => ({
  app: {
    isPackaged: true, isInApplicationsFolder: () => mac.inApplications, getPath: () => mac.userData,
    getVersion: () => mac.version, on: () => {}, exit: (code: number) => { mac.exits.push(code); },
    getLoginItemSettings: () => ({ status: mac.loginItem }),
    setLoginItemSettings: ({ openAtLogin }: { openAtLogin: boolean }) => {
      mac.registered.push(openAtLogin);
      mac.loginItem = openAtLogin ? 'enabled' : 'not-registered';
      if (!openAtLogin) return;
      // launchd starts it: it fails to bind while anything else holds the port
      mac.jobs['com.deskree.oby.electron.obid'] = 'state = running\npid = 200';
      if (mac.pids.length) return;
      mac.pids = ['200'];
      mac.answers = true;
    },
  },
  // Not Now
  dialog: { showMessageBox: async () => ({ response: 1 }) }, ipcMain: {}, shell: { openExternal: async () => {} },
}));
// a home of the test's own: empty, but for obid's token where a test writes one
const HOME = `${process.env.TMPDIR ?? '/tmp'}/oby-engine-test-${process.pid}`;
vi.mock('node:os', async (real) => ({ ...await real<typeof import('node:os')>(), homedir: () => HOME }));
vi.mock('node:child_process', () => ({
  execFile: (file: string, args: string[], ...rest: unknown[]) => {
    const done = rest[rest.length - 1] as (e: unknown, r?: unknown) => void;
    mac.calls.push([file, ...args]);
    if (file.endsWith('lsof')) {
      const pids = [...mac.pids];
      // the booted-out obid leaves the port a little after launchctl comes back
      if (!mac.jobs[SWIFT] && mac.lingers && --mac.lingers === 0) { mac.pids = []; mac.answers = false; }
      return pids.length ? done(null, { stdout: pids.join('\n') }) : done(new Error('exit 1'));
    }
    if (file.endsWith('launchctl')) {
      const label = args[1].split('/').pop()!;
      // a print that never comes back: only execFile's timeout ends it
      if (args[0] === 'print' && mac.printHangs) return rest.length > 1 && (rest[0] as { timeout?: number }).timeout ? done(new Error('timed out')) : undefined;
      if (args[0] === 'print') return mac.jobs[label] ? done(null, { stdout: mac.jobs[label] }) : done(new Error('no such job'));
      // a launchctl that never comes back: only execFile's timeout ends it
      if (mac.hangs && label === SWIFT) return rest.length > 1 && (rest[0] as { timeout?: number }).timeout ? done(new Error('timed out')) : undefined;
      if (args[0] === 'bootout' && label === SWIFT) {
        if (mac.refuse) return done(new Error('refused'));
        delete mac.jobs[label];
      }
    }
    return done(null, { stdout: '' });
  },
}));

const did = (verb: string) => mac.calls.filter(([, v]) => v === verb).map((c) => c[2].split('/').pop());
async function launch() {
  vi.resetModules();
  const { startObid, obid } = await import('../../../main/obid');
  await startObid();
  return (await obid('GET', '/v1/status')).text;
}
// this app's own, up and adopted; then a call as the page makes one
async function adopted() {
  Object.assign(mac, { jobs: { [OWN]: 'state = running\npid = 200' }, pids: ['200'], answers: true, loginItem: 'enabled' });
  const runner = `${HOME}/Library/Application Support/Oby/Runner`;
  mkdirSync(runner, { recursive: true });
  writeFileSync(`${runner}/api-token`, 'token', { mode: 0o600 });
  await launch();
  const { obid } = await import('../../../main/obid');
  return (method = 'GET') => obid(method, '/v1/status');
}

const running = 'state = running\npid = 100';
const stopped = 'state = not running\nlast exit code = 0';

beforeEach(() => {
  rmSync(HOME, { recursive: true, force: true });
  Object.assign(mac, { jobs: {}, pids: [], answers: false, loginItem: 'not-registered', calls: [], registered: [], refuse: false, lingers: 0, hangs: false, printHangs: false, inApplications: true, userData: '/nonexistent', version: '1.0.0', exits: [] });
  Object.defineProperty(process, 'platform', { value: 'darwin' });
  vi.stubGlobal('fetch', async () => {
    // as undici says it
    if (!mac.answers) throw Object.assign(new TypeError('fetch failed'), { cause: { code: 'ECONNREFUSED' } });
    return new Response('{}');
  });
});

describe('the engine at launch', () => {
  it('engine_1_swift_running_off_the_port_goes_for_good_and_this_apps_own_starts', async () => {
    mac.jobs[SWIFT] = running;
    await launch();
    expect(did('bootout')).toContain(SWIFT);
    expect(did('disable')).toEqual([SWIFT]);
    expect(mac.registered).toEqual([true]);
  });

  it('engine_2_swift_stopped_goes_for_good_and_this_apps_own_starts', async () => {
    mac.jobs[SWIFT] = stopped;
    const text = await launch();
    expect(did('bootout')).toEqual([SWIFT]);
    expect(did('disable')).toEqual([SWIFT]);
    expect(mac.registered).toEqual([true]);
    expect(text).not.toContain('Swift Oby');
  });

  it('engine_3_swift_on_the_port_goes_the_port_is_waited_for_and_this_apps_own_starts', async () => {
    // it holds the port for the first look after launchctl comes back
    Object.assign(mac, { jobs: { [SWIFT]: running }, pids: ['100'], answers: true, lingers: 1 });
    const text = await launch();
    expect(did('bootout')).toEqual([SWIFT]);
    expect(did('disable')).toEqual([SWIFT]);
    expect(mac.registered).toEqual([true]);
    expect(text).not.toContain(TAKEN);
  });

  it('engine_4_nothing_loaded_touches_nothing_and_this_apps_own_starts', async () => {
    await launch();
    expect(did('bootout')).toEqual([]);
    expect(did('disable')).toEqual([]);
    expect(mac.registered).toEqual([true]);
  });

  it('engine_5_a_foreign_program_on_the_port_shows_taken_and_nothing_changes', async () => {
    // and the Swift Oby's stays: this app's own couldn't start in its place
    Object.assign(mac, { jobs: { [SWIFT]: stopped }, pids: ['300'], answers: true });
    const text = await launch();
    expect(text).toContain(TAKEN);
    expect(mac.registered).toEqual([]);
    expect(did('bootout')).toEqual([]);
    expect(did('disable')).toEqual([]);
  });

  it('engine_6_swift_refusing_to_go_is_not_adopted_and_the_port_shows_taken', async () => {
    // its 5 seconds for the port, at once
    const now = setTimeout;
    vi.stubGlobal('setTimeout', (done: () => void) => now(done, 0));
    Object.assign(mac, { jobs: { [SWIFT]: running }, pids: ['100'], answers: true, refuse: true, loginItem: 'enabled' });
    const text = await launch();
    vi.stubGlobal('setTimeout', now);
    expect(did('disable')).toEqual([SWIFT]);
    expect(text).toContain(TAKEN);
    expect(mac.registered).toEqual([]);
  });

  it('engine_7_install_sh_on_another_port_is_left_alone_and_this_apps_own_starts', async () => {
    mac.jobs[SCRIPT] = 'state = running\npid = 400';
    await launch();
    expect(did('bootout')).toEqual([]);
    expect(did('disable')).toEqual([]);
    expect(mac.registered).toEqual([true]);
  });

  it('engine_8_install_sh_on_the_port_is_adopted_and_not_booted_out', async () => {
    Object.assign(mac, { jobs: { [SCRIPT]: 'state = running\npid = 400', [SWIFT]: stopped }, pids: ['400'], answers: true, loginItem: 'enabled' });
    const text = await launch();
    expect(mac.registered).toEqual([false]);
    expect(did('bootout')).toEqual([SESSION]);
    expect(did('disable')).toEqual([]);
    expect(text).not.toContain(TAKEN);
  });

  it.each([['stopped', stopped, [], false], ['on the port', running, ['100'], true]])('engine_9_opened_outside_applications_and_not_now_leaves_swift_%s_alone', async (_, printed, pids, answers) => {
    Object.assign(mac, { jobs: { [SWIFT]: printed }, pids, answers, inApplications: false });
    const text = await launch();
    expect(text).toContain('outside Applications');
    expect(did('bootout')).toEqual([]);
    expect(did('disable')).toEqual([]);
    expect(mac.registered).toEqual([]);
  });

  it('engine_10_a_launchctl_that_hangs_times_out_and_this_apps_own_starts', async () => {
    Object.assign(mac, { jobs: { [SWIFT]: stopped }, hangs: true });
    await launch();
    expect(did('bootout')).toEqual([SWIFT]);
    expect(did('disable')).toEqual([SWIFT]);
    expect(mac.registered).toEqual([true]);
  }, 3000);

  it('engine_11_this_apps_own_on_the_port_stays_and_swift_goes', async () => {
    Object.assign(mac, { jobs: { [SWIFT]: stopped, [OWN]: 'state = running\npid = 200' }, pids: ['200'], answers: true, loginItem: 'enabled' });
    await launch();
    expect(did('bootout')).toEqual([SWIFT]);
    expect(did('disable')).toEqual([SWIFT]);
    expect(mac.registered).toEqual([]);
  });
});

describe('the engine while the window is open', () => {
  afterEach(() => vi.useRealTimers());

  it('engine_12_a_restart_within_the_window_answers_waiting_and_then_carries_on', async () => {
    const call = await adopted();
    vi.useFakeTimers();
    // launchctl kickstart -k: the agent stays registered, its obid gone for a moment
    Object.assign(mac, { pids: [], answers: false, calls: [] });
    const during = await Promise.all([call(), call(), call()]);
    expect(during.map((a) => a.why)).toEqual(['waiting', 'waiting', 'waiting']);
    const asked = mac.calls.length;
    expect((await call()).why).toBe('waiting');
    // one shared wait: a call during it runs no lsof or launchctl of its own
    expect(mac.calls.length).toBe(asked);
    Object.assign(mac, { pids: ['200'], answers: true });
    await vi.advanceTimersByTimeAsync(1000);
    expect(await call()).toEqual({ status: 200, text: '{}' });
  });

  it('engine_12_a_stop_past_the_window_gives_the_hard_error_and_then_fails_fast', async () => {
    const call = await adopted();
    vi.useFakeTimers();
    Object.assign(mac, { pids: [], answers: false, jobs: { [OWN]: 'state = not running\nlast exit code = 1' } });
    expect((await call()).why).toBe('waiting');
    await vi.advanceTimersByTimeAsync(21_000);
    const after = await call();
    expect(after.why).toBeUndefined();
    expect(after.text).toContain('Nothing answers on 127.0.0.1:7777');
    expect(after.text).toContain('its last exit code was 1');
    // no wait again for a while: every call fails at once
    await vi.advanceTimersByTimeAsync(20_000);
    expect((await call()).why).toBeUndefined();
  });

  it('engine_12_a_launchctl_that_hangs_still_ends_the_wait', async () => {
    const call = await adopted();
    vi.useFakeTimers();
    Object.assign(mac, { pids: [], answers: false });
    expect((await call()).why).toBe('waiting');
    // saying why at the end of the wait asks launchctl, which hangs now
    mac.printHangs = true;
    await vi.advanceTimersByTimeAsync(21_000);
    const after = await call();
    expect(after.why).toBeUndefined();
    expect(after.text).toContain('Nothing answers on 127.0.0.1:7777');
  });

  it('engine_12_without_this_apps_agent_registered_the_hard_error_is_at_once', async () => {
    const call = await adopted();
    Object.assign(mac, { pids: [], answers: false, jobs: {} });
    expect(await call()).toEqual({ status: 0, text: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' });
  });
});

// runMarker.ts on a userData folder of the test's own
describe('the app’s own run records', () => {
  let records: typeof import('../../../main/runMarker');
  const kept = (name: string) => readFileSync(`${mac.userData}/${name}`, 'utf8');
  const lines = () => kept('crashes.jsonl').trim().split('\n').map((l) => JSON.parse(l));
  // the window's page: its reloads counted
  const page = () => { const p = { reloads: 0, reload: () => { p.reloads++; }, isDestroyed: () => false }; return p; };
  beforeEach(async () => {
    mac.userData = mkdtempSync(`${process.env.TMPDIR ?? '/tmp'}/oby-run-records-`);
    vi.resetModules();
    records = await import('../../../main/runMarker');
  });
  afterEach(() => {
    vi.useRealTimers();
    chmodSync(mac.userData, 0o700);
    rmSync(mac.userData, { recursive: true, force: true });
  });

  it('engine_17_a_gone_renderer_is_recorded', () => {
    const p = page();
    records.rendererGone({ reason: 'crashed', exitCode: 11 }, p);
    expect(lines()).toEqual([{ kind: 'renderer', reason: 'crashed', exitCode: 11, time: expect.stringMatching(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/) }]);
    expect(p.reloads).toBe(1);
    // gone again before that reload loaded: recorded, not reloaded
    records.rendererGone({ reason: 'oom', exitCode: 0 }, p);
    expect(lines().map((l) => l.reason)).toEqual(['crashed', 'oom']);
    expect(p.reloads).toBe(1);
    // once a page loaded, it may be reloaded once again
    records.windowLoaded();
    records.rendererGone({ reason: 'killed', exitCode: 9 }, p);
    expect(p.reloads).toBe(2);
    // a clean exit is recorded, never reloaded
    records.windowLoaded();
    records.rendererGone({ reason: 'clean-exit', exitCode: 0 }, p);
    expect(lines()).toHaveLength(4);
    expect(p.reloads).toBe(2);
    // another page's renderer, or a closed window's: recorded only
    records.rendererGone({ reason: 'crashed', exitCode: 11 }, null);
    records.rendererGone({ reason: 'crashed', exitCode: 11 }, { ...p, isDestroyed: () => true });
    expect(lines()).toHaveLength(6);
    expect(p.reloads).toBe(2);
  });

  it('engine_17_a_gone_child_is_recorded_with_nothing_else', () => {
    records.childGone({ type: 'GPU', reason: 'crashed', exitCode: 5, serviceName: 'gpu', name: 'GPU Process' } as never);
    expect(lines()).toEqual([{ kind: 'GPU', reason: 'crashed', exitCode: 5, time: expect.any(String) }]);
  });

  it('engine_17_crash_lines_are_bounded_and_the_files_are_private', () => {
    records.started(false);
    for (let i = 0; i < 150; i++) records.childGone({ type: 'Utility', reason: 'crashed', exitCode: i });
    const kept = lines();
    expect(kept).toHaveLength(100);
    expect(kept[0].exitCode).toBe(50);
    expect(kept[99].exitCode).toBe(149);
    for (const name of ['run-marker', 'start-attempts.json', 'crashes.jsonl']) expect(statSync(`${mac.userData}/${name}`).mode & 0o777).toBe(0o600);
    // a malformed line is skipped when read
    writeFileSync(`${mac.userData}/crashes.jsonl`, 'not json\n{"kind":"GPU","reason":"oom","exitCode":1,"time":"t"}\n[]\n');
    expect(records.runRecords().crashes).toEqual([{ kind: 'GPU', reason: 'oom', exitCode: 1, time: 't' }]);
  });

  it('engine_18_a_hung_quit_ends_at_30_s', () => {
    vi.useFakeTimers();
    records.started(false);
    records.quitting();
    // a second before-quit starts no second timer
    records.quitting();
    expect(kept('run-marker')).toBe('quitting');
    vi.advanceTimersByTime(29_900);
    expect(mac.exits).toEqual([]);
    vi.advanceTimersByTime(100);
    expect(mac.exits).toHaveLength(1);
    expect(kept('run-marker')).toBe('quit_timeout');
    expect(lines()).toEqual([{ kind: 'app', reason: 'quit_timeout', time: expect.any(String) }]);
    vi.advanceTimersByTime(60_000);
    expect(mac.exits).toHaveLength(1);
    // the next start adds no unclean exit for it
    records.started(false);
    expect(lines()).toHaveLength(1);
  });

  it('engine_18_a_quit_that_ends_in_time_is_clean', () => {
    vi.useFakeTimers();
    records.quitting();
    vi.advanceTimersByTime(10_000);
    records.quit();
    vi.advanceTimersByTime(60_000);
    expect(mac.exits).toEqual([]);
    expect(kept('run-marker')).toBe('clean');
  });

  it('engine_19_failed_starts_are_counted', () => {
    writeFileSync(`${mac.userData}/start-attempts.json`, JSON.stringify({ '0.9.0': 7 }));
    for (let i = 0; i < 3; i++) records.started(false);
    expect(records.runRecords().attempts).toBe(3);
    records.started(false);
    expect(JSON.parse(kept('start-attempts.json'))).toEqual({ '0.9.0': 7, '1.0.0': 4 });
    // a --background start opens no window: it doesn't count
    records.started(true);
    expect(records.runRecords().attempts).toBe(4);
    // the window's page loaded
    records.windowLoaded();
    expect(JSON.parse(kept('start-attempts.json'))).toEqual({ '0.9.0': 7, '1.0.0': 0 });
  });

  it.each([['running', 1], ['quitting', 1], ['clean', 0], ['quit_timeout', 0]])('engine_19_a_start_after_%s_records_an_unclean_exit_%i_times', (previous, times) => {
    writeFileSync(`${mac.userData}/run-marker`, previous);
    records.started(false);
    expect(records.runRecords().crashes).toEqual(times ? [{ kind: 'app', reason: 'unclean_exit', previous, time: expect.any(String) }] : []);
    expect(records.runRecords().marker).toBe('running');
  });

  it.each([
    ['unwritable', () => chmodSync(mac.userData, 0o500)],
    ['linked', () => {
      const elsewhere = `${mac.userData}/elsewhere`;
      writeFileSync(elsewhere, 'running');
      for (const name of ['run-marker', 'start-attempts.json', 'crashes.jsonl']) symlinkSync(elsewhere, `${mac.userData}/${name}`);
    }],
  ])('engine_19_an_%s_folder_never_stops_a_start_a_crash_or_a_quit', (_, make) => {
    vi.useFakeTimers();
    make();
    const p = page();
    expect(() => {
      records.started(false);
      records.windowLoaded();
      records.rendererGone({ reason: 'crashed', exitCode: 11 }, p);
      records.childGone({ type: 'GPU', reason: 'crashed', exitCode: 5 });
      records.quitting();
      vi.advanceTimersByTime(30_000);
      records.quit();
    }).not.toThrow();
    expect(p.reloads).toBe(1);
    expect(mac.exits).toHaveLength(1);
    expect(records.runRecords()).toEqual({ marker: null, attempts: 0, crashes: [] });
  });
});
