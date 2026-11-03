import type { Account, EntryDetail, Job, Outgoing, TranscriptEntry } from './obid';
import type { Page, Stage, StageId, ToolGroup } from './mock';
import { modeLabel } from './modes';
import type { Detail, Entry, ToolKind } from './session';

// obid's job as Sangam's task page shows it (TaskCard.tsx, mock.ts's Page): the stages with their
// times, the time track from obid's estimate, the agent's activity, what the job used and its share
// of the account's windows, and what went out (docs/runner-api.md, #730-#733). A part obid has no
// data for is left out, so his page doesn't draw it. Pure, so it's tested (test/jobPage.test.ts).

// seconds as "22s", "3m 19s", "11m" or "1h 4m"
export function span(seconds: number) {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return m < 10 && s % 60 ? `${m}m ${s % 60}s` : `${m}m`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}
// a range, never one number: "8–16m", "1h 2m–1h 30m"
function range(low: number, high: number) {
  if (high >= 3600) return `${span(low)}–${span(high)}`;
  const l = Math.max(1, Math.round(low / 60));
  return `${l}–${Math.max(l, Math.round(high / 60))}m`;
}
const since = (iso: string, now: number) => (now - Date.parse(iso)) / 1000;

const ENDED = new Set(['done', 'failed', 'stopped', 'interrupted']);
// nothing's running and nobody else has it: a stage it stopped on has no clock. (A done job's review
// or merge stage is current too, with someone at it, so its clock runs.)
const STOPPED = new Set(['failed', 'stopped', 'interrupted']);
// when a current stage's time ends: now, or when the task stopped (none known: it has no time)
const stageEnd = (j: Job, now: number) => (STOPPED.has(j.status) ? (j.finished ? Date.parse(j.finished) : null) : now);

// Whether a follow-up carries `j`'s branch on, as obid counts it for Publish (jobs.rs, `carried`): on
// its branch, at work or done, and not let go. That one publishes it, so `j` doesn't offer to.
export const carriedOn = (j: Job, jobs: Job[]) => jobs.some((o) => o.followUpOf === j.id && o.branch === j.branch && !o.dismissed && ['queued', 'running', 'done'].includes(o.status));

// Publish by hand (PUBLISHING-14), as obid offers it: Publish, or the owner's own word for commits
// Oby wouldn't send (PUBLISHING-9).
export function publishOf(j: Job, followedUp = false): { label: string; anyway?: true; why?: string } | undefined {
  const why = j.publishError ?? undefined;
  if (j.canPublish) return { label: j.published ? 'Publish the new commits…' : 'Publish…', why };
  return j.canPublishAnyway && !followedUp ? { label: 'Publish anyway…', anyway: true, why } : undefined;
}

// The stages, with how long each took (or should: "≈ 6m"), and the agent's activity and latest
// words under Work
export function stagesOf(j: Job, now = Date.now()): Stage[] {
  const stages = j.pipeline?.stages ?? [];
  // why its pull request couldn't be read (gh missing, say): under the stage that waits on it,
  // Merged, else Review, while it's current
  const unread = j.published && j.pullRequestError ? j.pullRequestError : undefined;
  const waits = ['merged', 'review'].find((id) => stages.some((st) => st.id === id && st.state === 'current'));
  return stages.map((st) => {
    const end = stageEnd(j, now);
    const took = st.seconds != null ? span(st.seconds)
      : st.state === 'current' && STOPPED.has(j.status) ? (st.started && end != null ? span(since(st.started, end) - (st.waited ?? 0)) : undefined)
      : st.state === 'current' && st.started ? `${span(since(st.started, now) - (st.waited ?? 0))} so far`
        : st.estimate ? `≈ ${span((st.estimate.low + st.estimate.high) / 2)}` : undefined;
    const work = st.id === 'work';
    return {
      id: st.id as StageId, title: st.title, state: st.state, detail: st.detail ?? undefined, took,
      activity: work && !!j.activity || undefined,
      sub: work && st.state === 'current' ? j.context || undefined : st.id === waits ? unread : undefined,
    };
  });
}

// how long it has run: every stage's time, the current one's so far (waiting on the owner left out)
function ran(j: Job, now: number) {
  return (j.pipeline?.stages ?? []).reduce((sum, st) =>
    sum + (st.seconds ?? (st.state === 'current' && st.started && stageEnd(j, now) != null ? Math.max(0, since(st.started, stageEnd(j, now)!) - (st.waited ?? 0)) : 0)), 0);
}

// The time track: how far in, and when it should be done (from-to), as % of the track; paused
// while it needs the owner. Done, what it took next to the range first given.
function timeOf(j: Job, needs: boolean, now: number): Page['time'] {
  const e = j.estimate;
  if (!e) return undefined;
  const t = ran(j, now);
  if (ENDED.has(j.status)) {
    const first = e.firstGiven ?? e;
    return { aside: j.status === 'done' ? `Took ${span(t)} · estimated ${range(first.low, first.high)}` : `Ran ${span(t)} · jobs like this: ${range(e.low, e.high)}` };
  }
  const track = Math.max(e.high, t) * 1.03, pct = (s: number) => Math.round((s / track) * 100);
  return {
    aside: t < e.high ? `${span(t)} in · done in about ${range(Math.max(e.low - t, 60), e.high - t)}` : `${span(t)} in · jobs like this: ${range(e.low, e.high)}`,
    done: pct(t), from: pct(Math.max(e.low, t)), to: pct(e.high), paused: needs || undefined, basis: e.basis,
  };
}

// tools by kind, as Sangam counts them ("31 read · 12 searches")
const KINDS: [keyof NonNullable<Job['activity']>['counts'], string, string][] = [
  ['read', 'read', 'read'], ['search', 'search', 'searches'], ['edit', 'edit', 'edits'], ['command', 'command', 'commands'],
  ['test', 'test run', 'test runs'], ['tetrix', 'Tetrix', 'Tetrix'], ['web', 'web', 'web'], ['mcp', 'tool', 'tools'],
];
// "Running cargo test": the verb, and what it ran or touched
const verbOf = (summary: string) => { const at = summary.indexOf(' '); return at < 0 ? { verb: summary, rest: '' } : { verb: summary.slice(0, at), rest: summary.slice(at + 1) }; };
const base = (path: string) => path.split('/').pop() ?? path;

function groupsOf(a: NonNullable<Job['activity']>, now: number): ToolGroup[] {
  return a.groups.map((g) => ({
    title: g.title, state: g.state,
    took: span(g.finished ? (Date.parse(g.finished) - Date.parse(g.started)) / 1000 : since(g.started, now)),
    meta: [g.files?.length ? base(g.files[g.files.length - 1]) : '', g.diff ? `+${g.diff.added} −${g.diff.removed}` : ''].filter(Boolean).join(' · ') || undefined,
    tools: g.tools?.map((t) => {
      const { verb, rest } = verbOf(t.summary);
      return { verb, path: rest || undefined, running: t.state === 'running' || undefined, took: t.seconds != null ? span(t.seconds) : undefined };
    }),
  }));
}

const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}K` : String(n));
const share = (points: number | null | undefined) => (points == null || points < 1 ? '<1%' : `${Math.round(points)}%`);

// Usage, as Sangam words it ("3.1M tokens · $4.20 at API prices, covered by your plan"): only
// when a plan covers it, the tool priced it and the account's windows are known
function usageOf(j: Job, account?: Account): Page['usage'] {
  const u = j.usage;
  if (!u || u.costUsd == null || u.priced !== 'plan' || account?.fiveHour == null || account.sevenDay == null) return undefined;
  const sum = (u.input ?? 0) + (u.output ?? 0) + (u.cacheRead ?? 0) + (u.cacheWrite ?? 0);
  return {
    tokens: tokens(sum), cost: `$${u.costUsd.toFixed(2)}`,
    fiveHour: Math.round(account.fiveHour * 100), job5: share(j.windowShare?.fiveHour),
    week: Math.round(account.sevenDay * 100), jobWeek: share(j.windowShare?.sevenDay),
  };
}

// A pull request (or issue) by its short name, "oby#375", from obid's URL or the mock's "owner/repo#375"
export function prName(ref: string) {
  const url = ref.match(/^https:\/\/github\.com\/[^/]+\/([^/]+)\/(?:pull|issues)\/(\d+)/);
  if (url) return `${url[1]}#${url[2]}`;
  const short = ref.match(/^[^/\s]+\/([^/#\s]+)#(\d+)$/);
  return short ? `${short[1]}#${short[2]}` : ref;
}
// its page on GitHub, or none
export function prUrl(ref: string) {
  if (ref.startsWith('https://github.com/')) return ref;
  const short = ref.match(/^([^/\s]+\/[^/#\s]+)#(\d+)$/);
  return short ? `https://github.com/${short[1]}/pull/${short[2]}` : undefined;
}

const clock = (iso: string) => new Date(iso).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
// Every outgoing action, in words ("Pushed oby/fix-75 to deskree-inc/oby"). A check run's id is
// its own URL, so it's named by the job's pull request (published); any other action shows as
// obid names it, with its id (docs/runner-api.md, `outgoing`).
export function outgoingText(o: Outgoing, published?: string | null) {
  const own = o.id && /\/(pull|issues)\/\d+/.test(o.id) ? prName(o.id) : undefined;
  const on = own ?? (published ? prName(published) : o.repo ?? '');
  switch (o.action) {
    case 'push': return `Pushed ${o.branch} to ${o.repo}`;
    case 'pullRequest': return `Opened pull request ${on}, ready for review`;
    case 'pullRequestEdit': return `Updated the description of ${on}`;
    case 'checkRun': return `Posted a check on ${on}`;
    case 'comment': return `Commented on ${on}`;
    case 'merge': return o.branch ? `Merged ${on} into ${o.branch}` : `Merged ${on}`;
    case 'branchDelete': return `Deleted the branch ${o.branch ?? on}`;
    case 'pullRequestApproval': return `Approved ${on}`;
    default: return [o.action, o.id].filter(Boolean).join(' ');
  }
}

// The page's extras. needs: the task waits on the owner (the track pauses)
export function pageOf(j: Job, account?: Account, needs = false, now = Date.now()): Page {
  const counts = j.activity ? KINDS.filter(([k]) => j.activity!.counts[k]).map(([k, one, many]): [number, string] => [j.activity!.counts[k]!, j.activity!.counts[k] === 1 ? one : many]) : [];
  return {
    mode: j.mode ? modeLabel(j.mode) : undefined,
    time: timeOf(j, needs, now),
    counts: counts.length ? counts : undefined,
    groups: j.activity?.groups.length ? groupsOf(j.activity, now) : undefined,
    usage: usageOf(j, account),
    outgoing: j.outgoing?.length ? j.outgoing.map((o) => ({ text: outgoingText(o, j.published), at: clock(o.date) })) : undefined,
  };
}

// obid's transcript as the session's lines (SessionView.tsx), in order
const TOOLS: Record<string, ToolKind> = { read: 'Read', search: 'Grep', edit: 'Edit', command: 'Bash', test: 'Bash', tetrix: 'Tetrix', web: 'Web' };
// obid names a call as it starts ("Reading scan.rs"); ended, it says what it did. Refused, it never ran.
const PAST: Record<string, string> = { Reading: 'Read', Running: 'Ran', Editing: 'Edited', Searching: 'Searched', Looking: 'Looked', Planning: 'Planned', Using: 'Used', Fetching: 'Fetched', Writing: 'Wrote' };
// A line once made stays the same object while its entry does (live.ts keeps those), so a long
// session's unchanged lines don't draw again; a call running has a clock in it, so it's made anew
const lines = new WeakMap<TranscriptEntry, Entry>();
function lineOf(e: TranscriptEntry, now: number): Entry {
  const n = e.n;
  switch (e.kind) {
    case 'start': return { n, kind: 'start', text: e.summary, at: clock(e.at) };
    case 'said': return { n, kind: 'said', text: e.text ?? e.summary, at: clock(e.at) };
    case 'thought': return { n, kind: 'thought', text: e.text ?? '', seconds: e.seconds ?? 0, running: e.state === 'running' || undefined };
    case 'you': return { n, kind: 'you', text: e.text ?? e.summary, at: clock(e.at) };
    case 'plan': return { n, kind: 'plan', todos: e.todos ?? [] };
    case 'tool': {
      const { verb, rest } = verbOf(e.summary);
      // ponytail: a tool kind the filters don't know (mcp, other) shows under All only
      return {
        n, kind: 'tool', tool: TOOLS[e.activity ?? ''] ?? (e.tool as ToolKind), verb: e.state === 'running' || e.state === 'declined' ? verb : PAST[verb] ?? verb, target: rest,
        state: e.state === 'running' || e.state === 'done' || e.state === 'declined' ? e.state : 'failed',
        seconds: e.seconds ?? (e.state === 'running' ? since(e.at, now) : 0),
        meta: e.state === 'declined' ? 'Declined' : e.exit ? `exit ${e.exit}` : e.added || e.removed ? `+${e.added ?? 0} −${e.removed ?? 0}` : undefined,
      };
    }
  }
}
export function sessionOf(entries: TranscriptEntry[], now = Date.now()): Entry[] {
  // one to-do list, the newest: each one obid wrote replaces the one before, as in Claude Code
  const plan = Math.max(0, ...entries.filter((e) => e.kind === 'plan').map((e) => e.n));
  return entries.filter((e) => e.kind !== 'plan' || e.n === plan).sort((a, b) => a.n - b.n).map((e) => {
    const had = lines.get(e);
    if (had) return had;
    const line = lineOf(e, now);
    if (!(e.kind === 'tool' && e.state === 'running')) lines.set(e, line);
    return line;
  });
}

// What obid gives for a call, as Sangam's session draws it: a command and its output, an edit's
// diff, a read's lines, a search's matches
export function detailOf(d: EntryDetail = {}): Detail {
  if (d.kept === false) return { input: 'Oby didn’t keep the details of this one: this task’s are full.' };
  const i = d.input ?? {}, text = (k: string) => (typeof i[k] === 'string' ? i[k] as string : undefined);
  const first = typeof i.offset === 'number' ? i.offset : undefined, many = typeof i.limit === 'number' ? i.limit : undefined;
  const asked = text('command') ?? text('file_path') ?? (text('pattern') ? [text('pattern'), text('path')].filter(Boolean).join('  in ') : undefined) ?? text('query') ?? (Object.keys(i).length ? JSON.stringify(i, null, 2) : undefined);
  return {
    input: asked,
    range: first != null || many != null ? `Lines ${first ?? 1}${many != null ? `–${(first ?? 1) + many - 1}` : ' on'}` : undefined,
    output: d.output?.text,
    lines: d.output?.lines,
    exit: d.output?.exit,
    diff: d.diff?.length ? d.diff.map((f) => f.patch).join('\n') : undefined,
  };
}
