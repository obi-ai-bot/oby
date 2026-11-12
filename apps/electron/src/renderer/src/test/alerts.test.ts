import { readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fresh, launch, mac, served, titles } from './macAlerts';

// Oby's alerts on this Mac (docs/contracts/alerts.md): main/notify.ts launched, quit and launched
// again over the same app folder, drawing what obid's GET /v1/alerts serves.

const SAID = () => join(mac.userData, 'alerts-said-v2.json');
const permission = served('p1', { title: 'Run npm test?', category: 'permission', item: 'p1', answers: ['Allow', 'Deny'] });

beforeEach(fresh);

describe('ALERTS-1', () => {
  it('alerts_1_the_mac_alerts_what_obid_serves', async () => {
    // a job the Mac once alerted by itself (failed): obid doesn't serve it, so nothing
    mac.jobs = [{ id: 'j9', title: 'Old gate', kind: 'build', status: 'failed', reason: 'boom', dismissed: null, permission: null }];
    mac.inbox = [{ id: 'q1', kind: 'obiRequest', job: '', title: 'Question from alex', text: 'quoted below', options: [], state: 'open', triage: 'needs_you',
      obi: { type: 'question', fromOwner: 'dіma', quoted: { text: 'Which‮ one?' } } }];
    await launch();
    mac.served = [
      // a job's permission: its buttons answer its inbox item, signed
      served('j1', { what: 'permission:p1', title: 'Needs your OK: Fix it', body: 'Run​ npm test', category: 'permission', job: 'j1', thread: 'jobs',
        item: 'j1-permission-p1', answers: ['Allow', 'Deny'] }),
      // a Start: answered as the window does
      served('s1', { title: 'Run the tests', body: 'npm test', category: 'start', item: 's1', conversation: 'c1', thread: 'c1', answers: ['Start'] }),
      // a Confirm never gets a button, whatever comes
      served('c9', { title: 'Confirm', body: 'Delete it\nAll of it', category: 'confirm', item: 'c9', answers: ['Allow'] }),
      // anything else opens its job on a click
      served('j2', { what: 'end:2026-11-12T10:00:00Z', title: `Failed: ${'x'.repeat(120)}`, body: 'boom', category: 'job', job: 'j2', thread: 'jobs' }),
      // another Oby's question, as its card shows it (PEER-30)
      served('q1', { title: 'Question from d\\u{456}ma', body: 'Which one?', item: 'q1' }),
    ];
    // a burst of changes: obid's list is read again
    mac.event('job', { id: 'j1', title: 'Fix it', status: 'running' });
    await vi.waitFor(() => expect(mac.posted).toHaveLength(5), { timeout: 5000 });
    const [ok, start, confirm, failed, question] = mac.posted;
    expect(ok).toMatchObject({ title: 'Needs your OK: Fix it', body: 'Run npm test', groupId: 'jobs', actions: ['Allow', 'Deny'] });
    expect(start).toMatchObject({ title: 'Run the tests', groupId: 'c1', actions: ['Start'] });
    expect(confirm).toMatchObject({ title: 'Confirm', body: 'Delete it\nAll of it', actions: [] });
    expect([...failed.title]).toHaveLength(100);
    expect(failed.actions).toEqual([]);
    expect(question).toMatchObject({ title: 'Question from d\\u{456}ma', body: 'Which one?', actions: [] });
    expect(mac.badge).toBe('5');
    // what was said, under obid's keys
    expect(JSON.parse(readFileSync(SAID(), 'utf8'))).toEqual({ j1: 'permission:p1', s1: 'open', c9: 'open', j2: 'end:2026-11-12T10:00:00Z', q1: 'open' });
    await ok.press('Allow');
    await start.press('Start');
    expect(mac.sent).toEqual([
      { method: 'POST', path: '/v1/inbox/j1-permission-p1/answer', body: { answer: 'Allow' }, signed: true },
      { method: 'POST', path: '/v1/inbox/s1/answer', body: { answer: 'Start' }, signed: false },
    ]);
    failed.click();
    expect(mac.opened).toEqual([{ job: 'j2', conversation: undefined }]);
    // settled anywhere: obid stops serving it, and its banner goes
    mac.served = mac.served.filter((a) => (a as { key: string }).key !== 'c9');
    mac.event('inbox', { id: 'c9', kind: 'confirm', job: '', title: '', text: '', options: [], state: 'done' });
    await vi.waitFor(() => expect(mac.badge).toBe('4'), { timeout: 5000 });
    expect(confirm.closed).toBe(true);
    expect(mac.posted).toHaveLength(5);
  });

  it('alerts_1_a_list_obid_couldnt_serve_is_read_again_by_itself', async () => {
    await launch();
    // obid can't serve its list when Oby opens: nothing changes, and nothing is counted yet
    mac.served = [permission];
    mac.down = true;
    await launch();
    expect(mac.posted).toEqual([]);
    expect(mac.badge).toBe(undefined);
    // it can again: with no event, the Mac reads it again by itself
    mac.down = false;
    await vi.waitFor(() => expect(titles()).toEqual(['Run npm test?']), { timeout: 5000 });
    expect(mac.badge).toBe('1');
    // a burst's read fails: the last list stands (nothing withdrawn), and it's read again by itself
    mac.down = true;
    mac.served = [permission, served('s1', { title: 'Run the tests', category: 'start', item: 's1' })];
    mac.event('job', { id: 'j1', title: 'Fix it', status: 'running' });
    await new Promise((done) => setTimeout(done, 2500));
    expect(mac.posted[0].closed).toBe(false);
    expect(mac.badge).toBe('1');
    mac.down = false;
    await vi.waitFor(() => expect(titles()).toEqual(['Run npm test?', 'Run the tests']), { timeout: 5000 });
    expect(mac.badge).toBe('2');
  });

  it('alerts_1_a_limit_decision_keeps_its_signed_choices', async () => {
    mac.decisions = [{ id: 'd1', revision: 3, account: 'a1', agent: 'Claude', resetAt: null, jobs: [{ id: 'j1', title: 'Fix it' }],
      options: [{ kind: 'move', account: 'a2', agent: 'Codex', name: 'Work' }, { kind: 'stop' }] }];
    await launch();
    mac.served = [served('d1', { title: 'Usage limit', body: '1 task is waiting on Claude’s limit.', category: 'limitDecision', item: 'd1', answers: [] })];
    await launch();
    expect(mac.posted[0].actions).toEqual(['Move all to Work · Codex', 'Keep all stopped']);
    await mac.posted[0].press('Move all to Work · Codex');
    expect(mac.sent).toEqual([{ method: 'POST', path: 'answerLimit', body: ['d1', 3, 'move', 'a2'], signed: true }]);
  });
});

describe('PUBLISHING-31', () => {
  it('publishing_31_publish_kept_after_checks_that_judged_nothing_alerts_nothing', async () => {
    // done with a commit Publish sends: obid serves Ready to publish, which needs the owner
    mac.served = [served('j1', { what: 'end:t', title: 'Ready to publish: Fix it', category: 'ready', job: 'j1' })];
    await launch();
    expect(mac.badge).toBe('1');
    // once obid stopped running its check again (PUBLISHING-31) it serves nothing: Publish stays on its page, quietly
    mac.served = [];
    await launch();
    expect(mac.posted).toEqual([]);
    expect(mac.badge).toBe('');
  });
});

describe('ALERTS-2', () => {
  it('alerts_2_what_waited_while_closed_alerts_once_at_launch', async () => {
    await launch();
    expect(mac.posted).toEqual([]);
    // quit; a permission comes while Oby is closed: the next launch alerts it, and counts it
    mac.served = [permission];
    await launch();
    expect(titles()).toEqual(['Run npm test?']);
    expect(mac.badge).toBe('1');
    // what it said is kept for the next launch, under obid's key
    expect(JSON.parse(readFileSync(SAID(), 'utf8'))).toEqual({ p1: 'open' });
    // a file that can't be parsed keeps nothing quiet: what waits alerts
    writeFileSync(SAID(), '{not json');
    await launch();
    expect(titles()).toEqual(['Run npm test?']);
  });

  it('alerts_2_a_restart_repeats_nothing', async () => {
    // nothing kept (the first launch with this): what's there isn't news, as before; the badge counts it
    mac.served = [permission];
    await launch();
    expect(mac.posted).toEqual([]);
    expect(mac.badge).toBe('1');
    // quit and relaunch with nothing new: nothing said again
    await launch();
    expect(mac.posted).toEqual([]);
    expect(mac.badge).toBe('1');
  });

  it('alerts_2_an_alert_seen_before_a_relaunch_is_not_posted_again', async () => {
    await launch();
    mac.served = [permission];
    await launch();
    expect(titles()).toEqual(['Run npm test?']);
    // seen, then Oby quit and opened again: the banner isn't posted a second time
    await launch();
    expect(mac.posted).toEqual([]);
    // until what it needs changes
    mac.served = [{ ...permission, what: 'permission:p2' }];
    await launch();
    expect(titles()).toEqual(['Run npm test?']);
  });

  it('alerts_2_the_said_file_is_read_without_following_a_link', async () => {
    // a link where the file was, to one that says it was all said: not read, so what waits alerts
    const elsewhere = join(mac.userData, 'elsewhere.json');
    writeFileSync(elsewhere, JSON.stringify({ p1: 'open' }));
    symlinkSync(elsewhere, SAID());
    mac.served = [permission];
    vi.spyOn(console, 'error').mockImplementation(() => {});
    await launch();
    expect(titles()).toEqual(['Run npm test?']);
    // and nothing is written through it
    expect(readFileSync(elsewhere, 'utf8')).toBe(JSON.stringify({ p1: 'open' }));
  });

  it('alerts_2_what_the_mac_said_under_its_own_keys_is_no_news', async () => {
    // the file the Mac kept before obid's keys (alerts-said.json) isn't read: the first launch with
    // obid's keys is a baseline, so nothing alerts twice
    writeFileSync(join(mac.userData, 'alerts-said.json'), JSON.stringify({ 'item:p1': { what: 'open', needs: true } }));
    mac.served = [permission];
    await launch();
    expect(mac.posted).toEqual([]);
  });
});

describe('ALERTS-8', () => {
  it('alerts_8_nothing_alerts_while_the_window_is_in_front', async () => {
    await launch();
    // the window in front: obid serves something that needs the owner, and nothing alerts
    mac.front = true;
    mac.served = [permission];
    const notify = await launch();
    expect(mac.posted).toEqual([]);
    // the badge still counts it
    expect(mac.badge).toBe('1');
    // the window leaves the front: it alerts, once
    mac.front = false;
    notify.windowLeft();
    notify.windowLeft();
    expect(titles()).toEqual(['Run npm test?']);
  });
});
