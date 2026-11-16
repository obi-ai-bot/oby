import { useEffect, useState } from 'react';
import { BrandMark } from './brands';
import { useScrollThumb } from './useScrollThumb';
import { agentOf, ago } from './view';
import { allowWaiting, answerWaiting, approvingTools, homeAsk, itemAsk, mergeWaiting, projectOf, questionOf, shownId, useLive, waitingOf } from './live';
import { QuestionBlock } from './QuestionCard';
import { AskBlock } from './TaskCard';
import { ALLOW_ONCE } from '../../shared/words';
import type { InboxItem } from './obid';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;
const name = (repo: string) => repo.split('/').pop() || repo;
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
const COUNTED = [['merge', (n: number) => `${n} to merge`], ['permission', (n: number) => plural(n, 'permission')], ['question', (n: number) => plural(n, 'question')]] as const;
const kindOf = (i: InboxItem) => (i.kind === 'merge' || i.kind === 'permission' ? i.kind : 'question');

// Waiting for you (Sangam's screen 17, docs/contracts/home.md WAITING-1 to 3): everything across the
// profile's projects that needs the owner's word, one list, longest waiting first: merges with
// Oby's summary (Open, Merge), permissions (Deny, Allow once) and questions (Answer, which opens
// their conversation). All projects narrows to one. A view over obid's inbox (live.ts, waitingOf).
export default function WaitingView({ leaving, covered = false, onClose, onGone, onOpenTask, onOpenTalk }: {
  leaving: boolean;
  covered?: boolean;  // a task's page or a conversation is open over it
  onClose: () => void;
  onGone: () => void;
  onOpenTask: (id: string) => void;
  onOpenTalk: (id: string) => void;
}) {
  const s = useLive();
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLElement>();
  const [project, setProject] = useState('');
  const all = waitingOf(s);
  const projects = [...new Set(all.map((i) => projectOf(s, i)).filter(Boolean))].sort();
  const shown = all.filter((i) => !project || projectOf(s, i) === project);
  const counts = COUNTED.map(([k, say]) => {
    const n = shown.filter((i) => kindOf(i) === k).length;
    return n ? say(n) : '';
  }).filter(Boolean).join(' · ');

  // a timer, not animationend: animations don't run while the window is hidden
  useEffect(() => {
    if (!leaving) return;
    const t = setTimeout(onGone, LEAVE_MS);
    return () => clearTimeout(t);
  }, [leaving, onGone]);

  useEffect(() => {
    if (covered) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [onClose, covered]);

  return (
    <div className={`group-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <section ref={list} className="group" aria-label="Waiting for you" onScroll={onScroll}>
        <div className="group-column">
          <header className="waiting-head rise" style={{ '--i': 0 } as React.CSSProperties}>
            <h1 className="group-title">Waiting for you</h1>
            <select className="waiting-chip" aria-label="Project" value={project} onChange={(e) => setProject(e.target.value)}>
              <option value="">All projects</option>
              {projects.map((p) => <option key={p} value={p}>{name(p)}</option>)}
            </select>
            {counts && <span className="waiting-counts">{counts}</span>}
          </header>
          <ul className="waiting-list rise" style={{ '--i': 1 } as React.CSSProperties}>
            {shown.map((i) => <WaitingRow key={i.id} item={i} onOpenTask={onOpenTask} onOpenTalk={onOpenTalk} />)}
          </ul>
        </div>
      </section>
      <div {...thumbProps} />
    </div>
  );
}

function WaitingRow({ item: i, onOpenTask, onOpenTalk }: { item: InboxItem; onOpenTask: (id: string) => void; onOpenTalk: (id: string) => void }) {
  const s = useLive();
  const j = s.jobs[i.job];
  const ask = itemAsk(i);
  const kind = kindOf(i);
  const project = projectOf(s, i);
  const talk = i.conversation ?? j?.conversation;
  // a merge's: Oby's summary of the work, its check as it ended, each tool that approved its pushed
  // head (else the Review stage's words: "Waiting for a second review"), and the lines it changes
  const stage = (id: string) => j?.pipeline?.stages.find((st) => st.id === id);
  const check = stage('check')?.state === 'done' ? 'Check passed' : stage('check')?.state === 'failed' ? 'Check failed' : undefined;
  const tools = j ? approvingTools(j) : [];
  const review = tools.length ? undefined : stage('review')?.detail;
  const summary = kind === 'merge' ? j?.summary : undefined;
  const diff = kind === 'merge' ? j?.diff : undefined;
  // what it waits on with no task's permission and nowhere to open: its card, as Home's stack shows
  // it (another Oby's question quoted, with Answer for the owner's own words; anything else obid's
  // answers and Later); a merge card with no Merge: its own answers, each signed by its button's label
  const permits = kind === 'permission' && !!j?.permission;
  const opens = kind !== 'merge' && !permits && (!!talk || !!i.job);
  const merges = kind === 'merge' && i.options.includes('Merge');
  const card = kind !== 'merge' && !permits && !opens;
  const question = card ? questionOf(i) : undefined;
  const answers = kind === 'merge' && !merges ? i.options : [];
  const meta = [project ? name(project) : 'No project', ago((Date.now() - Date.parse(i.created)) / 60_000)].join(' · ');
  return (
    <li className="waiting-row" aria-label={j?.title ?? i.title}>
      {j && <BrandMark brand={agentOf(j.agent).brand} size={16} mono />}
      <div className="waiting-text">
        <h2 className="waiting-title">{j?.title ?? i.title}</h2>
        <p className="waiting-meta">{meta}</p>
        {card && (question ? <QuestionBlock question={question} /> : <AskBlock task={{ id: i.id, job: { ask: homeAsk(i) } }} />)}
        {!card && <>
        <p className="waiting-about">{ask.headline}{ask.text ? ` ${ask.text}` : ''}</p>
        {summary && <p className="waiting-summary">{summary}</p>}
        {kind === 'permission' && j?.permission?.detail && <code className="waiting-detail">{j.permission.detail}</code>}
        <p className="waiting-chips">
          <span className={`waiting-kind is-${kind}`}>{ask.label ?? (kind === 'permission' ? 'Permission' : kind === 'merge' ? 'Merge' : 'Question')}</span>
          {check && <span className={`waiting-tag${check === 'Check failed' ? ' is-failed' : ''}`}>{check}</span>}
          {tools.map((t) => (
            <span key={t} className="waiting-tag" aria-label={`Approved by ${agentOf(t).name}`}><BrandMark brand={agentOf(t).brand} size={12} mono />Approved</span>
          ))}
          {review && <span className="waiting-tag">{review}</span>}
          {diff && <span className="waiting-tag waiting-diff"><span className="is-added">+{diff.added}</span> <span className="is-removed">−{diff.removed}</span> · {plural(diff.files, 'file')}</span>}
        </p>
        </>}
      </div>
      <div className="waiting-actions">
        {kind === 'merge' && i.job && <button className="tc-option" onClick={() => onOpenTask(i.job)}>Open</button>}
        {merges && <button className="tc-option is-primary" onClick={() => void mergeWaiting(i)}>Merge</button>}
        {permits && <>
          <button className="tc-option" onClick={() => void allowWaiting(i, 'Deny')}>Deny</button>
          <button className="tc-option is-primary" onClick={() => void allowWaiting(i, ALLOW_ONCE)}>{ALLOW_ONCE}</button>
        </>}
        {opens && <button className="tc-option" onClick={() => (talk ? onOpenTalk(shownId(talk)) : onOpenTask(i.job))}>Answer</button>}
        {answers.map((o) => <button key={o} className="tc-option" onClick={() => void answerWaiting(i, o)}>{o}</button>)}
      </div>
    </li>
  );
}
