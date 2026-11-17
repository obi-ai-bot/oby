import { createPrivateKey, createPublicKey, generateKeyPairSync, type KeyObject } from 'node:crypto';
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DEV_NAME, SCRATCH_NAME, ownFolder } from '../../../main/appFolder';
import { keptKey } from '../../../main/device';

// The app's own keys and folder (docs/contracts/keychain.md, KEYCHAIN-11 to 13)

const the = vi.hoisted(() => ({ dir: '', calls: [] as string[], opens: true }));
vi.mock('electron', () => ({
  app: { getPath: () => the.dir },
  BrowserWindow: {}, dialog: {},
  // the Keychain, stood in for: every use is counted; a sealed key is `sealed:` and its PEM
  safeStorage: {
    isEncryptionAvailable: () => { the.calls.push('isEncryptionAvailable'); return true; },
    encryptString: (s: string) => { the.calls.push('encryptString'); return Buffer.from(`sealed:${s}`); },
    decryptString: (b: Buffer) => {
      the.calls.push('decryptString');
      if (!the.opens) throw new Error('the user denied access');
      return b.toString().replace(/^sealed:/, '');
    },
  },
}));
vi.mock('../../../main/obid', () => ({ HOME: '/nowhere', obid: async () => ({ status: 200, text: '{}' }), takesKeys: async () => false }));

const pem = (k: KeyObject) => k.export({ type: 'pkcs8', format: 'pem' }) as string;
const pub = (k: KeyObject) => createPublicKey(k).export({ type: 'spki', format: 'der' }).toString('base64');
const p256 = () => generateKeyPairSync('ec', { namedCurve: 'prime256v1' }).privateKey;

beforeEach(() => {
  the.dir = mkdtempSync(join(tmpdir(), 'oby-keys-'));
  the.calls.length = 0;
  the.opens = true;
});

describe('KEYCHAIN-11: only the packaged Oby keeps the installed Oby’s folder and keys', () => {
  const appData = () => '/Users/someone/Library/Application Support';

  it('keychain_11_the_packaged_oby_keeps_oby_app_and_its_folder', () => {
    expect(ownFolder({ packaged: true, name: 'oby-app', scratchHome: null, appData })).toBeNull();
  });

  it('keychain_11_the_app_run_from_source_keeps_a_folder_of_its_own', () => {
    const own = ownFolder({ packaged: false, name: 'oby-app', scratchHome: null, appData });
    expect(own).toEqual({ name: DEV_NAME, userData: `${appData()}/${DEV_NAME}` });
    expect(own!.userData).not.toBe(`${appData()}/oby-app`);
  });

  it('keychain_11_a_scratch_install_keeps_its_own_beside_its_obid', () => {
    const scratchHome = '/tmp/scratch/runner/obid-home';
    for (const packaged of [true, false])
      expect(ownFolder({ packaged, name: 'oby-app', scratchHome, appData })).toEqual({ name: SCRATCH_NAME, userData: '/tmp/scratch/runner/app-data' });
  });

  it('keychain_11_a_test_launchers_own_name_and_folder_stay', () => {
    // the drives and tests launch Electron with a launcher of their own ("Electron") and set userData first
    expect(ownFolder({ packaged: false, name: 'Electron', scratchHome: null, appData })).toBeNull();
    expect(ownFolder({ packaged: false, name: undefined, scratchHome: null, appData })).toBeNull();
  });
});

describe('KEYCHAIN-12: the app’s keys are 0600 files in its folder, and the Keychain is never asked', () => {
  it('keychain_12_a_new_key_is_a_0600_pem_file_and_the_keychain_is_never_asked', () => {
    const made = p256();
    const key = keptKey('device-key', () => made);
    const file = join(the.dir, 'device-key.pem');
    expect(statSync(file).mode & 0o777).toBe(0o600);
    expect(readFileSync(file, 'utf8')).toBe(pem(made));
    expect(pub(key)).toBe(pub(made));
    expect(the.calls).toEqual([]);
  });

  it('keychain_12_the_same_key_every_start', () => {
    const first = keptKey('owner-key', () => generateKeyPairSync('ed25519').privateKey);
    const again = keptKey('owner-key', () => { throw new Error('made again'); });
    expect(pub(again)).toBe(pub(first));
    expect(the.calls).toEqual([]);
  });

  it('keychain_12_never_through_a_link', () => {
    const elsewhere = join(the.dir, 'elsewhere.pem');
    writeFileSync(elsewhere, pem(p256()));
    symlinkSync(elsewhere, join(the.dir, 'device-key.pem'));
    expect(() => keptKey('device-key', p256)).toThrow();
  });

  it('keychain_12_a_file_that_isnt_a_key_is_set_aside_for_a_new_one', () => {
    writeFileSync(join(the.dir, 'device-key.pem'), 'not a key', { mode: 0o600 });
    const made = p256();
    expect(pub(keptKey('device-key', () => made))).toBe(pub(made));
    expect(readdirSync(the.dir).some((f) => f.startsWith('device-key.pem.unreadable-'))).toBe(true);
    expect(the.calls).toEqual([]);
  });
});

describe('KEYCHAIN-13: a key an older Oby sealed in the Keychain is moved once, then the Keychain is never asked again', () => {
  it('keychain_13_a_sealed_key_moves_to_the_file_once_and_stays_the_same_key', () => {
    const old = p256();
    writeFileSync(join(the.dir, 'device-key'), `sealed:${pem(old)}`, { mode: 0o600 });
    const moved = keptKey('device-key', () => { throw new Error('made a new key'); });
    expect(pub(moved)).toBe(pub(old));
    expect(the.calls).toEqual(['isEncryptionAvailable', 'decryptString']);
    expect(readFileSync(join(the.dir, 'device-key.pem'), 'utf8')).toBe(pem(old));
    expect(statSync(join(the.dir, 'device-key.pem')).mode & 0o777).toBe(0o600);
    expect(existsSync(join(the.dir, 'device-key'))).toBe(false);
    // the next start: the file, and nothing asked of the Keychain
    the.calls.length = 0;
    expect(pub(keptKey('device-key', () => { throw new Error('made a new key'); }))).toBe(pub(old));
    expect(the.calls).toEqual([]);
  });

  it('keychain_13_a_sealed_key_that_cant_be_opened_is_set_aside_for_a_new_one', () => {
    the.opens = false;
    writeFileSync(join(the.dir, 'owner-key'), 'sealed:anything', { mode: 0o600 });
    const made = generateKeyPairSync('ed25519').privateKey;
    expect(pub(keptKey('owner-key', () => made))).toBe(pub(made));
    expect(readdirSync(the.dir).some((f) => f.startsWith('owner-key.unreadable-'))).toBe(true);
    expect(createPrivateKey(readFileSync(join(the.dir, 'owner-key.pem'))).asymmetricKeyType).toBe('ed25519');
    the.calls.length = 0;
    keptKey('owner-key', () => { throw new Error('made again'); });
    expect(the.calls).toEqual([]);
  });
});
