import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { DEV_NAME, SCRATCH_NAME, ownFolder } from '../../../main/appFolder';

// The app's own Keychain item, "oby-app Safe Storage" (docs/contracts/keychain.md, KEYCHAIN-11 and 12)

const appData = '/Users/someone/Library/Application Support';

describe('KEYCHAIN-11: only the packaged Oby opens the installed Oby’s Keychain item', () => {
  it('keychain_11_the_packaged_oby_keeps_oby_app_and_its_folder', () => {
    expect(ownFolder({ packaged: true, name: 'oby-app', scratchHome: null, appData })).toBeNull();
  });

  it('keychain_11_the_app_run_from_source_keeps_an_item_and_folder_of_its_own', () => {
    const own = ownFolder({ packaged: false, name: 'oby-app', scratchHome: null, appData });
    expect(own).toEqual({ name: DEV_NAME, userData: `${appData}/${DEV_NAME}` });
    // never the installed Oby's: neither its item ("oby-app Safe Storage") nor its folder, whose
    // device-key that item seals
    expect(own!.name).not.toBe('oby-app');
    expect(own!.userData).not.toBe(`${appData}/oby-app`);
  });

  it('keychain_11_a_scratch_install_keeps_its_own_beside_its_obid', () => {
    const scratchHome = '/tmp/scratch/runner/obid-home';
    for (const packaged of [true, false])
      expect(ownFolder({ packaged, name: 'oby-app', scratchHome, appData })).toEqual({ name: SCRATCH_NAME, userData: '/tmp/scratch/runner/app-data' });
  });

  it('keychain_11_a_test_launchers_own_name_and_folder_stay', () => {
    // the drives and tests launch Electron with a launcher of their own ("Electron") and set userData first
    expect(ownFolder({ packaged: false, name: 'Electron', scratchHome: null, appData })).toBeNull();
  });
});

// scripts/sign-identity.sh, as scripts/run-dev.sh runs it: `security find-identity` lines in
const script = fileURLToPath(new URL('../../../../../../scripts/sign-identity.sh', import.meta.url));
const TEAM = 'TEAM000001';
const devId = `  1) 1111111111111111111111111111111111111111 "Developer ID Application: Example Inc (${TEAM})"`;
const otherDevId = '  2) 2222222222222222222222222222222222222222 "Developer ID Application: Someone Else (TEAM000002)"';
const appleDev = '  3) 3333333333333333333333333333333333333333 "Apple Development: A Person (PERSON0001)"';
const pick = (lines: string[], want?: string) => {
  const env = { ...process.env };
  delete env.OBY_SIGN_IDENTITY;
  if (want !== undefined) env.OBY_SIGN_IDENTITY = want;
  const r = spawnSync('/bin/sh', [script, TEAM], { input: [...lines, `     ${lines.length} valid identities found`].join('\n'), env, encoding: 'utf8' });
  return { status: r.status, out: r.stdout.trim().split('\n'), err: r.stderr };
};

describe('KEYCHAIN-12: run-dev.sh signs Oby the same way build after build, never ad hoc by itself', () => {
  it('keychain_12_the_teams_developer_id_comes_first', () => {
    expect(pick([appleDev, otherDevId, devId])).toEqual({ status: 0, out: ['1111111111111111111111111111111111111111', `Developer ID Application: Example Inc (${TEAM})`], err: '' });
  });

  it('keychain_12_else_this_macs_apple_development_identity', () => {
    expect(pick([otherDevId, appleDev]).out).toEqual(['3333333333333333333333333333333333333333', 'Apple Development: A Person (PERSON0001)']);
  });

  it('keychain_12_no_identity_stops_rather_than_sign_ad_hoc', () => {
    const r = pick([otherDevId]);
    expect(r.status).toBe(1);
    expect(r.out).toEqual(['']);
    expect(r.err).toContain('"oby-app Safe Storage" after each one');
    expect(r.err).toContain('OBY_SIGN_IDENTITY=-');
    expect(pick([]).status).toBe(1);
  });

  it('keychain_12_ad_hoc_only_when_asked', () => {
    expect(pick([devId, appleDev], '-')).toEqual({ status: 0, out: ['-', '-'], err: '' });
    expect(pick([], '-').out).toEqual(['-', '-']);
  });

  it('keychain_12_a_named_identity_or_none', () => {
    expect(pick([devId, appleDev], 'Apple Development: A Person').out[0]).toBe('3333333333333333333333333333333333333333');
    const r = pick([devId], 'Apple Development: Nobody');
    expect(r.status).toBe(1);
    expect(r.err).toContain('no signing identity named Apple Development: Nobody');
  });
});
