import { Suspense, lazy, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { Panel } from '../browser/Panel';
import { closeArtifact, kindName, showTab, useOpenArtifact, type Tab } from './store';
// Code loads with its first use: CodeMirror reads the page as it loads, and needn't on every start
const Code = lazy(() => import('./Code').then((m) => ({ default: m.Code })));
import '../browser/browser.css';
import './artifacts.css';

// The artifact beside its conversation (Sangam's board 1b; docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-1, 4 and 5), in the browser panel's place and frame: its strip, with Preview and Code
// as LibreChat's panel has them, then the tab. Preview isn't the page's: main draws it, with
// Sandpack, in a view of its own laid over the box this pane keeps for it (main/artifactPreview.ts),
// told where that box is whenever it moves, and told nothing while Code shows.
const TABS: { tab: Tab; label: string }[] = [{ tab: 'preview', label: 'Preview' }, { tab: 'code', label: 'Code' }];

export function ArtifactPane({ width }: { width: number }) {
  const open = useOpenArtifact();
  const box = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const id = open?.artifact.id, version = open?.version, previewing = open?.tab === 'preview';

  useLayoutEffect(() => {
    const el = box.current;
    const preview = window.app?.artifactPreview;
    if (!el || !id || !version || !preview || !previewing) return;
    setError(null);
    let gone = false;
    const tell = () => {
      const r = el.getBoundingClientRect();
      void preview({ id, version, bounds: { x: r.left, y: r.top, width: r.width, height: r.height } }).then((a) => {
        if (!gone && a && 'error' in a) setError(a.error);
      });
    };
    tell();
    const ro = new ResizeObserver(tell);
    ro.observe(el);
    window.addEventListener('resize', tell);
    return () => { gone = true; ro.disconnect(); window.removeEventListener('resize', tell); void preview(null); };
  }, [id, version, previewing]);
  useEffect(() => () => { void window.app?.artifactPreview?.(null); }, []);

  if (!open) return null;
  const { artifact } = open;
  return (
    <Panel width={width} label={`${artifact.title}, an artifact`}>
      <div className="art-br" style={{ display: 'contents' }}>
        <div className="art-strip" data-artifact-strip>
          <span className="art-strip-title">
            <b>{artifact.title}</b>
            <span>Oby artifact · {kindName(artifact.kind)} · v{open.version}</span>
          </span>
          <span className="art-tabs" role="tablist" aria-label="Preview or code">
            {TABS.map(({ tab, label }) => (
              <button key={tab} role="tab" aria-selected={open.tab === tab} className={`art-tab${open.tab === tab ? ' is-on' : ''}`} onClick={() => showTab(tab)}>{label}</button>
            ))}
          </span>
          <button className="icon-button is-28" aria-label="Close the artifact" onClick={closeArtifact}><X size={14} strokeWidth={2} /></button>
        </div>
        {open.tab === 'code' ? <Suspense fallback={null}><Code artifact={artifact} version={open.version} /></Suspense> : (
          <div ref={box} className="art-view" data-artifact-view={artifact.id} role="region" aria-label={`${artifact.title}, its preview`}>
            {!window.app?.artifactPreview && <p className="art-view-note">The preview shows in Oby’s Mac app.</p>}
            {error && <p className="art-view-note" role="alert">{error}</p>}
          </div>
        )}
      </div>
    </Panel>
  );
}
