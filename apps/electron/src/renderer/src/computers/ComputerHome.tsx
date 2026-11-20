// What the window says about the computer in use, outside the menu (the approved design's screens
// 07 to 09; docs/contracts/computers-app.md, COMPUTERSAPP-5 to 8): the window's connection line
// while a fallback route takes over (only past 2 s) or, outside Home, while no route answers; and
// Home's line under Oby while no route answers (every route tried, Try again, Back to This Mac) or
// while a switch takes its moment. Quiet otherwise.
import { useEffect, useState } from 'react';
import { current, retry, use, useComputers } from './store';
import { ROUTE_NAME, lostHome, lostLine, movingLine, switchingHome, type Computer } from './view';

/** A fallback route shows only once it has taken this long. */
export const QUIET_MS = 2000;

/** The connection line: a fallback past 2 s, or no route (red edge) when Home isn't showing. */
export function ComputerLine({ onHome }: { onHome: boolean }) {
  const s = useComputers();
  const c = current(s);
  const since = c ? s.since[c.id] : undefined;
  const [, tick] = useState(0);
  const wait = since === undefined ? null : since + QUIET_MS - Date.now();
  useEffect(() => {
    if (since === undefined) return;
    const t = setTimeout(() => tick((n) => n + 1), Math.max(0, since + QUIET_MS - Date.now()));
    return () => clearTimeout(t);
  }, [since]);
  if (!s.offered || !c || s.ui.switching) return null;
  if (c.state === 'lost' && !onHome) return <p className="connection is-down" role="status">{lostLine(c.name)}</p>;
  if (c.state === 'reconnecting' && c.from && c.route && wait !== null && wait <= 0) {
    return <p className="connection is-connecting" role="status">{movingLine(c.name, c.from, c.route.kind)}</p>;
  }
  return null;
}

/** Whether Home's stage says something of the computer's instead of its own line and chips. */
export function useComputerStage(): boolean {
  const s = useComputers();
  return s.offered && (!!s.ui.switching || current(s)?.state === 'lost');
}

/** Home's line under Oby while no route answers, or while a switch takes its moment. */
export function ComputerStage() {
  const s = useComputers();
  const c = current(s);
  if (!s.offered) return null;
  if (s.ui.switching) {
    return (
      <section className="attention" aria-label="Your tasks">
        <span className="attention-label is-idle" role="status">{switchingHome(s.ui.switching.to, s.ui.switching.from)}</span>
      </section>
    );
  }
  if (c?.state !== 'lost') return null;
  return (
    <section className="attention" aria-label="Your tasks">
      <span className="attention-label" role="status">{lostHome(c.name)}</span>
      <Tried c={c} />
      <div className="chips">
        <button className="chip" onClick={() => void retry(c.id)}>Try again</button>
        <button className="chip" onClick={() => void use('this')}>Back to This Mac</button>
      </div>
    </section>
  );
}

function Tried({ c }: { c: Computer }) {
  if (!c.tried?.length) return null;
  return (
    <ul className="computer-tried">
      {c.tried.map((t, i) => (
        <li key={i}><b>{ROUTE_NAME[t.route]}</b>{t.what ? ` · ${t.what}` : ''} · {t.why}</li>
      ))}
    </ul>
  );
}
