import { readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apply } from '../../../shared/live-state';
import { unwired, type LiveSnapshot } from '../../../shared/live-wire';
import { fresh, launch, mac, served, titles } from './macAlerts';

// Oby's alerts on this Mac (docs/contracts/alerts.md): main/notify.ts and main/live.ts launched,
// quit and launched again over the same app folder, drawing what obid's GET /v1/alerts serves.

const SAID = () => join(mac.userData, 'alerts-said-v2.json');
const SETTINGS = () => join(mac.userData, 'settings.json');
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

  it('alerts_1_a_streaming_reply_doesnt_read_the_list_again', async () => {
    await launch();
    const before = mac.alertReads;
    // Oby's reply streams: each turn only looks again, so obid scans nothing more
    for (let i = 0; i < 3; i++) mac.event('turn', { id: `t${i}`, conversation: 'c1', who: 'oby', text: 'Working on it', at: new Date().toISOString() });
    await new Promise((done) => setTimeout(done, 2500));
    expect(mac.alertReads).toBe(before);
    // a job's change still reads obid's list again
    mac.event('job', { id: 'j1', title: 'Fix it', status: 'running' });
    await vi.waitFor(() => expect(mac.alertReads).toBe(before + 1), { timeout: 5000 });
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

  it('alerts_2_an_earlier_launch_posts_nothing_into_a_later_one', async () => {
    // an earlier test's launch, its read of obid's list still waiting when that test ends
    await launch();
    mac.event('job', { id: 'j1', title: 'Fix it', status: 'running' });
    fresh();
    // the next test: what the Mac said under its own keys is no news, and nothing else posts into it
    writeFileSync(join(mac.userData, 'alerts-said.json'), JSON.stringify({ 'item:p1': { what: 'open', needs: true } }));
    mac.served = [permission];
    await launch();
    await new Promise((done) => setTimeout(done, 2500));
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

describe('ALERTS-9', () => {
  it('alerts_9_one_follower_of_obids_events_with_alerts_window_open_or_closed', async () => {
    // launched with no window (obid's open -g -j): main follows by itself
    mac.window = false;
    await launch();
    expect(mac.streams).toEqual(['/v1/events?alerts=1']);
    // the window opens, and its page asks main for what it holds
    mac.window = true;
    expect(mac.ask('obid-snapshot')).toMatchObject({ connection: 'live' });
    await new Promise((done) => setTimeout(done, 100));
    expect(mac.streams).toEqual(['/v1/events?alerts=1']);
    // it closes, and a new page asks again
    mac.window = false;
    mac.window = true;
    expect(mac.ask('obid-snapshot')).toMatchObject({ connection: 'live' });
    await new Promise((done) => setTimeout(done, 100));
    expect(mac.streams).toEqual(['/v1/events?alerts=1']);
  });
});

describe('ALERTS-10', () => {
  it('alerts_10_an_item_answered_on_the_iphone_leaves_the_window_and_the_alert_from_the_same_state', async () => {
    const item = { id: 'p1', kind: 'permission', job: '', title: 'Run npm test?', text: 'npm test', options: ['Allow', 'Deny'], created: '2026-11-12T10:00:00Z', state: 'open' };
    await launch();
    mac.served = [permission];
    mac.inbox = [item];
    const { main } = await launch();
    expect(titles()).toEqual(['Run npm test?']);
    // the window's page, as it holds main's state
    let page = unwired((mac.ask('obid-snapshot') as LiveSnapshot).state);
    expect(page.inbox.map((i) => i.id)).toEqual(['p1']);
    // answered on the iPhone: obid's event says it's settled, and obid stops serving it
    const before = mac.page.length;
    mac.served = [];
    mac.event('inbox', { ...item, state: 'done' });
    await vi.waitFor(() => expect(main.obidState().inbox).toEqual([]));
    const told = mac.page.slice(before).filter((m) => m.kind === 'event');
    expect(told).toEqual([expect.objectContaining({ event: { kind: 'inbox', item: { ...item, state: 'done' } } })]);
    for (const m of told) if (m.kind === 'event' && m.event.kind === 'inbox') page = apply(page, m.event);
    expect(page.inbox).toEqual([]);
    await vi.waitFor(() => expect(mac.posted[0].closed).toBe(true), { timeout: 5000 });
    expect(mac.badge).toBe('');
  });
});

describe('ALERTS-15', () => {
  it('alerts_15_a_switch_off_leaves_its_kind_out_of_the_read', async () => {
    // on by default: every kind asked for, in order
    await launch();
    expect(mac.alertPaths.at(-1)).toBe('/v1/alerts?fyi=finished,merged,limit');
    // merged turned off, as settings.json keeps it
    writeFileSync(SETTINGS(), JSON.stringify({ notifyMerged: false }));
    await launch();
    expect(mac.alertPaths.at(-1)).toBe('/v1/alerts?fyi=finished,limit');
    // all three off: the plain list, as obid serves it to the phones
    writeFileSync(SETTINGS(), JSON.stringify({ notifyFinished: false, notifyMerged: false, notifyLimit: false }));
    await launch();
    expect(mac.alertPaths.at(-1)).toBe('/v1/alerts');
  });
});

describe('ALERTS-18', () => {
  const finished = served('j1', { what: 'end:1900000000', title: 'Done: Fix it', body: 'Fixed it.', category: 'job', thread: 'jobs', job: 'j1',
    conversation: 'c1', urgent: false, answers: ['Allow'], fyi: 'finished' });
  const resets = '2030-03-14T15:30:00Z';
  const limit = served('account:work', { what: `limit:${Date.parse(resets) / 1000}`, title: 'Work reached its limit', body: `It resets ${resets}.`,
    category: 'job', thread: 'accounts', urgent: false, fyi: 'limit', account: 'work', resets });

  it('alerts_18_a_finished_task_alerts_with_no_button_and_no_badge', async () => {
    await launch();
    mac.served = [finished];
    await launch();
    expect(mac.posted).toHaveLength(1);
    expect(mac.posted[0]).toMatchObject({ title: 'Done: Fix it', body: 'Fixed it.', groupId: 'jobs', actions: [] });
    expect(mac.badge).toBe('');
  });

  it('alerts_18_a_click_opens_its_conversation_or_settings_agents', async () => {
    await launch();
    mac.served = [finished, limit];
    await launch();
    expect(titles()).toEqual(['Done: Fix it', 'Work reached its limit']);
    mac.posted[0].click();
    mac.posted[1].click();
    expect(mac.opened).toEqual([{ conversation: 'c1' }, { settings: 'agents' }]);
  });

  it('alerts_18_a_limit_alert_shows_its_reset_in_local_time', async () => {
    await launch();
    mac.served = [limit];
    await launch();
    const local = new Date(resets).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' });
    expect(mac.posted[0].body).toBe(`It resets ${local}.`);
    // a reset it can't read: obid's own words
    mac.served = [{ ...limit, what: 'limit:unknown', body: 'Its reset time is unknown.', resets: 'soon' }];
    await launch();
    expect(mac.posted[0].body).toBe('Its reset time is unknown.');
  });

  it('alerts_18_fyi_in_front_never_alerts_later', async () => {
    await launch();
    // served while the window is in front: on screen, so said with no banner
    mac.front = true;
    mac.served = [finished];
    const notify = await launch();
    expect(mac.posted).toEqual([]);
    mac.front = false;
    notify.windowLeft();
    expect(mac.posted).toEqual([]);
    // it was said, so no later look posts it
    expect(JSON.parse(readFileSync(SAID(), 'utf8'))).toEqual({ j1: 'end:1900000000' });
  });

  it('alerts_18_turning_a_kind_on_alerts_nothing_already_there', async () => {
    writeFileSync(SETTINGS(), JSON.stringify({ notifyFinished: false }));
    await launch();
    mac.served = [];
    await launch();
    // turned on: the list read next already holds a finished task, which is no news
    mac.served = [finished];
    expect(mac.ask('notification-kinds', { finished: true })).toEqual({ finished: true, merged: true, limit: true });
    await vi.waitFor(() => expect(mac.alertPaths.at(-1)).toBe('/v1/alerts?fyi=finished,merged,limit'));
    await vi.waitFor(() => expect(JSON.parse(readFileSync(SAID(), 'utf8'))).toEqual({ j1: 'end:1900000000' }));
    expect(mac.posted).toEqual([]);
    // what comes later alerts
    mac.served = [finished, { ...finished, key: 'j2', what: 'end:1900000100', title: 'Done: Ship it' }];
    mac.event('job', { id: 'j2', title: 'Ship it', status: 'done' });
    await vi.waitFor(() => expect(titles()).toEqual(['Done: Ship it']), { timeout: 5000 });
  });

  it('alerts_18_the_three_switches_are_kept_and_on_by_default', async () => {
    await launch();
    expect(mac.ask('notification-kinds')).toEqual({ finished: true, merged: true, limit: true });
    expect(mac.ask('notification-kinds', { merged: false })).toEqual({ finished: true, merged: false, limit: true });
    expect(JSON.parse(readFileSync(SETTINGS(), 'utf8'))).toMatchObject({ notifyMerged: false });
    // anything else is refused
    for (const bad of [{ merged: 'no' }, { sound: true }, [true], 'merged', null]) expect(mac.ask('notification-kinds', bad)).toBe(null);
    // and kept for the next launch
    await launch();
    expect(mac.ask('notification-kinds')).toEqual({ finished: true, merged: false, limit: true });
  });
});

describe('ALERTS-22 to 24', () => {
  // a teammate's review request as obid serves it with its two answers (push.rs `answers`)
  const review = served('obi-c1', { title: 'Review request from alex', body: "alex's Obi asks for a review of acme/app pull request #12.",
    item: 'obi-c1', answers: ['Accept', 'Decline'] });
  const item = { id: 'obi-c1', kind: 'obiRequest', job: '', title: 'Review request from alex', text: "alex's Obi asks for a review of acme/app pull request #12.",
    options: ['Accept', 'Accept, and remember for this repo', 'Decline', 'Decline, and remember for this repo'], state: 'open', triage: 'needs_you',
    obi: { type: 'review_request', fromOwner: 'alex', quoted: { goal: 'Fix it' } } };
  // obid serves `alert` after launch (the first look is the baseline, ALERTS-2): its banner
  const servedAfterLaunch = async (alert: ReturnType<typeof served>) => {
    mac.served = [alert];
    mac.event('inbox', item);
    await vi.waitFor(() => expect(mac.posted).toHaveLength(1), { timeout: 5000 });
    return mac.posted;
  };

  it('alerts_22_a_review_request_banner_answers_its_item_once', async () => {
    mac.inbox = [item];
    await launch();
    const [banner] = await servedAfterLaunch(review);
    expect(banner).toMatchObject({ title: 'Review request from alex', actions: ['Accept', 'Decline'] });
    await banner.press('Accept');
    expect(mac.sent).toEqual([{ method: 'POST', path: '/v1/inbox/obi-c1/answer', body: { answer: 'Accept' }, signed: false }]);
  });

  it('alerts_23_a_review_request_served_with_no_answers_has_no_buttons', async () => {
    mac.inbox = [item];
    await launch();
    const [banner] = await servedAfterLaunch({ ...review, answers: [] });
    expect(banner).toMatchObject({ title: 'Review request from alex', actions: [] });
  });

  it('alerts_24_a_settled_review_request_click_sends_nothing_more', async () => {
    // answered on another device: obid answers 409, and the banner sends nothing more
    mac.inbox = [item];
    mac.receipts = true;
    mac.refusing = 409;
    await launch();
    const [banner] = await servedAfterLaunch(review);
    await banner.press('Accept');
    expect(mac.sent).toHaveLength(1);
    expect(mac.sent[0]).toMatchObject({ path: '/v1/inbox/obi-c1/answer', signed: false });
  });
});
