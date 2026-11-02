import { beforeEach, describe, expect, it, vi } from 'vitest';

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
vi.mock('node:os', async (real) => ({ ...await real<typeof import('node:os')>(), homedir: () => '/nonexistent' }));
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

const running = 'state = running\npid = 100';
const stopped = 'state = not running\nlast exit code = 0';

beforeEach(() => {
  Object.assign(mac, { jobs: {}, pids: [], answers: false, loginItem: 'not-registered', calls: [], registered: [], refuse: false, lingers: 0, hangs: false, inApplications: true });
  Object.defineProperty(process, 'platform', { value: 'darwin' });
  vi.stubGlobal('fetch', async () => {
    if (!mac.answers) throw new Error('refused');
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
