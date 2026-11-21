import { Suspense, lazy, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { Panel } from '../browser/Panel';
import { closeArtifact, kindName, onLatest, save, showTab, showVersion, useOpenArtifact, type Tab } from './store';
import { saveAs } from '../../../shared/words';
// Code loads with its first use: CodeMirror reads the page as it loads, and needn't on every start
const Code = lazy(() => import('./Code').then((m) => ({ default: m.Code })));
import '../browser/browser.css';
import './artifacts.css';

// The artifact beside its conversation (Sangam's board 1b; docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-1, 4, 5, 8 and 9), in the browser panel's place and frame: its strip, with its
// version menu, Save once the owner edited it, Preview and Code as LibreChat's panel has them, then
// the tab. Preview isn't the page's: main draws it, with
// Sandpack, in a view of its own laid over the box this pane keeps for it (main/artifactPreview.ts),
// told where that box is whenever it moves, and told nothing while Code shows.
const TABS: { tab: Tab; label: string }[] = [{ tab: 'preview', label: 'Preview' }, { tab: 'code', label: 'Code' }];

export function ArtifactPane({ width }: { width: number }) {
  const open = useOpenArtifact();
  const box = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const id = open?.artifact.id, version = open?.version, previewing = open?.tab === 'preview';
  // the owner's unsaved edits show in Preview as they would once saved (ARTIFACTSAPP-8)
  const edits = open && Object.keys(open.edits).length ? open.edits : undefined;

  useLayoutEffect(() => {
    const el = box.current;
    const preview = window.app?.artifactPreview;
    if (!el || !id || !version || !preview || !previewing) return;
    setError(null);
    let gone = false;
    const tell = () => {
      const r = el.getBoundingClientRect();
      void preview({ id, version, bounds: { x: r.left, y: r.top, width: r.width, height: r.height }, ...(edits ? { edits } : {}) }).then((a) => {
        if (!gone && a && 'error' in a) setError(a.error);
      });
    };
    tell();
    const ro = new ResizeObserver(tell);
    ro.observe(el);
    window.addEventListener('resize', tell);
    return () => { gone = true; ro.disconnect(); window.removeEventListener('resize', tell); void preview(null); };
  }, [id, version, previewing, edits]);
  useEffect(() => () => { void window.app?.artifactPreview?.(null); }, []);

  if (!open) return null;
  const { artifact } = open;
  const latest = artifact.versions.at(-1)?.version ?? 1;
  const edited = Object.keys(open.edits).length > 0;
  return (
    <Panel width={width} label={`${artifact.title}, an artifact`}>
      <div className="art-br" style={{ display: 'contents' }}>
        <div className="art-strip" data-artifact-strip>
          <span className="art-strip-title">
            <b>{artifact.title}</b>
            <span>Oby artifact · {kindName(artifact.kind)} · v{open.version}</span>
          </span>
          {/* its versions, as LibreChat's version menu has them, from two on; the latest marked */}
          {artifact.versions.length > 1 && (
            <select className="art-version" aria-label="Version" value={open.version} onChange={(e) => showVersion(Number(e.target.value))}>
              {[...artifact.versions].reverse().map((v) => <option key={v.version} value={v.version}>{v.version === latest ? `v${v.version}, the latest` : `v${v.version}`}</option>)}
            </select>
          )}
          {edited && onLatest(open) && <button className="tc-option is-sm art-save" onClick={() => void save()}>{saveAs(open.version)}</button>}
          <span className="art-tabs" role="tablist" aria-label="Preview or code">
            {TABS.map(({ tab, label }) => (
              <button key={tab} role="tab" aria-selected={open.tab === tab} className={`art-tab${open.tab === tab ? ' is-on' : ''}`} onClick={() => showTab(tab)}>{label}</button>
            ))}
          </span>
          <button className="icon-button is-28" aria-label="Close the artifact" onClick={closeArtifact}><X size={14} strokeWidth={2} /></button>
        </div>
        {/* why the last Save was refused, the edits kept (ARTIFACTSAPP-10) */}
        {open.refused && <p className="art-refused" role="alert">{open.refused}</p>}
        {open.tab === 'code' ? <Suspense fallback={null}><Code artifact={artifact} version={open.version} edits={open.edits} editable={onLatest(open)} /></Suspense> : (
          <div ref={box} className="art-view" data-artifact-view={artifact.id} role="region" aria-label={`${artifact.title}, its preview`}>
            {!window.app?.artifactPreview && <p className="art-view-note">The preview shows in Oby’s Mac app.</p>}
            {error && <p className="art-view-note" role="alert">{error}</p>}
          </div>
        )}
      </div>
    </Panel>
  );
}
