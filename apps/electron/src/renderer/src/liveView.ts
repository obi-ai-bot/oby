import { LIMIT_REACHED, LIMIT_WORDS, MANAGE_USAGE } from './chatgpt';
import { isInboxKind, isJobStatus, type AskMark, type Conversation, type GitHubWork, type InboxItem, type Job, type LimitDecision, type Turn } from './obid';
import { carriedOn, pageOf, publishOf, stagesOf } from './jobPage';
import { TELL, agentOf, type Ask, type AskKind, type GitHubItem, type Job as JobView, type Message, type Offer } from './view';
import type { Chat } from './chats';
import { repoTicks } from './sharing';
import type { Asking, PersonQuestion, PersonReview, Task } from './tasks';
import { BRANDS } from './brands';
import { plainId } from './peers';
import { actionOn, awaited, starts } from './pending';
import type { Live } from './live';

// obid's state as the screens' conversations and tasks (live.ts keeps the state; these only read
// the one they're given and answer what the screens show, as the mock's stores held it)

export const minutesAgo = (iso: string | null | undefined) => (iso ? Math.max(0, (Date.now() - Date.parse(iso)) / 60_000) : 0);
export const wordsOf = (text: string) => text.match(/\S+\s*/g) ?? [];
// Oby answering words: not a job's report or a routine's run, which come whenever they come
export const answers = (t: Turn) => t.who === 'oby' && !t.ended && !t.routine && !folded(t);
// The cards a turn holds, of a shape this app reads. Any other leaves its turn as words.
const asksOn = (t: Turn): AskMark[] => (Array.isArray(t.asks) ? t.asks : []).filter((a) => !!a && typeof a.owner === 'string' && typeof a.name === 'string' && typeof a.question === 'string' && typeof a.state === 'string');
// obid's own turn about a question, which its card says instead (only those are relayed with a card)
const folded = (t: Turn) => !!t.relayed && asksOn(t).length > 0;
// an answer on its way (pending.ts): its item, or limit card, is off the stack meanwhile
const answering = (s: Live, on: string) => !!actionOn(s.actions, 'answer', on);
// the app's id for one of obid's conversations, and back
const shownIn = (s: Live, c: string) => s.alias[c] ?? c;
const realIn = (s: Live, shown: string) => Object.keys(s.alias).find((k) => s.alias[k] === shown) ?? shown;
// A conversation's line before its words load: its tasks, and once they've all ended, that they have
const ENDED = ['done', 'failed', 'stopped', 'interrupted'];
const tasksLine = (s: Pick<Live, 'jobs'>, tasks?: string[]) => {
  if (!tasks?.length) return '';
  const n = tasks.length, of = `${n} task${n === 1 ? '' : 's'}`;
  return tasks.every((id) => ENDED.includes(s.jobs[id]?.status ?? '')) ? `Finished ${of}` : `Started ${of}`;
};

// What Oby offers to start in a conversation (obid's oby_offer), or in any (c: none, Home's
// stack): its start items still waiting, one card per offer (a turn's), in the order they came.
// Put off with Later, a card is gone until its time.
function offersOf(s: Live, c?: string, items = s.inbox): Offer[] {
  const cards = new Map<string, InboxItem[]>();
  for (const i of items) if (i.kind === 'start' && i.offer && (c === undefined || i.conversation === c)) cards.set(i.offer.id, [...(cards.get(i.offer.id) ?? []), i]);
  return [...cards].filter(([, items]) => items.some((i) => !i.later || Date.parse(i.later) <= Date.now())).map(([id, items]) => ({
    id, title: items.length === 1 ? items[0].title : `${items.length} jobs`,
    jobs: items.map((i) => ({ what: i.offer!.what, task: i.offer!.task })), untrusted: items.some((i) => i.offer!.untrusted),
  }));
}

export function chatOf(s: Live, c: Conversation): Chat {
  // a task's card under the answer that started it, not again under each that mentions it. A
  // job your words started or were offered (its turn), and what Oby offered for them (the offer's
  // id), go under Oby's answer to them: its first turn after them that isn't a job's report or a
  // routine's run
  const carded = new Set<string>();
  const p = s.pace[c.id];
  const byTurn = Object.values(s.jobs).filter((j) => j.conversation === c.id && j.turn).sort((a, b) => a.created.localeCompare(b.created));
  const offers = offersOf(s, c.id);
  const said: Message[] = [];
  // the Confirms Oby's answers made here, each under the first answer that came after it, as on
  // Home's stack: obid's words under the headline, not put off, not on their way (#1680)
  const now = Date.now();
  const confirms = s.inbox.filter((i) => i.kind === 'confirm' && i.conversation === c.id
    && !(i.later && Date.parse(i.later) > now) && !answering(s, i.id));
  const confirmed = new Set<string>();
  const confirm = (m: Omit<Message, 'confirm'>, by?: string) => confirms.filter((i) => (by === undefined || Date.parse(i.created) <= Date.parse(by)) && !confirmed.has(i.id) && confirmed.add(i.id))
    .forEach((i) => said.push({ ...m, confirm: { id: i.id, ask: { ...itemAsk(i), text: i.text } } }));
  let asked: string | undefined, askedWords: Message | undefined;
  // words more words followed with no answer between keep what they started, or its card is lost
  const unanswered = () => {
    const kept = byTurn.filter((j) => j.turn === asked && !carded.has(j.id) && carded.add(j.id)).map((j) => j.id);
    if (kept.length && askedWords) askedWords.jobs = [...(askedWords.jobs ?? []), ...kept];
  };
  const turns = (s.turns[c.id] ?? []).filter((t) => t.id !== p?.turn);
  // and so do the last words once no answer to them is on its way (a Stop, #1788), even under a
  // report later: anything else under way (the next words, a supervisor's turn) isn't for them
  const lastWords = turns.map((t) => t.who).lastIndexOf('user');
  const last = turns[lastWords];
  const coming = !!last && (!!p?.turn || awaited(s.pending, last.id) || (s.on[c.id] === last.id && (c.id in s.working || c.id in s.words || !!p)));
  const settled = !coming && !turns.slice(lastWords + 1).some(answers) ? last : undefined;
  // a question to another Oby is one card: where it first showed, under Oby's next words, as it
  // stands now, in place of the turns that tell of it. Whichever turn holds it, a restart reads the same
  const latest = new Map(turns.flatMap(asksOn).flatMap((a) => (a.exchange ? [[a.exchange, a] as const] : [])));
  const shownAsks = new Set<string>();
  let cards: Message[] = [];
  const dealCards = () => { said.push(...cards); cards = []; };
  for (const t of turns) {
    const held = asksOn(t);
    for (const a of held) {
      if (a.exchange && shownAsks.has(a.exchange)) continue;
      if (a.exchange) shownAsks.add(a.exchange);
      cards.push({ who: 'oby', text: '', ago: minutesAgo(t.at), askCard: a.exchange ? latest.get(a.exchange) : a });
    }
    if (folded(t)) { dealCards(); continue; }
    if (t.who === 'user') unanswered();
    const answer = answers(t) ? asked : undefined;
    const mine = answer ? byTurn.filter((j) => j.turn === answer).map((j) => j.id) : [];
    const jobs = [...t.jobs, ...mine].filter((j) => !carded.has(j) && carded.add(j));
    // the files that went with your words: their tags (a photo's preview while this app has it)
    const files = t.attachments?.map((name) => { const p = s.previews[t.id]?.find((f) => f.name === name); return { name: p?.shown ?? name, preview: p?.preview }; });
    const message: Message = { who: t.who === 'user' ? 'you' : 'oby', text: t.text, ago: minutesAgo(t.at), ...(jobs.length ? { jobs } : {}), ...(files?.length ? { files } : {}) };
    said.push(message);
    for (const offer of offers.filter((o) => o.id === answer)) said.push({ who: 'oby', text: '', ago: minutesAgo(t.at), offer });
    if (t.who === 'oby') confirm({ who: 'oby', text: '', ago: minutesAgo(t.at) }, t.at);
    if (t.who === 'user') { asked = t.id; askedWords = message; if (t === settled || s.unanswered.has(t.id)) unanswered(); }
    else if (answer) asked = undefined;
    if (t.who === 'oby') dealCards();
  }
  const words = p ? wordsOf(p.to).slice(0, p.n).join('') : s.words[c.id], q = s.waiting[c.id];
  if (p || words) said.push({ who: 'oby', text: words ?? '', ago: 0, streaming: true });
  dealCards();
  // its answer still coming (under the words so far), or an event turn's offer: last
  const placed = new Set(said.flatMap((m) => (m.offer ? [m.offer.id] : [])));
  for (const offer of offers.filter((o) => !placed.has(o.id))) said.push({ who: 'oby', text: '', ago: 0, offer });
  confirm({ who: 'oby', text: '', ago: 0 });
  return {
    id: s.alias[c.id] ?? c.id, title: c.title, pinned: c.pinned, at: minutesAgo(c.lastActivity), said, fromObys: c.fromObys, talkAccount: c.talkAccount, talkedOn: c.talkedOn, movedOff: c.movedOff,
    about: tasksLine(s, c.tasks),
    typing: c.id in s.working, step: p || words ? undefined : s.working[c.id],
    waiting: q ? { question: q.text, options: q.options } : undefined,
    // waiting on you with no task to say so (a task's says it itself, Sidebar.tsx): Oby's question back,
    // its cards, a limit on its tasks, and in the one for what other Obys ask, a question only you can answer
    needs: !!q || confirms.length > 0 || offers.length > 0 || limitAsks(s, c.id).length > 0
      || (!!c.fromObys && s.inbox.some((i) => i.kind === 'obiRequest' && i.obi?.type === 'question' && !(i.later && Date.parse(i.later) > now) && i.triage !== 'fyi' && i.triage !== 'ignore')),
  };
}
export function chatsOf(s: Live): Chat[] {
  const chats = [...s.conversations].sort((a, b) => b.lastActivity.localeCompare(a.lastActivity)).map((c) => chatOf(s, c));
  // words starting a conversation obid hasn't listed yet: shown under the app's id meanwhile
  const now = new Date().toISOString();
  const rows: Chat[] = [];
  for (const st of Object.values(s.pending)) {
    if (!starts(st) || [...rows, ...chats].some((c) => c.id === st.shown)) continue;
    const real = st.conversation ?? Object.keys(s.alias).find((k) => s.alias[k] === st.shown);
    const shown = chatOf(s, { id: real ?? st.shown, title: st.said.split('\n')[0].slice(0, 48), pinned: false, created: now, lastActivity: now, cursor: '' });
    rows.push({ ...shown, id: st.shown, said: shown.said.length ? shown.said : [{ who: 'you', text: st.said, ago: 0, ...(st.files.length ? { files: st.files } : {}) }], ...(st.over ? {} : { typing: true, step: shown.step ?? 'Thinking' }) });
  }
  return [...rows, ...chats];
}

// The decision a job's card shows, of its open inbox items: what its agent waits on first (a
// permission or a question, its agent's own among them), else the oldest (an offer like a check to
// keep). The card holds one.
const WAITS = new Set(['permission', 'agentQuestion']);
export const itemOf = (s: Live, job: string) => {
  const items = s.inbox.filter((i) => i.job === job);
  // Oby's own question comes last: it's about the task, and hides no decision it may lead to (a
  // Confirm to go past a safety limit)
  const others = items.filter((i) => i.kind !== 'question');
  return others.find((i) => WAITS.has(i.kind)) ?? others[0] ?? items[0];
};
// Oby's own question: its escalation takes an answer that settles it. A stalled task's
// (supervisor-) takes none: only obid can settle it, so it means something only while it's stopped.
export const takesNoAnswer = (i: InboxItem) => i.id.startsWith('supervisor-');
export const isEscalation = (i: InboxItem) => i.id.startsWith('orchestrator-');
// Escalation on the stack: whole text, and with a job the box that steers it (TASKS-17)
const escalationAsk = (ask: Ask, item: InboxItem, steer: boolean): Ask =>
  isEscalation(item) ? { ...ask, fullText: true, ...(steer && item.job ? { steer: true } : {}) } : ask;

export const limitHolds = (s: Live, j: Job) => !!j.accountLimit || s.limits.some((d) => d.jobs.some((job) => job.id === j.id));
// Failed or interrupted, not let go, no follow-up carrying it on, and no account's limit holding it
// (a limit's job goes on only by its limit card's signed choice, #1563): its menu's Try again and
// Stop here (TASKS-25)
const stoppedHere = (s: Live, j: Job) =>
  (j.status === 'failed' || j.status === 'interrupted') && !limitHolds(s, j) && !j.dismissed && !Object.values(s.jobs).some((o) => o.followUpOf === j.id);

// What a job needs from you: that decision (a permission's command as its detail). A failed or
// interrupted one with no item asks nothing: its reason is a plain line, Try again in its menu (TASKS-25)
export function askOf(s: Live, j: Job): Ask | undefined {
  // a status this build doesn't know: no answer it could judge (docs/contracts/wire.md, WIRE-3)
  if (!isJobStatus(j.status)) return undefined;
  const item = itemOf(s, j.id);
  // The grouped limit appears once, but unrelated permissions or questions still need attention.
  if (limitHolds(s, j) && (!item || item.kind === 'limit')) return undefined;
  if (item) {
    const ask = itemAsk(item, item.kind === 'permission' ? j.permission?.detail : undefined);
    if (item.kind === 'permission' && j.permission?.always) ask.always = j.permission.always;
    // Oby's own question about a task that stopped: its own Done, and his question card's Tell it
    // something (#1795); a stalled task's takes no answer, so Tell it something alone. Try again and
    // Stop here are the task's menu's (TASKS-25). What Oby recommends is in its text, so no button
    // looks recommended
    if (stoppedHere(s, j) && item.kind === 'question') {
      return escalationAsk({ ...ask, options: [...(takesNoAnswer(item) ? [] : ask.options), TELL], recommended: -1 }, item, false);
    }
    // (a task let go: obid settles its question by itself, TASKS-13)
    if (item.kind === 'question' && (j.dismissed || takesNoAnswer(item))) return undefined;
    // still running or done: Done alone only settles it, so the card also has Tell it what to do
    // (TASKS-17), signed as the task page's box; Later stays; the headline stays
    return escalationAsk(ask, item, item.kind === 'question');
  }
}

// An item's decision as a card shows it; detail: a permission's command, else the review a ready
// review's Approve posts (as text). A GitHub offer is Sangam's "Start it?", as on To do's cards,
// an Oby failure his "Failed", and the agent's own question (#858) his Question, the question its
// headline as on his cards, quoted as the agent wrote it, its options and his Tell it something
// for the owner's own words (obid takes any words as the answer), so one with no options is
// answered too.
const LABELLED: Record<string, AskKind> = { github: 'start', failure: 'failed', agentQuestion: 'question' };
export const itemAsk = (item: InboxItem, detail?: string): Ask => {
  // a kind this build doesn't know: obid's words, its kind as its label, and nothing to answer (WIRE-2)
  if (!isInboxKind(item.kind)) return { kind: item.kind as AskKind, headline: item.headline ?? item.title, text: item.text, options: [], unknown: true };
  // Oby's ask for a repository's check (set_check): the command under the question, and the
  // answers in plain words (answerJob turns them into obid's Confirm and Cancel)
  // (Cancel keeps the check a repository has; with none it's No check)
  const check = checkOf(item);
  if (check) {
    const options = ['Use it', 'Edit', item.confirm!.payload?.was ? 'Keep it' : 'No check'];
    return { kind: 'check', headline: `Use this as ${item.confirm!.target}’s check before publishing?`, text: item.text, options, detail: check };
  }
  if (item.kind === 'agentQuestion') return { kind: 'question', headline: item.text, text: '', options: [...item.options, TELL] };
  // A merge card's title is its task's, which the card shows already, after what it is (Not
  // merged, Ready to merge, Needs a teammate): that is its label. With no headline of the model's,
  // obid's reason leads, its first sentence (without the label's words again) over the rest (what to
  // change, then every other hold, one a line), as text shows when headline is null
  // (docs/runner-api.md, #1680, MERGING-21).
  const label = item.kind === 'merge' ? item.title.split(': ')[0] : undefined;
  if (item.kind === 'merge' && !item.headline && item.text) {
    const [line, ...holds] = item.text.split('\n');
    const [first, ...rest] = line.split(/(?<=\.) /);
    const headline = first.replace(/^Not merged( yet)?: (.)/, (_, _yet, c: string) => c.toUpperCase());
    const text = [rest.join(' '), ...holds].filter(Boolean).join('\n');
    return { kind: 'merge', label, headline, text, options: item.options, recommended: item.recommended ?? undefined };
  }
  // A failure's title is its task's, which the card shows already: obid's words say what happened.
  if (item.kind === 'failed' && !item.headline && item.text) {
    return { kind: 'failed', headline: item.text, text: '', options: item.options, recommended: item.recommended ?? undefined };
  }
  return {
    // obid's own question (Oby's escalation, a stalled task): its why and what Oby recommends are
    // its text, shown under its title (TASKS-14)
    kind: LABELLED[item.kind] ?? item.kind as AskKind, headline: item.headline ?? item.title, text: item.headline || item.kind === 'question' ? item.text : '',
    options: item.options, recommended: item.recommended ?? undefined, detail: detail ?? item.watch?.review ?? undefined,
    ...(label ? { label } : {}),
    repos: repoTicks(item),
  };
};
// a check to keep: one taking a check out shows as any confirm item, its Confirm as such
export const checkOf = (item?: InboxItem) =>
  (item?.kind === 'confirm' && item.confirm?.action === 'set_check' ? item.confirm.payload?.check ?? undefined : undefined);

// What obid asks the owner with no task to show it on (a job's item shows on its task's card), for
// Home's stack, of the kinds it was made for: GitHub's offers, Confirms, failures, and work Oby
// offers to start (its job isn't one until it starts), as OfferCard shows it in its conversation.
// Of what another Oby asks, a question and a review request, which their cards show whole
// (QuestionCard.tsx, AskStack.tsx); any other kind stays where it's shown in full (an Obi-to-Obi
// request must show all it quotes before it's answered). Only what needs the owner (as obid sorts it: needs_you, pending with it),
// not put off, and not while its answer is on its way.
const ON_HOME = new Set(['github', 'confirm', 'start', 'failed', 'failure', 'routine']);
// On Home, obid's words under the headline, unless they are the headline (a failure's)
export const homeAsk = (i: InboxItem): Ask => {
  const ask = itemAsk(i);
  const text = ask.headline === i.text ? '' : i.text;
  // Oby's escalation with no task: whole text, Done and Later only (TASKS-17)
  return { ...escalationAsk({ ...ask, text }, i, false), text };
};
// Another Oby's question, by its shape: all but obid's own words are what that Oby wrote
const texts = (list: unknown) => (Array.isArray(list) ? list.filter((x): x is string => typeof x === 'string') : []);
export const questionOf = (i: InboxItem): PersonQuestion | undefined => {
  const o = i.obi, words = o?.quoted?.text;
  if (i.kind !== 'obiRequest' || o?.type !== 'question' || typeof words !== 'string' || typeof o.fromOwner !== 'string') return undefined;
  return { id: i.id, from: o.fromOwner, words, choices: texts(o.quoted?.choices), repos: texts(o.repos) };
};
// Another Oby's review request (TEAMREVIEW-2), by its shape: obid's words (its text, typed fields
// only), what that Oby wrote quoted under obi.quoted (core/crates/obid/src/sphere/inbox.rs,
// review_request_item), and the item's own answers
const word = (x: unknown) => (typeof x === 'string' ? x : '');
export const reviewOf = (i: InboxItem): PersonReview | undefined => {
  const o = i.obi, q = o?.quoted;
  if (i.kind !== 'obiRequest' || o?.type !== 'review_request' || typeof o.fromOwner !== 'string') return undefined;
  const conditions = (Array.isArray(q?.acceptance) ? q.acceptance as unknown[] : [])
    .map((c) => ({ text: word((c as { text?: unknown } | null)?.text), note: word((c as { note?: unknown } | null)?.note) }))
    .filter((c) => c.text || c.note);
  return {
    id: i.id, from: o.fromOwner, text: i.text, repo: word(o.repo), headSha: word(o.headSha),
    goal: word(q?.goal), summary: word(q?.summary), conditions, options: texts(i.options),
  };
};
// An escalation with no job still reaches the stack (TASKS-17): Done and Later, whole text
const onHome = (i: InboxItem) => ON_HOME.has(i.kind) || !!questionOf(i) || !!reviewOf(i) || (isEscalation(i) && i.kind === 'question' && !i.job);
export function asksOf(s: Live): Asking[] {
  const now = Date.now();
  const open = s.inbox.filter((i) => onHome(i) && !s.jobs[i.job] && (!i.triage || i.triage === 'needs_you' || i.triage === 'pending')
    && !(i.later && Date.parse(i.later) > now) && !answering(s, i.id));
  return [
    ...open.filter((i) => !i.offer).map((i) => {
      const question = questionOf(i);
      if (question) return { id: i.id, title: `Question from ${plainId(question.from)}`, question };
      const review = reviewOf(i);
      if (review) return { id: i.id, title: `Review request from ${plainId(review.from)}`, review };
      // over it a GitHub item's own title, as on To do; obid's words under Oby's headline, or under
      // the title till one comes (clamped: they can be long; an escalation's is whole, TASKS-17)
      const g = i.github;
      return {
        id: i.id, ask: homeAsk(i), title: g?.quoted?.title ? `#${g.number} ${g.quoted.title}` : i.title, talk: i.conversation ? shownIn(s, i.conversation) : undefined,
        ...(g ? { who: g.list === 'reviews' ? g.author ?? undefined : undefined, brand: BRANDS.github, url: g.url } : {}),
      };
    }),
    // an offer under the conversation it was made in, the way to it one click away
    ...offersOf(s, undefined, open).map((offer) => {
      const c = open.find((i) => i.offer?.id === offer.id)?.conversation;
      return { id: offer.id, title: s.conversations.find((x) => x.id === c)?.title ?? offer.title, offer, talk: c ? shownIn(s, c) : undefined };
    }),
    ...limitAsks(s),
  ];
}

// Waiting for you (docs/contracts/home.md, WAITING-1 to 3): everything across the profile's
// projects that needs the owner's word, as obid sorts it (needs_you), and every merge card
// (MERGING-21); open, not put off, not while its answer is on its way (its entry, pending.ts);
// longest waiting first. A view over GET /v1/inbox, with no state of its own.
export function waitingOf(s: Live): InboxItem[] {
  const now = Date.now();
  return s.inbox.filter((i) => i.state === 'open' && (i.triage === 'needs_you' || i.kind === 'merge')
    && !(i.later && Date.parse(i.later) > now) && !answering(s, i.id))
    .sort((a, b) => Date.parse(a.created) - Date.parse(b.created));
}
// the project an item is in: its task's repository, else GitHub's, else none ('')
export const projectOf = (s: Live, i: InboxItem) => s.jobs[i.job]?.repo || i.github?.repo || '';
// The tools whose reviews of Oby's approved the head obid last pushed to the job's branch, as obid
// counts them (job.rs, approving_tools): its approvals and its latest review, each tool once
export const approvingTools = (j: Job) => {
  const head = (j.outgoing ?? []).filter((o) => o.action === 'push' && o.branch === j.branch).at(-1)?.id;
  return [...new Set([...(j.approvals ?? []), ...(j.review ? [j.review] : [])]
    .filter((r) => r.verdict === 'approved' && !!head && r.commit === head).map((r) => r.agent))].sort();
};

// An account's limit (#1563): one card for all the jobs it stopped, its answers obid's options. The
// card's id carries the revision it shows, so a choice covers exactly the jobs and destination on
// it: once obid's decision changes, the old card's answer is refused (answerJob) and a new card
// takes its place. conversation/job: only the decisions for that conversation's or job's work.
export const limitLabel = (o: LimitDecision['options'][number]) =>
  o.kind === 'move' ? `Move all to ${o.name} · ${o.agent}` : o.kind === 'resume' ? 'Resume when available' : 'Keep stopped';
export const limitLater = new Map<string, number>();  // Later: put off an hour, in this window only
export const limitError = new Map<string, string>();  // why the last answer to it didn't go
export function limitAsks(s: Live, conversation?: string, job?: string): Asking[] {
  const now = Date.now();
  return s.limits.filter((d) => (limitLater.get(`${d.id}:${d.revision}`) ?? 0) < now && !answering(s, `limit:${d.id}:${d.revision}`)
    && (!conversation && !job || d.jobs.some((j) => job === j.id || (!!conversation && s.jobs[j.id]?.conversation === realIn(s, conversation)))))
    .map((d) => {
      const source = s.status?.accounts.find((a) => a.id === d.account);
      const reset = d.resetAt != null && Number.isFinite(d.resetAt)
        ? new Date(d.resetAt * 1000).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' }) : null;
      const n = d.jobs.length;
      // ChatGPT's limit names no time (docs/contracts/chatgpt-sign-in.md), and OpenAI's own words go first
      const plan = source?.login === 'chatgpt';
      const text = [
        `${n} ${n === 1 ? 'task is' : 'tasks are'} waiting: ${d.jobs.map((j) => j.title).join(', ')}.`,
        plan ? LIMIT_WORDS : reset ? `It resets ${reset}.` : 'Its reset time is unknown.',
        source?.fiveHour != null ? `Five-hour usage: ${Math.round(source.fiveHour * 100)}%.` : '',
        limitError.get(d.id) ?? d.problem ?? '',
      ].filter(Boolean).join(' ');
      return {
        id: `limit:${d.id}:${d.revision}`, title: source?.name ?? d.agent,
        brand: agentOf(d.agent).brand,
        ask: { kind: 'limit', headline: plan ? LIMIT_REACHED : `${source?.name ?? d.agent} reached its limit`, text, options: [...(plan ? [MANAGE_USAGE] : []), ...d.options.map(limitLabel)] },
      };
    });
}

// GitHub's work no job is on yet (once one is, the job is the task), as To do's cards: reviews and
// your own pull requests under Review PR (as the Swift app had them: Sangam's design has no group
// for your own), assigned issues under Issues. A tap starts Oby's job where it can: not on a draft,
// an issue in a repository Oby doesn't work in, or your own pull request unless its checks failed
// on a branch of the repository itself; a review in a repository Oby doesn't work in asks your
// Confirm first, which adds it.
const REVIEWED: Record<string, string> = { APPROVED: 'Approved', CHANGES_REQUESTED: 'Changes requested', REVIEW_REQUIRED: 'Waiting for review' };
const CHECKED: Record<string, string> = { SUCCESS: 'checks passed', FAILURE: 'checks failed', ERROR: 'checks failed', PENDING: 'checks running', EXPECTED: 'checks running' };
export function workOf(w: GitHubWork | null): GitHubItem[] {
  return (w ? [...w.reviews, ...w.mine, ...w.assigned] : []).filter((g) => !g.job).map((g) => {
    const name = g.repo.split('/')[1] ?? g.repo, at = `${name} #${g.number}`;
    const red = g.checks === 'FAILURE' || g.checks === 'ERROR';
    const action = g.draft ? undefined : g.list === 'reviews' ? 'Review it' as const
      : g.known && (g.list === 'assigned' || (red && !g.fork)) ? 'Fix it' as const : undefined;
    // where your own stands: "Changes requested, checks failed."
    const stands = [REVIEWED[g.reviewDecision ?? ''], CHECKED[g.checks ?? '']].filter(Boolean).join(', ');
    return {
      id: g.id, type: g.list === 'assigned' ? 'issues' : 'review', who: g.list === 'reviews' ? g.author ?? undefined : undefined,
      title: `#${g.number} ${g.title}`, repo: name, url: g.url, board: g.board ?? undefined, updated: minutesAgo(g.updated), action,
      request: g.list === 'reviews' ? `${g.author ?? 'Someone'} asked you to review ${at}.` : g.list === 'assigned' ? `Assigned to you in ${at}.`
        : stands ? `${stands[0].toUpperCase()}${stands.slice(1)}.` : `Open in ${at}.`,
      label: action ? undefined : g.list === 'mine' ? 'Your pull request' : 'On GitHub',
      note: g.list === 'mine' ? undefined : g.draft ? 'It’s a draft: Oby can review it once it’s ready for review.' : g.known ? undefined
        : g.list === 'reviews' ? `Oby doesn’t work in ${g.repo} yet: Review it asks you to confirm adding it.` : `Oby doesn’t work in ${g.repo} yet: add it to repos in obid’s configuration.`,
    };
  });
}

export function taskOf(s: Live, j: Job): Task {
  // a status this build doesn't know: placed as done, with no ask, shown neutral (WIRE-3)
  const unknown = !isJobStatus(j.status);
  // answered here: answered, as it was asked, until obid's state holds what it answered newer (its
  // entry, pending.ts). Put off: until its time, by obid's item, so a restart keeps it
  const now = askOf(s, j), item = itemOf(s, j.id), was = unknown ? undefined : actionOn(s.actions, 'answer', item?.id ?? j.id);
  const putOff = !!now && (item?.later ? Date.parse(item.later) > Date.now() : was?.option === 'Later');
  const answer = putOff ? 'Later' : was?.option !== 'Later' ? was?.option : undefined;
  const ask = answer && !putOff ? was!.ask ?? now : now;
  const title = actionOn(s.actions, 'rename', j.id)?.title ?? j.title;
  const job: JobView = {
    id: j.id, type: j.kind === 'review' ? 'review' : j.issue != null ? 'issues' : 'tasks', title, request: j.request,
    agent: j.agent, account: s.status?.accounts.find((a) => a.id === j.account)?.name ?? j.account,
    model: j.model ?? s.status?.models[j.agent]?.model ?? 'default', repo: j.repo, branch: j.branch || '—', base: j.base ?? '—',
    check: j.check ?? undefined, repoCheck: s.repos.find((r) => r.repo === j.repo)?.check, status: j.status, step: j.step, waiting: j.waiting ?? undefined,
    round: j.rounds > 1 ? [j.round, j.rounds] : undefined,
    stages: stagesOf(j),
    ask, summary: j.summary || undefined, reason: j.reason || undefined, canTryAgain: !unknown && stoppedHere(s, j), commits: j.commits ?? undefined, published: j.published ?? undefined, nothingDone: j.nothingDone || undefined,
    created: minutesAgo(j.created), finished: j.finished ? minutesAgo(j.finished) : undefined, conversation: j.conversation ? shownIn(s, j.conversation) : undefined,
    noConversation: !s.conversations.some((c) => c.id === j.conversation),
    archived: !!j.archived, publish: publishOf(j, carriedOn(j, Object.values(s.jobs))),
  };
  const needs = !!now && (!answer || putOff);
  job.page = pageOf(j, s.status?.accounts.find((a) => a.id === j.account), needs);
  return {
    id: j.id, type: job.type, title, brand: agentOf(j.agent).brand, job,
    status: needs ? 'needs' : j.status === 'queued' || j.status === 'running' ? 'running' : 'done',
    fresh: !s.firstJobs.has(j.id) && !s.seen.has(j.id), answer, ...(unknown ? { unknown } : {}),
  };
}
// The repository a conversation's work was last in, by its id in the app: the one picked for
// words sent in it, or its newest job's, whichever came later; with none, or on Home, the newest
// job's anywhere
const newest = (jobs: Job[]) => jobs.reduce<Job | undefined>((a, j) => (!a || j.created > a.created ? j : a), undefined);
export function lastRepo(s: Live, shown?: string) {
  const c = shown && realIn(s, shown);
  const all = Object.values(s.jobs);
  const job = c ? newest(all.filter((j) => j.conversation === c)) : undefined, picked = shown ? s.picked[shown] : undefined;
  return { here: picked && (!job || picked.at > job.created) ? picked.repo : job?.repo, anywhere: newest(all)?.repo };
}
export const tasksOf = (s: Live) => Object.values(s.jobs).sort((a, b) => b.created.localeCompare(a.created)).map((j) => taskOf(s, j));
