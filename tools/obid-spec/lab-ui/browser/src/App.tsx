// The lab's pages: the index (every board, a link each) and one board's screen at its own size.
// /#/<n> draws board n on its fixture; /#/<n>?live draws it on lane A's API (live.ts).
import { useEffect, useState } from 'react';
import { BOARDS } from './boards';
import { LabProvider, useLab } from './store';

/** Live mode's trouble (no API, a refusal), under the window: the lab's own words, not Oby's. */
function Problem() {
  const { problem } = useLab();
  return problem ? <p className="lab-problem" data-lab="problem">{problem}</p> : null;
}

const parse = () => {
  const [path, query = ''] = location.hash.replace(/^#\/?/, '').split('?');
  const q = new URLSearchParams(query);
  return { n: Number(path) || 0, live: q.has('live') ? q.get('live') || 'proxy' : null };
};

export default function App() {
  const [route, setRoute] = useState(parse);
  useEffect(() => {
    const on = () => setRoute(parse());
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  }, []);
  const board = BOARDS.find((b) => b.n === route.n);
  if (!board) {
    return (
      <div className="lab-index">
        <h1>Oby’s browser, in the lab</h1>
        <p>The approved design’s 17 states, on the app’s own styles. Each opens at its board’s size; its buttons work (Watch, Take control, Give back, Full width, Type the password, Fill in, Save a copy, the switches, Stop). The design board itself is at <code>/board/&lt;name&gt;.html</code>.</p>
        <ol>{BOARDS.map((b) => <li key={b.n}><a href={`#/${b.n}`}>{b.title}</a> · <a href={`/board/${b.file}.html`}>board</a></li>)}</ol>
      </div>
    );
  }
  return (
    <main className="lab-stage" data-board={board.n} data-mode={route.live ? 'live' : 'fixtures'}>
      <LabProvider key={`${board.n}-${route.live}`} start={board.state} ui={board.ui ?? {}} live={route.live}>
        {board.screen()}
        <Problem />
      </LabProvider>
    </main>
  );
}
