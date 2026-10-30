import type { Account, Job, Outgoing, TranscriptEntry } from './obid';
import type { Page, Stage, StageId, ToolGroup } from './mock';
import type { Entry, ToolKind } from './session';

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
const MODES: Record<string, string> = { ask: 'Ask', acceptEdits: 'Accept edits', plan: 'Plan', bypass: 'Bypass', sandbox: 'Sandbox' };

// The stages, with how long each took (or should: "≈ 6m"), and the agent's activity and latest
// words under Work
export function stagesOf(j: Job, now = Date.now()): Stage[] {
  return (j.pipeline?.stages ?? []).map((st) => {
    const took = st.seconds != null ? span(st.seconds)
      : st.state === 'current' && st.started ? `${span(since(st.started, now) - (st.waited ?? 0))} so far`
        : st.estimate ? `≈ ${span((st.estimate.low + st.estimate.high) / 2)}` : undefined;
    const work = st.id === 'work';
    return {
      id: st.id as StageId, title: st.title, state: st.state, detail: st.detail ?? undefined, took,
      activity: work && !!j.activity || undefined, sub: work && st.state === 'current' ? j.context || undefined : undefined,
    };
  });
}

// how long it has run: every stage's time, the current one's so far (waiting on the owner left out)
function ran(j: Job, now: number) {
  return (j.pipeline?.stages ?? []).reduce((sum, st) =>
    sum + (st.seconds ?? (st.state === 'current' && st.started ? Math.max(0, since(st.started, now) - (st.waited ?? 0)) : 0)), 0);
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
// Every outgoing action, in words ("Pushed oby/fix-75 to deskree-inc/oby")
export function outgoingText(o: Outgoing) {
  const on = prName(o.id ?? o.repo ?? '');
  switch (o.action) {
    case 'push': return `Pushed ${o.branch} to ${o.repo}`;
    case 'pullRequest': return `Opened pull request ${on}, ready for review`;
    case 'pullRequestEdit': return `Updated the description of ${on}`;
    case 'checkRun': return `Posted a check on ${on}`;
    case 'comment': return `Commented on ${on}`;
    case 'merge': return o.branch ? `Merged ${on} into ${o.branch}` : `Merged ${on}`;
    case 'branchDelete': return `Deleted the branch ${o.branch ?? on}`;
    case 'pullRequestApproval': return `Approved ${on}`;
    default: return `Updated ${on}`;
  }
}

// The page's extras. needs: the task waits on the owner (the track pauses)
export function pageOf(j: Job, account?: Account, needs = false, now = Date.now()): Page {
  const counts = j.activity ? KINDS.filter(([k]) => j.activity!.counts[k]).map(([k, one, many]): [number, string] => [j.activity!.counts[k]!, j.activity!.counts[k] === 1 ? one : many]) : [];
  return {
    mode: j.mode ? MODES[j.mode] : undefined,
    time: timeOf(j, needs, now),
    counts: counts.length ? counts : undefined,
    groups: j.activity?.groups.length ? groupsOf(j.activity, now) : undefined,
    usage: usageOf(j, account),
    outgoing: j.outgoing?.length ? j.outgoing.map((o) => ({ text: outgoingText(o), at: clock(o.date) })) : undefined,
  };
}

// obid's transcript as the session's lines (SessionView.tsx), in order
const TOOLS: Record<string, ToolKind> = { read: 'Read', search: 'Grep', edit: 'Edit', command: 'Bash', test: 'Bash', tetrix: 'Tetrix', web: 'Web' };
export function sessionOf(entries: TranscriptEntry[], now = Date.now()): Entry[] {
  return [...entries].sort((a, b) => a.n - b.n).map((e): Entry => {
    switch (e.kind) {
      case 'start': return { kind: 'start', text: e.summary, at: clock(e.at) };
      case 'said': return { kind: 'said', text: e.text ?? e.summary, at: clock(e.at) };
      case 'thought': return { kind: 'thought', text: e.text ?? '', seconds: e.seconds ?? 0 };
      case 'you': return { kind: 'you', text: e.text ?? e.summary, at: clock(e.at) };
      case 'plan': return { kind: 'plan', todos: e.todos ?? [] };
      case 'tool': {
        const { verb, rest } = verbOf(e.summary);
        // ponytail: a tool kind the filters don't know (mcp, other) shows under All only
        return {
          kind: 'tool', tool: TOOLS[e.activity ?? ''] ?? (e.tool as ToolKind), verb, target: rest,
          state: e.state === 'running' ? 'running' : e.state === 'done' ? 'done' : 'failed',
          seconds: e.seconds ?? (e.state === 'running' ? since(e.at, now) : 0),
          meta: e.exit ? `exit ${e.exit}` : e.added != null || e.removed != null ? `+${e.added ?? 0} −${e.removed ?? 0}` : undefined,
        };
      }
    }
  });
}
