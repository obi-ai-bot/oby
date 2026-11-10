import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// startObid itself, on a fake launchd, lsof and port: nothing here reaches the Mac's.
const OWN = 'com.deskree.oby.electron.obid', SWIFT = 'com.deskree.oby.obid', SCRIPT = 'com.deskree.obid', SESSION = 'com.deskree.oby.electron.obid.session';
const TAKEN = 'Something other than your Oby engine answers';
const mac = vi.hoisted(() => ({
  jobs: {} as Record<string, string>, pids: [] as string[], answers: false, loginItem: 'not-registered',
  calls: [] as string[][], registered: [] as boolean[], refuse: false, lingers: 0, hangs: false, inApplications: true,
}));

vi.mock('electron', () => ({
  app: {
    isPackaged: true, isInApplicationsFolder: () => mac.inApplications, getPath: () => '/nonexistent',
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
  Object.assign(mac, { jobs: {}, pids: [], answers: false, loginItem: 'not-registered', calls: [], registered: [], refuse: false, lingers: 0, hangs: false, inApplications: true });
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

  it('engine_12_without_this_apps_agent_registered_the_hard_error_is_at_once', async () => {
    const call = await adopted();
    Object.assign(mac, { pids: [], answers: false, jobs: {} });
    expect(await call()).toEqual({ status: 0, text: 'Nothing answers on 127.0.0.1:7777: obid isn’t running.' });
  });
});
