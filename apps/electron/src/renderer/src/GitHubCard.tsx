import { useState } from 'react';
import { ArrowUpRight, Check } from 'lucide-react';
import { BrandMark, type Brand } from './brands';
import { answer, openOnGitHub } from './tasks';
import type { Ask } from './view';

// Home's GitHub card, lean (docs/contracts/github-work.md, GITHUBWORK-20 to 27), as T3 Code shows a
// pull request and #2454's decision bar shows a decision: one line says what happened (obid's kind
// word, the pull request's or issue's own title, whose and where), then only the checks that failed,
// one line each with their page, then the start button (Fix CI, Review it, Fix it), Later and
// Dismiss, with Open on GitHub a quiet link. obid's sentence says the line again, so only what
// follows its first sentence shows (a repository Oby doesn't work in yet). The pager is the stack's,
// on the line, only when there is more than one card (nav). A click on the card is AskStack's.
export type GitHubEntry = { id: string; title: string; who?: string; brand?: Brand; url: string; talk?: string; ask: Ask; answer?: string };

const SHOWN = 3;  // failed checks past these are one line to the pull request's checks on GitHub
const openPage = (url: string | null | undefined) => { if (url?.startsWith('https://')) window.app?.openExternal(url); };

// "CI failed: demo-app #26" → ["CI failed", "demo-app"]; obid's words with no ": " are the kind alone
export const kindOf = (headline: string): [string, string | undefined] => {
  const at = headline.indexOf(': ');
  return at < 0 ? [headline, undefined] : [headline.slice(0, at), headline.slice(at + 2).replace(/\s*#\d+$/, '') || undefined];
};
// what obid's text adds to the line: all but its first sentence
export const noteOf = (text: string) => text.replace(/^[^.!?]*[.!?](\s+|$)/, '').trim();

export function GitHubAsk({ entry, nav, onAnswer, onOpenTalk }: {
  entry: GitHubEntry;
  nav?: React.ReactNode;
  onAnswer?: (option: string) => void;
  onOpenTalk?: (id: string) => void;
}) {
  const { ask } = entry;
  const [kind, repo] = kindOf(ask.headline);
  const note = ask.text ? noteOf(ask.text) : '';
  const failed = (ask.checks ?? []).filter((r) => r.state === 'FAILURE');
  // one click, one answer: nothing before its words have shown (AskBlock's half second)
  const [shownAt] = useState(() => performance.now());
  const pick = (o: string) => { if (performance.now() - shownAt < 500) return; answer(entry.id, o); onAnswer?.(o); };
  const [start, ...rest] = ask.options;
  return (
    <>
      <header className="gh-line">
        {entry.brand && <BrandMark brand={entry.brand} size={16} mono />}
        <p className="gh-what">
          <span className={`gh-kind${/fail/i.test(kind) ? ' is-failed' : ''}`}>{kind}</span>
          <span className="gh-title" title={entry.title}>{entry.title}</span>
          {(entry.who || repo) && <span className="gh-meta">· {[entry.who, repo].filter(Boolean).join(' · ')}</span>}
        </p>
        {nav}
      </header>
      {note && <p className="gh-note">{note}</p>}
      {failed.length > 0 && (
        <ul className="gh-checks" aria-label="Failed checks">
          {failed.slice(0, SHOWN).map((r, i) => (
            <li key={`${r.name}-${i}`}>
              <span className="status-dot is-failed" aria-hidden />
              <span className="gh-check" title={r.name}><span className="gh-check-name">{r.name}</span> failed</span>
              {r.url?.startsWith('https://') && <button type="button" className="gh-check-details" onClick={() => openPage(r.url)}>Details</button>}
            </li>
          ))}
          {failed.length > SHOWN && (
            <li><button type="button" className="gh-checks-more" onClick={() => openPage(`${entry.url}/checks`)}>{failed.length - SHOWN} more failed</button></li>
          )}
        </ul>
      )}
      {entry.answer ? (
        <p className="tc-answered gh-answered"><Check size={14} strokeWidth={1.8} />{entry.answer}</p>
      ) : (
        <div className="gh-actions">
          <div className="tc-options">
            {start && <button className="tc-option is-primary" onClick={() => pick(start)}>{start}</button>}
            <button className="tc-option is-quiet" onClick={() => pick('Later')}>Later</button>
            {rest.map((o) => <button key={o} className="tc-option is-quiet" onClick={() => pick(o)}>{o}</button>)}
          </div>
          <span className="gh-links">
            {entry.talk && <button type="button" className="tc-link gh-open" onClick={() => onOpenTalk?.(entry.talk!)}>Open conversation</button>}
            <button type="button" className="tc-link gh-open" onClick={() => openOnGitHub(entry.url)}>Open on GitHub<ArrowUpRight size={13} strokeWidth={1.8} /></button>
          </span>
        </div>
      )}
    </>
  );
}
