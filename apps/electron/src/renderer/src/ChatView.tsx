import { copyPlain } from './room';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Check, ChevronRight, Ellipsis, File as FileIcon, X } from 'lucide-react';
import { BrandMark } from './brands';
import { ObyFace, PeerByline } from './ObyFace';
import AskCard from './AskCard';
import ObyFaceLive from './ObyFaceLive';
import { markRead, say, useConvos } from './peers';
import StatusDot, { TaskMark } from './StatusDot';
import { AskBlock, resultLine } from './TaskCard';
import DecisionBar, { type Pending } from './DecisionBar';
import { useScrollThumb } from './useScrollThumb';
import TaskMenu from './TaskMenu';
import TaskLog from './TaskLog';
import { ChangesPill } from './ChangesPanel';
import { replyTarget, type Box } from './replyTarget';
import Composer, { typingIn } from './Composer';
import Formatted from './Formatted';
import Said from './Said';
import { CopyButton, Photo, ReplyPlace, type Place } from './Reply';
import { convoTitle, threadOf } from './mock';
import { ENDED, agentOf, type Message } from './view';
import { asksWaiting, markSeen, stop, tell, tellPlaceholder, useTasks, type Task } from './tasks';
import { cancelQueued, obys, ofObid, seeChat, stopChat, tellChat, useChats, type Chat } from './chats';
import CantRunLine from './CantRunLine';
import Refusals from './Refusals';
import RefusalFixes from './RefusalFixes';
import ReplySelection from './ReplySelection';
import { historyOf } from './recall';
import SetupCard from './SetupCard';
import OfferCard from './OfferCard';
import { WELCOME, typedFirst } from './onboarding';
import { openConversation, talkReady, useLimitAsks } from './live';
import { onObid } from './lean';
import { box as draftBox } from './drafts';
import { useHelpers } from './helpers';
import FindBar from './FindBar';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;
// a quiet this long between two messages (minutes) starts a new run, with its own time
const GAP = 60;
// a message's time: the clock today, else the day and the clock
export const clock = (ago: number) => {
  const d = new Date(Date.now() - ago * 60_000);
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  return d.toDateString() === new Date().toDateString() ? time : `${d.toLocaleDateString([], { weekday: 'short' })} ${time}`;
};

// One entry in the thread: a message (the decision a task needs from you waits over the message box,
// DecisionBar, never in the thread: LIVETASK-24)
type Entry = Message;

// A conversation. kind 'oby': one with Oby (chats.ts), opened from
// Conversations or started on Home: what you asked, Oby's answers with the
// tasks they started as cards under them; the message box carries it on. kind
// 'task': a task's own thread, for a task that came in from elsewhere (GitHub,
// Slack…): the message it came in on, Oby's replies, what you've told the agent
// since, and the agent's live line while it works; the message box tells the
// agent. kind 'peer': one with someone else's Oby (or a few) in the Obysphere
// (peers.ts): each Oby's words under its owner's picture and name. Whatever is
// happening now comes last: an Oby at work, or the agent's line. It lies over
// what's open; back (in the title bar, or Esc) returns to it. A task's card is
// its progress only; what it did shows under it, in the conversation (TaskLog),
// and every way to a task opens here at its card (flash, LIVETASK-28). The
// message box at the foot talks to Oby, or to one card's agent once its menu's
// "Tell Claude Code…" has put that task's chip on it (replyTarget, LIVETASK-17).
export default function ChatView({ id, kind = 'task', leaving, covered = false, flash, onFlashed, onBack, onGone, onOpenMember }: {
  id: string;
  kind?: 'task' | 'oby' | 'peer';
  leaving: boolean;
  covered?: boolean;  // something is open over it
  flash?: { task: string; n: number };  // opened at a task: its card scrolled into view and lit a moment, once for each n
  onFlashed?: () => void;  // and done
  onBack: () => void;
  onGone: () => void;
  onOpenMember?: (owner: string) => void;  // a question's card: that person's thread
}) {
  const tasks = useTasks();
  const chats = useChats();
  const convos = useConvos();
  // deleted, it's gone while this sinks away: keep showing the last of it
  const keptTask = useRef<Task>(null), keptChat = useRef<Chat>(null);
  const task = kind === 'task' ? tasks.find((t) => t.id === id) ?? keptTask.current! : null;
  // where its replies' paths are read (Reply.tsx, CHATFIRST-39): a task's thread, its worktree; a
  // conversation with Oby, its folders; other people's Obys' words, nowhere
  const place = useMemo<Place | null>(() => (kind === 'task' ? { place: 'task', id } : kind === 'oby' ? { place: 'conversation', id } : null), [kind, id]);
  const talk = kind === 'oby' ? chats.find((c) => c.id === id) ?? keptChat.current! : null;
  const convo = kind === 'peer' ? convos.find((c) => c.id === id)! : null;
  if (task) keptTask.current = task;
  if (talk) keptChat.current = talk;
  // an agent's conversation (helpers.ts): its Oby, in its colour, speaks; the box names it
  const helpers = useHelpers();
  const helper = talk?.helper ? helpers.find((h) => h.id === talk.helper) : undefined;
  const limitsOf = useLimitAsks(talk?.id, task?.id), limits = talk || task ? limitsOf : [];
  const name = task ? convoTitle(task.job) : talk ? talk.title : convo!.title;
  const j = task?.job;
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLElement>();
  // a card's menu (right-click), at the pointer
  const [menu, setMenu] = useState<{ id: string; at: DOMRect } | null>(null);
  const menuTask = menu && tasks.find((t) => t.id === menu.id);
  // the task the message box talks to, from its card's menu (Tell Claude Code…): its chip on the box
  const [to, setTo] = useState<string | null>(null);
  const toTask = to ? tasks.find((t) => t.id === to) : undefined;
  // the card opened at, lit a moment; each opening at a task brings back what Later put off here
  const [lit, setLit] = useState<string | null>(null);
  const [openings, setOpenings] = useState(0);
  const agent = j ? agentOf(j.agent).name : 'Oby';
  // a task's: what came before, then what's been said since
  const thread: Entry[] = task ? [
    ...(j ? threadOf(j) : []),
    ...(task.said ?? []),
  ] : talk ? talk.said : convo!.messages.map((m): Message => (m.who === 'you'
    ? { who: 'you', text: m.text, ago: m.ago, jobs: [...(m.task ? [m.task] : []), ...(m.jobs ?? [])], files: m.files, obys: m.obys }
    // your Oby (answering another Oby for you) as in your own conversations; another
    // person's Oby from the same side, under its owner's name (by). Only you, the
    // person, get a bubble
    : { who: 'oby', by: m.who === 'oby' ? undefined : m.who, text: m.text, ago: m.ago, jobs: m.task ? [m.task] : undefined, streaming: m.streaming }));
  // the owner's earlier words here, for ↑ in an empty box (COMPOSER-20), and where a reply's picked
  // words go in it (COMPOSER-23)
  const history = useMemo(() => historyOf(thread.filter((m) => m.who === 'you').map((m) => m.text)), [thread]);
  const quote = useRef<((text: string) => void) | null>(null);
  // an Oby at work on an answer, and what it's on (someone else's: writing)
  const typing = !!(task ?? talk ?? convo)!.typing, step = talk ? talk.step : convo?.typing ? 'Writing' : undefined;
  // who's writing, in a conversation with other people's Obys: the first in it
  const writer = convo?.with[0];
  // Whose run a message is in: another Oby answering what Oby asked is part of
  // Oby's turn (quoted in it), not a turn of its own; with other people's Obys,
  // each Oby's words are a run of their own
  const runOf = (e: Entry) => (e.who === 'peer' ? 'oby' : e.who === 'oby' && (e as Message).by ? `oby:${(e as Message).by}` : e.who);
  // the first message of the run at the end, if it's Oby's: its face is live
  // while Oby still works on that turn
  let head = -1;
  for (let k = thread.length - 1; k >= 0 && runOf(thread[k]) === 'oby'; k--) head = k;
  const working = typing && head >= 0 && !convo;
  // Oby's words coming in, in that run: its face talks (a speech balloon)
  const talking = head >= 0 ? (thread.slice(head).find((m) => (m as Message).streaming) as Message | undefined) : undefined;

  // opened, it's seen; it opens at the latest message, and anything new scrolls into view
  useEffect(() => { if (kind === 'task') markSeen(id); if (kind === 'peer') markRead(id); if (kind === 'oby' && ofObid(id)) void openConversation(id); }, [id, kind]);
  // in sight, a conversation is seen: obid's unread goes, and a turn that comes while it shows is
  // seen too (CHATSTATE-6)
  useEffect(() => { if (talk && !covered && !leaving) seeChat(talk.id); }, [talk?.id, talk?.unread, talk?.seen, covered, leaving]);
  const first = useRef(true);
  useLayoutEffect(() => {
    const l = list.current;
    if (!l) return;
    l.scrollTo({ top: l.scrollHeight, behavior: first.current ? 'instant' : 'smooth' });
    first.current = false;
  // (the last, as its words come in, then the card or task it ends with)
  }, [thread.length, typing, step, limits.length, (thread.at(-1) as Message | undefined)?.text?.length, (thread.at(-1) as Message | undefined)?.streaming, (thread.at(-1) as Message | undefined)?.jobs?.length]);

  // a live card growing at the end of the thread stays in view, while you're there
  const atEnd = useRef(true);
  useEffect(() => {
    const l = list.current, column = l?.firstElementChild;
    if (!l || !column) return;
    const ro = new ResizeObserver(() => { if (atEnd.current) l.scrollTop = l.scrollHeight; });
    ro.observe(column);
    return () => ro.disconnect();
  }, [list]);

  // opened at a task (a notification, Waiting for you, its row): its card in view, lit a moment, once it's
  // here (a conversation's turns can come in after it opens)
  const flashed = useRef(0);
  useEffect(() => {
    if (!flash || flashed.current === flash.n) return;
    const el = list.current?.querySelector<HTMLElement>(`[data-task="${CSS.escape(flash.task)}"]`);
    if (!el) return;
    flashed.current = flash.n;
    atEnd.current = false;
    el.scrollIntoView({ block: 'center' });
    setLit(flash.task);
    setOpenings((n) => n + 1);
    onFlashed?.();
  });
  useEffect(() => {
    if (!lit) return;
    const t = setTimeout(() => setLit(null), 1600);
    return () => clearTimeout(t);
  }, [lit, flash?.n]);

  // a timer, not animationend: animations don't run while the window is hidden
  useEffect(() => {
    if (!leaving) return;
    const t = setTimeout(onGone, LEAVE_MS);
    return () => clearTimeout(t);
  }, [leaving, onGone]);

  useEffect(() => {
    // with a task's page open over it, Esc is the page's
    if (covered) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !typingIn(e)) onBack(); };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [onBack, covered]);

  // the decisions waiting over the message box: this task's own, else those of the tasks this
  // conversation started (each named by its task: carded here, or its job's conversation, its card
  // not placed yet), then an account's limit stopping its work
  const carded = [...new Set([...thread.flatMap((m) => m.jobs ?? []), ...(talk ? tasks.filter((t) => t.job.conversation === talk.id).map((t) => t.id) : [])])];
  const inThread = new Set(thread.flatMap((m) => [(m as Message).confirm?.id, (m as Message).offer?.id].filter((x): x is string => !!x)));
  const pending: Pending[] = [
    ...(task ? (task.status === 'needs' && j!.ask ? [{ key: task.id, task }] : [])
      : carded.flatMap((jid) => {
        const t = tasks.find((x) => x.id === jid);
        return t && t.status === 'needs' && t.job.ask ? [{ key: t.id, task: t, from: t.title }] : [];
      })),
    ...limits.map((a) => ({ key: a.id, task: { id: a.id, job: { ask: a.ask! } } })),
    // and what this conversation asks with no task to show it on (a failure, a GitHub offer made
    // here), unless its thread shows it already (a Confirm, an offer): never another conversation's
    // (CHATFIRST-8)
    ...(talk ? asksWaiting().filter((a) => a.talk === talk.id && a.ask && !inThread.has(a.id))
      .map((a) => ({ key: a.id, task: { id: a.id, job: { ask: a.ask! } }, from: a.title })) : []),
  ];

  // where the message box's words go: Oby, a task's own agent, or another Oby (replyTarget)
  const foot: Box = toTask ? { card: toTask.id } : { foot: task ? { kind: 'task', id: task.id } : talk ? { kind: 'oby', id: talk.id } : { kind: 'peer', id: convo!.id } };
  const sendFoot = (text: string, att: Parameters<typeof tellChat>[2]) => {
    const at = replyTarget(foot);
    // to a task's chip: once obid took the words the box talks to Oby again; refused, the chip and the
    // words stay (ATTACH-20)
    if (toTask && 'job' in at) {
      const sent = tell(at.job, text, att);
      if (!sent) { setTo(null); return; }
      return sent.then((took) => { if (took) setTo(null); return took; });
    }
    return 'job' in at ? tell(at.job, text, att) : 'oby' in at ? (at.oby === WELCOME && typedFirst(text)) || tellChat(at.oby, text, att) : say(at.peer, text, att);
  };
  // the box's words for a task: its agent's, or a follow-up once it has ended
  const toAgent = toTask && agentOf(toTask.job.agent).name;
  const tellTo = (tid: string) => {
    setTo(tid);
    requestAnimationFrame(() => document.querySelector<HTMLElement>('.chat-layer:not(.is-covered):not(.is-leaving) > .composer [contenteditable="true"]')?.focus());
  };

  return (
    <ReplyPlace.Provider value={place}>
    <div className={`chat-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <div className="chat-scroll">
        {/* ⌘F: find in this conversation (SHORTCUTS-3) */}
        <FindBar root={list} shown={thread.length} off={covered} />
        <section ref={list} className="chat" aria-label={task ? `Conversation about ${task.title}` : name} onScroll={(e) => { onScroll(); const el = e.currentTarget; atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80; }}>
          <div className="chat-column">
            <div role="list" onCopy={(e) => copyPlain(e)} className="chat-thread rise" style={{ '--i': 1 } as React.CSSProperties}>
              {[...thread.map((m, i) => {
                const prev = thread[i - 1], next = thread[i + 1];
                // A run: the same side, one after another, 4px apart (a long quiet
                // starts a new one). Oby's face by the first of its run; the time
                // under the end of the last, at its right edge (Oby's at its left)
                const cont = !!prev && runOf(prev) === runOf(m) && prev.ago - m.ago < GAP;
                const last = !next || runOf(next) !== runOf(m) || m.ago - next.ago >= GAP;
                // with other people's Obys, yours speaks for you: on your side (the
                // right), still as an Oby (its face, no bubble)
                const mine = !!convo && m.who === 'oby' && !(m as Message).by;
                const side = m.who === 'you' ? 'you' : m.who === 'oby' || m.who === 'peer' ? 'oby' : 'source';
                const parts = [
                  // what Oby did on the way, folded
                  ...('steps' in m && m.steps?.length ? [<Steps key="steps" steps={m.steps} looked={m.looked ?? m.steps.length} asked={m.asked} />] : []),
                  // what went with yours: photos and files (Obys are @mentions in the words; tasks: cards, below)
                  ...('files' in m && m.files?.length ? [<Attached key="att" files={m.files} />] : []),
                  m.who === 'peer' ? (
                    // another Oby's answer to what Oby asked: quoted in Oby's turn,
                    // under its owner's picture and name
                    <blockquote key="text" className="msg-quote"><PeerByline name={(m as Message).by!} /><Said text={m.text} /></blockquote>
                  ) : side !== 'oby' ? (
                    // yours (and the ask it came in on), set as typed (none: only attachments)
                    m.text ? <p key="text" className="msg-bubble"><Formatted text={m.text} /></p> : null
                  ) : m.text ? (
                    // Oby's as it said it (its steps alone, folded where another
                    // Oby's answer came in, have no words)
                    // with its Copy (CHATFIRST-37), once all its words are in
                    <div key="text" className="msg-bubble"><Said text={m.text} streaming={(m as Message).streaming} />{!(m as Message).streaming && <CopyButton text={m.text} className="msg-copy" />}</div>
                  ) : null,
                  // a task it couldn't start on the account named: Install or Sign in (ACCOUNTS-37)
                  ...((m as Message).fixes?.length && !(m as Message).streaming ? [<RefusalFixes key="fixes" fixes={(m as Message).fixes!} />] : []),
                  // setting up (Oby's first conversation): where you act, once its words are in
                  ...((m as Message).setup && !(m as Message).streaming ? [<SetupCard key="setup" step={(m as Message).setup!} />] : []),
                  // work Oby offers to start: yours to start with a tap
                  ...((m as Message).offer ? [<OfferCard key="offer" offer={(m as Message).offer!} />] : []),
                  // a question to another Oby: one card, updating in place
                  ...((m as Message).askCard ? [<AskCard key="askcard" card={(m as Message).askCard!} onOpen={onOpenMember} />] : []),
                  // a Confirm Oby's answer made: one click here or on Home, the owner's signed Confirm
                  ...((m as Message).confirm ? [<div key={`confirm-${(m as Message).confirm!.id}`} className="msg-ask"><AskBlock task={{ id: (m as Message).confirm!.id, job: { ask: (m as Message).confirm!.ask } }} inChat /></div>] : []),
                  // what this message started
                  ...('jobs' in m ? m.jobs ?? [] : []).map((jid) => {
                    const t = tasks.find((x) => x.id === jid);
                    return t && (
                      <TaskEntry
                        key={jid}
                        task={t}
                        lit={lit === jid}
                        onMenu={(at) => setMenu({ id: jid, at })}
                      />
                    );
                  }),
                ].filter(Boolean);
                const end = parts.pop();
                return (
                  <div role="listitem" key={i} className={`msg is-${side}${mine ? ' is-mine' : ''}${cont ? ' is-cont' : ''}${m.ago === 0 ? ' is-new' : ''}${(m as Message).streaming ? ' is-streaming' : ''}`}>
                    {side === 'oby' && !cont && ((m as Message).by && m.who === 'oby'
                      ? <PeerByline name={(m as Message).by!} />
                      // yours among other people's Obys: named, as theirs are
                      : mine ? <span className="peer-by"><ObyFace size={22} /><span>Your Oby</span></span>
                      : <span className="msg-face"><Face live={working && i === head && !covered} talk={i === head && !covered ? talking?.text.length : undefined} color={helper?.color} /></span>)}
                    {parts}
                    {last && !(m as Message).streaming && !(typing && i === thread.length - 1 && side === 'oby') ? (
                      <div className={`msg-end${end?.type === TaskEntry || end?.type === SetupCard || end?.type === OfferCard || end?.type === AskCard || (m as Message).confirm ? ' is-wide' : ''}`}>
                        {end}
                        <time className="msg-time">{clock(m.ago)}</time>
                      </div>
                    ) : end}
                  </div>
                );
              }),
              // Oby at work: Oby itself, melting into bouncing dots, and under it
              // what it's on now. Keyed as its words will be, so when they land it's
              // the same row and the same live Oby, which then comes back together
              // (carrying on a turn already under way, its face is up there already).
              // Someone else's Oby: its name, unless its words are just above
              typing && (step || !working) && (() => {
                const cont = writer ? runOf(thread.at(-1) ?? { who: 'you', text: '', ago: 0 }) === `oby:${writer}` : working;
                return (
                  <div role="listitem" key={thread.length} className={`msg is-oby is-new${cont ? ' is-cont' : ''}`}>
                    {!cont && (writer ? <PeerByline name={writer} /> : <span className="msg-face"><Face live={!covered} color={helper?.color} /></span>)}
                    {step && <p className="msg-step" role="status">{step}</p>}
                  </div>
                );
              })()]}

              {/* a task's agent at work: its live line, last */}
              {task && j && task.status === 'running' && !typing && (
                <div role="listitem" className={`chat-live${j.status === 'queued' ? '' : ' is-working'}`}>
                  <StatusDot task={task} live />
                  <span className="tc-step">{agent} · {j.step}</span>
                  {j.round && <span className="tc-round">Round {j.round[0]} of {j.round[1]}</span>}
                </div>
              )}
            </div>
          </div>
        </section>
        <div {...thumbProps} />
      </div>

      {/* Oby waiting on you (a question back, not a task's): held over the message
          box until you answer, with a pick or in your own words below */}
      {talk?.waiting && (
        <div className="chat-ask" role="group" aria-label="Oby is asking">
          <div className="chat-ask-card">
            <span className="tc-ask-kind">Question</span>
            <p className="tc-ask-head">{talk.waiting.question}</p>
            <div className="tc-options">
              {talk.waiting.options.map((o) => (
                <button key={o} className="tc-option" onClick={() => tellChat(talk.id, o)}>{o}</button>
              ))}
            </div>
            <p className="chat-ask-hint">Or answer in your own words below</p>
          </div>
        </div>
      )}
      {/* what this conversation's tasks, or this task, wait on the owner for, and an account's limit
          stopping its work: one line each, over the message box (LIVETASK-24 to 27) */}
      <DecisionBar key={openings} pending={pending} />
      {/* what obid refused of a click here, until dismissed; and the account it goes to, when that
          can't run (CHATSTATE-1 to 4) */}
      <Refusals where={talk ? [talk.id, ...tasks.filter((t) => t.job.conversation === talk.id).map((t) => t.id)] : task ? [task.id] : []} />
      {(talk && !talk.local && !talk.helper) || task ? <CantRunLine chat={talk ?? undefined} task={task ?? undefined} /> : null}
      {toTask ? <>
        {/* talking to one task's agent: its chip over the box, × back to Oby (LIVETASK-30) */}
        <div className="composer-to" role="status">
          <span className="composer-to-chip"><BrandMark brand={agentOf(toTask.job.agent).brand} size={12} />To {toAgent} · <b>{toTask.title}</b>
            <button type="button" aria-label={`Back to Oby`} onClick={() => setTo(null)}><X size={12} strokeWidth={2} /></button>
          </span>
        </div>
        <Composer
          key={`to:${toTask.id}`}
          style={{ '--i': 2 } as React.CSSProperties}
          placeholder={tellPlaceholder(toTask, toAgent!)}
          label={`Tell the agent about ${toTask.title}`}
          onSend={sendFoot}
          // `/` lists the task's agent's skills, which a picked one goes to (COMPOSER-25, 27)
          skills={{ job: toTask.id }}
          draft={draftBox.task(toTask.id)}
          history={history}
          quote={quote}
          onStop={toTask.status === 'running' && !toTask.unknown ? () => stop(toTask.id) : undefined}
        />
      </> : (
        <Composer
          style={{ '--i': 2 } as React.CSSProperties}
          placeholder={convo ? `Message ${convo.with.length > 1 ? 'the group' : `${writer}’s Oby`}…` : helper ? `Message ${helper.name}…` : !task ? 'Reply to Oby…' : tellPlaceholder(task, agent)}
          label={convo ? `Message ${convo.title}` : helper ? `Message ${helper.name}` : task ? `Tell the agent about ${task.title}` : 'Reply to Oby'}
          onSend={sendFoot}
          repos={!!talk && !talk.local}
          conversation={talk?.id}
          talk={talk && !talk.local && (!onObid || talkReady(talk.id)) ? talk : undefined}
          // a task's own thread: its agent's skills (COMPOSER-25); a conversation's are its talk's
          skills={task ? { job: task.id } : undefined}
          draft={task ? draftBox.task(task.id) : talk ? draftBox.conversation(talk.id) : draftBox.peer(convo!.id)}
          onStop={task ? (task.status === 'running' && !task.unknown ? () => stop(task.id) : undefined) : talk && typing && ofObid(talk.id) && talkReady(talk.id) ? () => stopChat(talk.id) : undefined}
          history={history}
          quote={quote}
          // words sent while Oby answers here, waiting their turn (COMPOSER-8 to 10)
          queued={talk?.queued}
          onCancelQueued={talk ? (q) => cancelQueued(talk.id, q.id) : undefined}
        />
      )}
      {/* words picked out of a reply: Reply about this, quoted into the box (COMPOSER-23) */}
      <ReplySelection root={list} onQuote={(text) => quote.current?.(text)} />
      {/* a card's menu: tell its agent something, Stop or Archive, and Delete */}
      {/* (from its ⋯, at the card's right edge: it opens leftward; from a right-click, at the pointer) */}
      {menu && menuTask && <TaskMenu task={menuTask} at={menu.at} align={menu.at.width > 0 ? 'right' : 'left'} onClose={() => setMenu(null)} onTell={() => tellTo(menu.id)} />}
    </div>
    </ReplyPlace.Provider>
  );
}

// The steps Oby took, as one line over its words ("Looked at 2 things, asked
// Wellington's Oby"); a click opens the list of them, each with a check
function Steps({ steps, looked, asked = [] }: { steps: string[]; looked: number; asked?: string[] }) {
  const [open, setOpen] = useState(false);
  const label = [
    looked > 0 && `Looked at ${looked} thing${looked === 1 ? '' : 's'}`,
    asked.length > 0 && `asked ${obys(asked)}`,
  ].filter(Boolean).join(', ');
  return (
    <div className={`msg-steps${open ? ' is-open' : ''}`}>
      <button type="button" className="msg-steps-head" aria-expanded={open} onClick={() => setOpen(!open)}>
        {label[0].toUpperCase() + label.slice(1)}
        <ChevronRight size={14} strokeWidth={1.8} />
      </button>
      {open && (
        <ul>
          {steps.map((st, i) => <li key={i}><Check size={12} strokeWidth={2} />{st}</li>)}
        </ul>
      )}
    </div>
  );
}

// What went with a message of yours: photos as pictures, files as small tags,
// on your side over your words (Obys brought in are named in the words: @Wellington)
function Attached({ files = [] }: { files?: { name: string; preview?: string }[] }) {
  return (
    <div className="msg-attached">
      {files.map((f, i) => (f.preview
        // a click shows it full size (CHATFIRST-41)
        ? <Photo key={i} className="msg-photo" src={f.preview} alt={f.name} />
        : <span key={i} className="msg-file"><FileIcon size={14} strokeWidth={1.8} /><span>{f.name}</span></span>))}
    </div>
  );
}

// Oby's face over what it says. live (while it writes): the face melts into
// bouncing dots; once its words are in, they run back together into the face
// live: at work, and in sight (under a page opened over it, it rests: the power budget).
// talk: its words coming in (their length), while the face has its speech balloon
// color: an agent's, in its own conversation
const Face = ({ live = false, talk, color }: { live?: boolean; talk?: number; color?: string }) => <ObyFaceLive live={live} talk={talk} color={color} />;

// A task started in the conversation, as a card under Oby's message (Sangam's screen 03): its progress
// only (LIVETASK-29): its agent, its title, how it stands, who has it and where ("Running · Claude Code ·
// oby"), its stages as a bar, and its status mark at the right (as on its nested row in the side panel).
// What it did shows under it, never in it (TaskLog). A click opens nothing: every way to a task opens
// this conversation at this card (LIVETASK-28). Its ⋯, or a right-click, opens its menu (onMenu): tell
// its agent something, Stop. Waiting on the owner, it stays its head: "Needs you", the stage it waits on
// orange; the decision waits over the message box (DecisionBar, LIVETASK-9, LIVETASK-25). A status this
// build doesn't know says no word and shows StatusDot's neutral mark (docs/contracts/wire.md, WIRE-3).
export function ThreadTask({ task, onMenu }: { task: Task; onMenu?: (at: DOMRect) => void }) {
  return (
    <div
      className="msg-task"
      role="group"
      aria-label={task.title}
      onContextMenu={onMenu && ((e) => { e.preventDefault(); onMenu(new DOMRect(e.clientX, e.clientY, 0, 0)); })}
    >
      <TaskHead task={task} />
      {onMenu && (
        <button type="button" className="icon-button msg-task-more" aria-label={`More for ${task.title}`} aria-haspopup="menu" onClick={(e) => onMenu(e.currentTarget.getBoundingClientRect())}>
          <Ellipsis size={16} strokeWidth={1.8} />
        </button>
      )}
    </div>
  );
}

// A task in the conversation: its card, its Changes (CHANGES-4), and what it did under it, outside the card (TaskLog); data-task is
// where opening the conversation at the task scrolls to (LIVETASK-28)
export function TaskEntry({ task, lit = false, onMenu }: { task: Task; lit?: boolean; onMenu?: (at: DOMRect) => void }) {
  return (
    <div className={`msg-task-work${task.status === 'running' || task.status === 'needs' ? ' is-live' : ''}${lit ? ' is-lit' : ''}`} data-task={task.id}>
      <ThreadTask task={task} onMenu={onMenu} />
      {/* what it changed, in the side panel beside the conversation (CHANGES-4) */}
      <ChangesPill task={task} />
      <TaskLog task={task} />
    </div>
  );
}

// A task's head, as its card shows it (screen 03)
export function TaskHead({ task }: { task: Task }) {
  const j = task.job;
  const repo = j.repo.includes('/') ? j.repo.split('/')[1] : j.repo || null;
  const word = task.unknown ? null : task.status === 'running' ? (j.status === 'queued' ? 'Queued' : 'Running') : task.status === 'needs' ? 'Needs you' : j.nothingDone ? 'Nothing done' : ENDED[j.status] ?? 'Done';
  return <>
    <BrandMark brand={agentOf(j.agent).brand} size={16} />
    <span className="msg-task-text">
      <span className="msg-task-title">
        {task.who && <span className="tc-who">{task.who}</span>}
        {task.title}
      </span>
      <span className="msg-task-meta">
        {word}{word && ' · '}{agentOf(j.agent).name}{j.on && <> · on {j.on}</>}{repo && <> · {repo}</>}
      </span>
      {/* stopped: why, in obid's words, on one line */}
      {task.status === 'done' && ENDED[j.status] && j.reason && <span className="msg-task-meta">{j.reason.split('\n')[0]}</span>}
      {task.status === 'done' && !task.unknown && resultLine(task)}
      {j.stages.length > 0 && task.status !== 'done' && <Stages task={task} />}
    </span>
    {/* stopped or failed isn't the check a finished one gets (Sangam's board "Stuck and failed") */}
    <TaskMark task={task} />
  </>;
}

// Its stages as a bar of six (screen 03): those done filled, the one it's on pulsing, or orange while
// it waits on the owner, a failed one red
function Stages({ task }: { task: Task }) {
  const stages = task.job.stages;
  const at = stages.findIndex((st) => st.state === 'current');
  const needs = task.status === 'needs';
  const label = at >= 0 ? `Stage ${at + 1} of ${stages.length}: ${stages[at].title}${needs ? ', needs you' : ''}` : `${stages.filter((st) => st.state === 'done').length} of ${stages.length} stages done`;
  return (
    <span className="tc-progress msg-task-stages" role="img" aria-label={label}>
      {stages.map((st, i) => <span key={i} className={st.state === 'done' ? 'is-done' : st.state === 'failed' ? 'is-failed' : st.state === 'current' ? (needs ? 'is-needs' : 'is-current') : undefined} />)}
    </span>
  );
}
