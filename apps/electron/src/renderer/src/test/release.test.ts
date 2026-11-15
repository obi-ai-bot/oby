import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { load } from 'js-yaml';
import { afterEach, describe, expect, it } from 'vitest';

// The Mac release from a v* tag on main (docs/contracts/release.md): the scripts
// .github/workflows/release.yml runs, each on its own in a folder of its own with stand-ins for
// aws and gitleaks, and the workflow's order around them.
const scripts = resolve(__dirname, '../../../../scripts');
const workflow = resolve(__dirname, '../../../../../../.github/workflows/release.yml');
const folders: string[] = [];
function folder() {
  const made = mkdtempSync(join(tmpdir(), 'oby-release-'));
  folders.push(made);
  mkdirSync(join(made, 'bin'));
  return made;
}
function run(dir: string, shell: string, script: string, args: string[], env: Record<string, string> = {}) {
  const { status, stdout, stderr, error } = spawnSync(shell, [join(scripts, script), ...args], { cwd: dir, encoding: 'utf8', env: { PATH: `${join(dir, 'bin')}:/usr/bin:/bin`, HOME: dir, ...env } });
  if (error) throw new Error(`${shell} is missing or couldn't be started (${error.message}): scripts/${script} needs it, as ci.yml's electron lane installs zsh`);
  return { status, words: stdout + stderr };
}
function standIn(dir: string, name: string, body: string) {
  writeFileSync(join(dir, 'bin', name), `#!/bin/sh\n${body}\n`);
  chmodSync(join(dir, 'bin', name), 0o755);
}
// dist as package.sh leaves it for 1.2.3, with a stand-in aws that notes each upload
function built() {
  const dir = folder();
  const dist = join(dir, 'dist');
  mkdirSync(dist);
  const zips = ['Oby-1.2.3-arm64-mac.zip', 'Oby-1.2.3-x64-mac.zip'];
  for (const zip of zips) writeFileSync(join(dist, zip), zip);
  writeFileSync(join(dist, 'Oby-1.2.3-arm64.dmg'), 'dmg');
  const feed = (signed: boolean) => `version: 1.2.3\nfiles:\n${zips.map((z) => `  - url: ${z}\n    sha512: x\n    size: 1\n`).join('')}path: ${zips[0]}\n${signed ? "signature: 'abc'\n" : ''}`;
  const calls = join(dir, 'calls');
  standIn(dir, 'aws', `echo "$*" >> "${calls}"`);
  const said = () => existsSync(calls) ? readFileSync(calls, 'utf8').trim().split('\n') : [];
  const upload = (tag: string) => run(dir, '/bin/zsh', 'upload-release.sh', [tag, 'dist'], { OBY_R2_ENDPOINT: 'https://r2.example.invalid' });
  return { dist, zips, feed, said, upload };
}
const destination = (call: string) => call.split(' ').find((word) => word.startsWith('s3://'));
const steps = () => (load(readFileSync(workflow, 'utf8')) as { jobs: Record<string, { needs?: string; environment?: string; steps: { run?: string }[] }> }).jobs;

describe('the Mac release from a tag on main', () => {
  afterEach(() => { for (const made of folders.splice(0)) rmSync(made, { recursive: true, force: true }); });

  it('release_1_a_tag_publishes_zips_then_the_feed', () => {
    const { dist, zips, feed, said, upload } = built();

    // unsigned, or naming a zip that isn't there: nothing goes up
    writeFileSync(join(dist, 'latest-mac.yml'), feed(false));
    expect(upload('v1.2.3').status).not.toBe(0);
    rmSync(join(dist, zips[1]));
    writeFileSync(join(dist, 'latest-mac.yml'), feed(true));
    expect(upload('v1.2.3').words).toContain(`names ${zips[1]}`);
    expect(said()).toEqual([]);

    // both zips and the DMG first, the feed last, never cached
    writeFileSync(join(dist, zips[1]), zips[1]);
    const published = upload('v1.2.3');
    expect(published.status, published.words).toBe(0);
    const uploads = said();
    expect(uploads.map(destination)).toEqual([
      ...zips.map((zip) => `s3://oby-downloads/electron/${zip}`),
      's3://oby-downloads/electron/Oby-1.2.3-arm64.dmg',
      's3://oby-downloads/electron/latest-mac.yml',
    ]);
    expect(uploads.at(-1)).toContain('--cache-control no-cache');
    for (const call of uploads) expect(call).toContain('--endpoint-url https://r2.example.invalid');

    // in the workflow: packaged and signed, then uploaded, in the job the owner approves
    const mac = steps().mac;
    expect(mac.environment).toBe('release');
    const runs = mac.steps.map((step) => step.run ?? '');
    const packaged = runs.findIndex((r) => r.includes('scripts/package.sh arm64 x64'));
    expect(packaged).toBeGreaterThan(-1);
    expect(runs.findIndex((r) => r.includes('scripts/upload-release.sh "$TAG" dist'))).toBeGreaterThan(packaged);
  });

  it('release_4_a_test_tag_publishes_only_to_the_test_path', () => {
    const { feed, dist, said, upload } = built();
    writeFileSync(join(dist, 'latest-mac.yml'), feed(true));

    // a tag that isn't this build's version, as a release or a test: nothing goes up
    for (const tag of ['v1.2.4', 'v1.2.3-', 'v1.2.3-a/b', 'v1.2', '1.2.3']) {
      const refused = upload(tag);
      expect(refused.status, tag).not.toBe(0);
      expect(refused.words, tag).toContain('nothing is uploaded');
    }
    expect(said()).toEqual([]);

    // a test tag: everything under its own folder of electron-test/, nothing under electron/
    const tested = upload('v1.2.3-test.1');
    expect(tested.status, tested.words).toBe(0);
    const places = said().map(destination);
    expect(places).toHaveLength(4);
    for (const place of places) expect(place).toMatch(/^s3:\/\/oby-downloads\/electron-test\/v1\.2\.3-test\.1\/[^/]+$/);
    expect(places.at(-1)).toBe('s3://oby-downloads/electron-test/v1.2.3-test.1/latest-mac.yml');
  });

  it('release_2_a_tag_off_main_publishes_nothing', () => {
    const dir = folder();
    const git = (...args: string[]) => {
      const r = spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.invalid', '-c', 'commit.gpgsign=false', '-c', 'tag.gpgsign=false', ...args], { cwd: dir, encoding: 'utf8', env: { PATH: '/usr/bin:/bin', HOME: dir, GIT_CONFIG_NOSYSTEM: '1' } });
      expect(r.status, r.stderr).toBe(0);
    };
    git('init', '--quiet', '--initial-branch=main');
    git('commit', '--quiet', '--allow-empty', '-m', 'on main');
    git('tag', 'v1.0.0');
    git('tag', '-a', '-m', 'annotated', 'v1.0.1');
    git('checkout', '--quiet', '-b', 'side');
    git('commit', '--quiet', '--allow-empty', '-m', 'off main');
    git('tag', 'v1.0.2');
    git('update-ref', 'refs/remotes/origin/main', 'main');

    for (const tag of ['v1.0.0', 'v1.0.1']) {
      const on = run(dir, '/bin/bash', 'on-main.sh', [tag, 'origin/main']);
      expect(on.status, on.words).toBe(0);
      expect(on.words).toContain(`${tag} (`);
    }
    const off = run(dir, '/bin/bash', 'on-main.sh', ['v1.0.2', 'origin/main']);
    expect(off.status).not.toBe(0);
    expect(off.words).toContain("isn't on main: nothing is published");
    // no main to check against, or no such tag: nothing either
    expect(run(dir, '/bin/bash', 'on-main.sh', ['v1.0.0', 'origin/nothing']).status).not.toBe(0);
    expect(run(dir, '/bin/bash', 'on-main.sh', ['v9.9.9', 'origin/main']).status).not.toBe(0);

    // in the workflow: only v* tags, and the job with the secrets waits on the guard
    const workflowText = load(readFileSync(workflow, 'utf8')) as { on: { push: { tags: string[]; branches?: string[] } } };
    expect(workflowText.on).toEqual({ push: { tags: ['v*'] } });
    const jobs = steps();
    expect(jobs['on-main'].steps.some((step) => step.run?.includes('apps/electron/scripts/on-main.sh "$TAG" origin/main'))).toBe(true);
    expect(jobs['on-main'].environment).toBeUndefined();
    expect(jobs.mac.needs).toBe('on-main');
  });

  it('release_3_a_secret_in_the_app_stops_the_release', () => {
    const dir = folder();
    mkdirSync(join(dir, 'Oby.app'));
    const scan = (exit: number) => {
      standIn(dir, 'gitleaks', `echo scanned "$@"\nexit ${exit}`);
      return run(dir, '/bin/zsh', 'scan-app.sh', [join(dir, 'bin/gitleaks'), join(dir, 'Oby.app'), join(dir, '.gitleaks.toml'), join(dir, 'gitleaks-arm64')]);
    };
    // a finding, or a scanner that can't run, stops it
    const found = scan(99);
    expect(found.status).not.toBe(0);
    expect(found.words).toContain('possible secret in Oby.app');
    const broken = scan(1);
    expect(broken.status).not.toBe(0);
    expect(broken.words).toContain("gitleaks couldn't scan Oby.app");
    // a clean scan goes on, and it scanned the app with the repository's rules
    const clean = scan(0);
    expect(clean.status, clean.words).toBe(0);
    expect(readFileSync(join(dir, 'gitleaks-arm64.log'), 'utf8')).toContain(`dir ${join(dir, 'Oby.app')} --config ${join(dir, '.gitleaks.toml')}`);

    // package.sh scans each app before it notarizes anything, and stops on a scan that isn't clean;
    // the upload comes after package.sh in the same job, so it never runs either
    const lines = readFileSync(join(scripts, 'package.sh'), 'utf8').split('\n');
    const scanned = lines.findIndex((l) => l.includes('zsh scripts/scan-app.sh'));
    expect(scanned).toBeGreaterThan(-1);
    expect(lines[scanned]).toContain('|| die');
    expect(lines.findIndex((l) => /^\s*notarize "\$app"/.test(l))).toBeGreaterThan(scanned);
    expect(lines.findIndex((l) => l.includes('ditto -c -k --keepParent "$app"'))).toBeGreaterThan(scanned);
  });
});
