// Settings › About's Version: the commit the app and obid were built from (src/shared/build.ts,
// docs/build-and-run.md, "Today's dev on this Mac"), and how a build is told it (buildInfo.ts).
import { afterEach, describe, expect, it } from 'vitest';
import { versionText } from '../../../shared/build';
import { buildInfo } from '../../../../buildInfo';

const APP = '150bdab5c0ffee00112233445566778899aabbcc';
const ENGINE = 'dce386cd0000111122223333444455556666777788';

describe('About’s Version', () => {
  it('shows the commit the app and obid were built from, once when it is the same', () => {
    expect(versionText('0.2.23', APP, APP)).toBe('0.2.23 (150bdab)');
  });
  it('names obid’s own commit when it was built from another', () => {
    expect(versionText('0.2.23', APP, ENGINE)).toBe('0.2.23 (150bdab, engine dce386c)');
  });
  it('shows the app’s alone while obid doesn’t say (an older obid, or no answer yet)', () => {
    expect(versionText('0.2.23', APP, null)).toBe('0.2.23 (150bdab)');
    expect(versionText('0.2.23', APP, undefined)).toBe('0.2.23 (150bdab)');
  });
  it('is the bare version for a build that wasn’t told', () => {
    expect(versionText('0.2.23', '', null)).toBe('0.2.23');
    expect(versionText('0.2.23', '', ENGINE)).toBe('0.2.23 (engine dce386c)');
  });
});

describe('what a build bakes in', () => {
  const saved = { ...process.env };
  afterEach(() => { process.env = { ...saved }; });

  it('takes the commit it is given, else git’s HEAD', () => {
    process.env.OBY_BUILD_COMMIT = APP;
    expect(JSON.parse(buildInfo().__OBY_COMMIT__)).toBe(APP);
    delete process.env.OBY_BUILD_COMMIT;
    expect(JSON.parse(buildInfo().__OBY_COMMIT__)).toMatch(/^[0-9a-f]{40}$/);
  });
  it('bakes no scratch obid into an ordinary build', () => {
    delete process.env.OBY_SCRATCH_PORT; delete process.env.OBY_SCRATCH_HOME;
    expect(JSON.parse(buildInfo().__OBY_SCRATCH__)).toBeNull();
  });
  it('bakes a scratch install’s port and home, never 7777 or a relative home', () => {
    process.env.OBY_SCRATCH_PORT = '7901'; process.env.OBY_SCRATCH_HOME = '/tmp/oby-scratch/obid-home';
    expect(JSON.parse(buildInfo().__OBY_SCRATCH__)).toEqual({ port: 7901, home: '/tmp/oby-scratch/obid-home' });
    process.env.OBY_SCRATCH_PORT = '7777';
    expect(() => buildInfo()).toThrow(/other than 7777/);
    process.env.OBY_SCRATCH_PORT = '7901'; process.env.OBY_SCRATCH_HOME = 'obid-home';
    expect(() => buildInfo()).toThrow(/absolute/);
    delete process.env.OBY_SCRATCH_HOME;
    expect(() => buildInfo()).toThrow(/absolute/);
  });
});
