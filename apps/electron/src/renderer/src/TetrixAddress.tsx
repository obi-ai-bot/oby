import { useId, useState } from 'react';
import { connectTetrix } from './onboarding';
import { TETRIX_URL_PLACEHOLDER, tetrixUrl } from './tetrixUrl';

// The field for the address of the person's own Tetrix (TETRIX-URL-1 to 3), under the first run's
// Tetrix row and in Settings › Connectors' Connect Tetrix: Oby offers no Tetrix by name. What they
// typed is checked as they go (a problem shows once they've left the field or pressed Connect, and
// stays live after); Connect waits until it's an address obid would take. `serverSaid` is why the
// last try didn't take (obid's reason, as signInWhy.ts says it), shown till the next.
export function useTetrixAddress(serverSaid?: string, opts: { initial?: string; onConnect?: (url: string) => void } = {}) {
  const [text, setText] = useState(opts.initial ?? '');
  const [seen, setSeen] = useState(false);
  const parsed = tetrixUrl(text);
  const url = parsed && 'url' in parsed ? parsed.url : null;
  const problem = seen && parsed && 'problem' in parsed ? parsed.problem : seen && !text.trim() ? 'Add the address of your Tetrix.' : serverSaid;
  const connect = () => {
    setSeen(true);
    if (!url) return;
    if (opts.onConnect) opts.onConnect(url); else void connectTetrix({ url });
  };
  return { text, setText, url, ready: !!url, problem, connect, seen: () => setSeen(true) };
}

export function TetrixAddress({ text, setText, problem, connect, seen, autoFocus }: ReturnType<typeof useTetrixAddress> & { autoFocus?: boolean }) {
  const why = useId();
  return <>
    <input className="modal-field setup-field" value={text} onChange={(e) => setText(e.target.value)} onBlur={() => { if (text.trim()) seen(); }}
      onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); connect(); } }}
      type="url" inputMode="url" placeholder={TETRIX_URL_PLACEHOLDER} aria-label="Your Tetrix URL" aria-invalid={!!problem} aria-describedby={problem ? why : undefined}
      autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} autoFocus={autoFocus} />
    {problem && <span id={why} className="wl-sub is-problem" role="alert">{problem}</span>}
  </>;
}
