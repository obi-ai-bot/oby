import { beforeEach, describe, expect, it, vi } from 'vitest';
import { atLaunch, type Other } from '../../../shared/engine';

// startObid itself, on a fake launchd, lsof and port: nothing here reaches the Mac's.
const OWN = 'com.deskree.oby.electron.obid', SWIFT = 'com.deskree.oby.obid';
const mac = vi.hoisted(() => ({
  jobs: {} as Record<string, string>, pids: [] as string[], answers: false, loginItem: 'not-registered',
  calls: [] as string[][], registered: [] as boolean[],
}));

vi.mock('electron', () => ({
  app: {
    isPackaged: true, isInApplicationsFolder: () => true, getPath: () => '/nonexistent',
    getLoginItemSettings: () => ({ status: mac.loginItem }),
    setLoginItemSettings: ({ openAtLogin }: { openAtLogin: boolean }) => {
      mac.registered.push(openAtLogin);
      mac.loginItem = openAtLogin ? 'enabled' : 'not-registered';
      if (!openAtLogin) return;
      mac.jobs['com.deskree.oby.electron.obid'] = 'state = running\npid = 200';
      mac.pids = ['200'];
      mac.answers = true;
    },
  },
  dialog: {}, ipcMain: {}, shell: { openExternal: async () => {} },
}));
vi.mock('node:os', async (real) => ({ ...await real<typeof import('node:os')>(), homedir: () => '/nonexistent' }));
vi.mock('node:child_process', () => ({
  execFile: (file: string, args: string[], ...rest: unknown[]) => {
    const done = rest[rest.length - 1] as (e: unknown, r?: unknown) => void;
    mac.calls.push([file, ...args]);
    if (file.endsWith('lsof')) return mac.pids.length ? done(null, { stdout: mac.pids.join('\n') }) : done(new Error('exit 1'));
    if (file.endsWith('launchctl') && args[0] === 'print') {
      const printed = mac.jobs[args[1].split('/').pop()!];
      return printed ? done(null, { stdout: printed }) : done(new Error('no such job'));
    }
    return done(null, { stdout: '' });
  },
}));

const bootedOut = () => mac.calls.filter(([, verb]) => verb === 'bootout').map((c) => c[2].split('/').pop());
async function launch() {
  vi.resetModules();
  const { startObid, obid } = await import('../../../main/obid');
  await startObid();
  return (await obid('GET', '/v1/status')).text;
}

const stopped = 'state = not running\nlast exit code = 0';
const failed = 'state = not running\nlast exit code = 1: Operation not permitted';
const killed = 'state = not running\nlast exit code = 0\nlast terminating signal = Killed: 9';
const other = (printed: string): Other => ({ label: SWIFT, printed });

beforeEach(() => {
  Object.assign(mac, { jobs: {}, pids: [], answers: false, loginItem: 'not-registered', calls: [], registered: [] });
  Object.defineProperty(process, 'platform', { value: 'darwin' });
  vi.stubGlobal('fetch', async () => {
    if (!mac.answers) throw new Error('refused');
    return new Response('{}');
  });
});

describe('the engine at launch', () => {
  it('engine_1_loaded_and_stopped_blocks_nothing_and_stays_untouched', async () => {
    expect(atLaunch('none', [], [other(stopped)], false)).toEqual({ do: 'own', who: 'none', retire: [] });
    mac.jobs[SWIFT] = stopped;
    const text = await launch();
    expect(mac.registered).toEqual([true]);
    expect(bootedOut()).not.toContain(SWIFT);
    expect(text).not.toContain('Swift Oby');
  });

  it('engine_2_loaded_and_answering_is_adopted_and_this_apps_own_stands_down', async () => {
    const running = 'state = running\npid = 100';
    expect(atLaunch('mine', ['100'], [other(running)], false)).toEqual({ do: 'adopt' });
    Object.assign(mac, { jobs: { [SWIFT]: running }, pids: ['100'], answers: true, loginItem: 'enabled' });
    await launch();
    expect(mac.registered).toEqual([false]);
    expect(bootedOut()).toEqual([`${OWN}.session`]);
  });

  it('engine_3_nothing_loaded_starts_this_apps_own', async () => {
    expect(atLaunch('none', [], [], false)).toEqual({ do: 'own', who: 'none', retire: [] });
    await launch();
    expect(mac.registered).toEqual([true]);
    expect(bootedOut()).toEqual([]);
  });

  it('engine_4_one_launchd_would_start_again_gets_its_time_then_goes_until_the_next_login', async () => {
    const starting = 'state = running\npid = 100';
    expect(atLaunch('none', [], [other(starting)], false)).toEqual({ do: 'wait' });
    expect(atLaunch('none', [], [other(starting)], true)).toEqual({ do: 'own', who: 'none', retire: [SWIFT] });
    for (const printed of [failed, killed, 'state = spawn scheduled\nlast exit code = 1']) {
      expect(atLaunch('none', [], [other(printed)], false)).toEqual({ do: 'own', who: 'none', retire: [SWIFT] });
    }
    mac.jobs[SWIFT] = failed;
    await launch();
    expect(bootedOut()).toEqual([SWIFT]);
    expect(mac.registered).toEqual([true]);
  });

  it('engine_5_this_apps_own_on_the_port_stays_whatever_else_is_loaded', async () => {
    expect(atLaunch('mine', ['200'], [other(stopped)], false)).toEqual({ do: 'own', who: 'mine', retire: [] });
    // a loser's moment: launchd says it's running, and the port is this app's
    expect(atLaunch('mine', ['200'], [other('state = running\npid = 100')], false)).toEqual({ do: 'own', who: 'mine', retire: [SWIFT] });
    Object.assign(mac, { jobs: { [SWIFT]: failed, [OWN]: 'state = running\npid = 200' }, pids: ['200'], answers: true, loginItem: 'enabled' });
    await launch();
    expect(mac.registered).toEqual([]);
    expect(bootedOut()).toEqual([SWIFT]);
  });

  it('engine_6_a_foreign_program_on_the_port_shows_taken_and_nothing_changes', async () => {
    expect(atLaunch('foreign', ['300'], [other(stopped)], false)).toEqual({ do: 'taken' });
    Object.assign(mac, { jobs: { [SWIFT]: stopped }, answers: true });
    const text = await launch();
    expect(text).toContain('Something other than your Oby engine answers');
    expect(mac.registered).toEqual([]);
    expect(bootedOut()).toEqual([]);
  });
});
