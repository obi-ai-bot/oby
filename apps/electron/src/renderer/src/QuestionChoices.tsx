import { useId, useState } from 'react';
import { Check } from 'lucide-react';
import { recommendedIn } from './live';
import type { Ask } from './view';

// Whether an agent's question shows its options as a list (QuestionChoices) rather than a row of
// buttons: any has a description, one is recommended, or several may be picked
export const listsChoices = (ask: Ask) => ask.kind === 'question' && (!!ask.about || !!ask.multi || ask.options.some(recommendedIn));

// An agent's question's options (docs/contracts/permissions.md, BYPASS-10 and 11), as T3 Code's
// pending question shows them (apps/web/src/components/chat/ComposerPendingUserInputPanel.tsx): each
// its label as the agent wrote it, then its description under it; the one it recommends (its label
// ends "(Recommended)") ringed. One pick answers at once. With several allowed, each is ticked
// into the answer's box, as obid takes them ("Unit, Lint"), and Send answers with what the box
// says (the owner may change it). A button holds only its label, and the box only the answer, so
// what goes out is the text the owner's click or keystroke held, which main signs (shared/words.ts).
// choices: the agent's own options (Tell it something is the card's row's).
export default function QuestionChoices({ ask, choices, onPick }: { ask: Ask; choices: string[]; onPick: (option: string, words?: string) => void }) {
  const [picked, setPicked] = useState<string[]>([]);
  const base = useId();
  const [words, setWords] = useState('');
  // ticked in the options' order, whatever order they were ticked in
  const toggle = (o: string) => {
    const next = picked.includes(o) ? picked.filter((x) => x !== o) : [...picked, o];
    setPicked(next);
    setWords(choices.filter((c) => next.includes(c)).join(', '));
  };
  const typed = words.trim();
  return (
    <div className="tc-choices" role="group" aria-label={ask.multi ? 'Pick any' : 'Pick one'}>
      {choices.map((o) => {
        const i = ask.options.indexOf(o), about = ask.about?.[i], on = picked.includes(o);
        return (
          <div key={o} className={`tc-choice${ask.recommended === i ? ' is-recommended' : ''}${on ? ' is-on' : ''}`}>
            <button
              type="button" className="tc-choice-pick"
              {...(ask.multi ? { role: 'checkbox', 'aria-checked': on } : {})}
              aria-describedby={about ? `${base}-${i}` : undefined}
              onClick={() => (ask.multi ? toggle(o) : onPick(o))}
            >
              {o}
            </button>
            {ask.multi && <span className="tc-choice-box" aria-hidden>{on && <Check size={11} strokeWidth={2.4} />}</span>}
            {about && about !== o && <span className="tc-choice-about" id={`${base}-${i}`}>{about}</span>}
          </div>
        );
      })}
      {ask.multi && (
        <form className="tc-options tc-choices-send" onSubmit={(e) => { e.preventDefault(); if (typed) onPick('', typed); }}>
          <input className="set-field" value={words} maxLength={2000} spellCheck={false} aria-label="Your answer" placeholder="Pick any, or say it in your words"
            onChange={(e) => setWords(e.target.value)} />
          <button type="submit" className="tc-option is-primary" disabled={!typed}>Send</button>
        </form>
      )}
    </div>
  );
}
