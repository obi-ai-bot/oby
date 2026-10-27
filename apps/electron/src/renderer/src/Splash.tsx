import { useEffect, useState } from 'react';
import { ObyFace } from './ObyFace';
import { useLive } from './live';

// Oby's face on the window's background until obid answers and the first load is in (main.tsx):
// nothing more while that's quick, a quiet line once it takes a while, and obid's own reason when
// something is wrong (main/obid.ts, reason). It goes by itself when obid comes up (live.ts retries).
export default function Splash() {
  const { problem, why } = useLive();
  const [slow, setSlow] = useState(false);
  useEffect(() => { const t = setTimeout(() => setSlow(true), 4000); return () => clearTimeout(t); }, []);
  const failed = !!problem && why !== 'waiting';
  const line = failed ? problem : slow ? 'Getting ready…' : null;
  return (
    <main className="splash" role="status" aria-busy={!failed}>
      <div className="splash-face" role="img" aria-label={line ? 'Oby' : 'Oby is getting ready'}><ObyFace size={88} /></div>
      {line && <p className={`splash-line${failed ? ' is-failed' : ''}`}>{line}</p>}
      {failed && why === 'approval' && <button className="set-button" onClick={() => window.app?.openLoginItems()}>Open Login Items</button>}
    </main>
  );
}
