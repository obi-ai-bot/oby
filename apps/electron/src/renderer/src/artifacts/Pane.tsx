import { Suspense, lazy, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Download, RotateCw, X } from 'lucide-react';
import { Panel } from '../browser/Panel';
import { useClicksEverywhere } from '../Menu';
import { closeArtifact, kindName, onLatest, save, showTab, showVersion, useOpenArtifact, type Tab } from './store';
import { ASK_TO_FIX, FIX_WORDS, saveAs } from '../../../shared/words';
import { ask, uploadFiles, useLive } from '../live';
// Code loads with its first use: CodeMirror reads the page as it loads, and needn't on every start
const Code = lazy(() => import('./Code').then((m) => ({ default: m.Code })));
import '../browser/browser.css';
import './artifacts.css';

// The artifact beside its conversation (Sangam's board 1b; docs/contracts/artifacts-app.md,
// ARTIFACTSAPP-1, 4, 5, 8 and 9), in the browser panel's place and frame: its strip, with its
// version menu, Save once the owner edited it, Reload on Preview, Preview and Code as LibreChat's panel has them, then
// the tab. Preview isn't the page's: main draws it, with
// Sandpack, in a view of its own laid over the box this pane keeps for it (main/artifactPreview.ts),
// told where that box is whenever it moves, and told nothing while Code shows.
const TABS: { tab: Tab; label: string }[] = [{ tab: 'preview', label: 'Preview' }, { tab: 'code', label: 'Code' }];

export function ArtifactPane({ width }: { width: number }) {
  const open = useOpenArtifact();
  const box = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  // what the page on view said broke, as main answered it (ARTIFACTSAPP-24), and Ask to fix on its way
  const [pageError, setPageError] = useState<string | null>(null);
  const [fixing, setFixing] = useState(false);
  const { waiting } = useLive();
  const [saved, setSaved] = useState<string | null>(null);
  const [menu, setMenu] = useState(false);
  useEffect(() => { setMenu(false); setSaved(null); }, [open?.artifact.id, open?.version]);
  // Export's menu closes as the app's other menus do (Menu.tsx): Esc (focus back on Export), a click
  // outside it and its button, or the window changing size
  const exportWrap = useRef<HTMLSpanElement>(null), exportButton = useRef<HTMLButtonElement>(null);
  useClicksEverywhere(menu);
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(false);
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); e.stopImmediatePropagation(); close(); exportButton.current?.focus(); } };
    const onDown = (e: Event) => { if (!exportWrap.current?.contains(e.target as Node)) close(); };
    addEventListener('keydown', onKey, true);
    addEventListener('pointerdown', onDown, true);
    addEventListener('resize', close);
    return () => { removeEventListener('keydown', onKey, true); removeEventListener('pointerdown', onDown, true); removeEventListener('resize', close); };
  }, [menu]);
  // obid's export of the version on view, saved where the owner picks in main's dialog
  const download = async (as: 'zip' | 'html') => {
    setMenu(false);
    if (!open || !window.app?.artifactExport) return;
    const r = await window.app.artifactExport(open.artifact.id, open.version, as);
    setSaved(!r ? null : 'saved' in r ? `Saved to ${r.saved}` : 'error' in r ? r.error : null);
  };
  // main's view lies over the page: while Export's menu is open, Preview's view steps aside for it
  const id = open?.artifact.id, version = open?.version, previewing = open?.tab === 'preview' && !menu;
  // the owner's unsaved edits show in Preview as they would once saved (ARTIFACTSAPP-8)
  const edits = open && Object.keys(open.edits).length ? open.edits : undefined;

  useLayoutEffect(() => {
    const el = box.current;
    const preview = window.app?.artifactPreview;
    if (!el || !id || !version || !preview || !previewing) return;
    setError(null);
    setPageError(null);
    let gone = false;
    const tell = () => {
      const r = el.getBoundingClientRect();
      void preview({ id, version, bounds: { x: r.left, y: r.top, width: r.width, height: r.height }, ...(edits ? { edits } : {}) }).then((a) => {
        if (!gone && a && 'error' in a) setError(a.error);
        // a resize of the same version answers without it: the strip stays
        if (!gone && a && 'shown' in a && a.pageError) setPageError(a.pageError);
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
  // the page's error to the agent of the artifact's conversation, only on this click: as an attached
  // file, never in the words (data from the page, "From outside"); its words carry the artifact by
  // themselves (ARTIFACTSAPP-7), signed by the button's own label (WORDS-24)
  const conversation = artifact.conversation;
  const fix = async () => {
    if (!pageError || !conversation) return;
    setFixing(true);
    try {
      const files = await uploadFiles([{ name: 'preview-error.txt', file: new File([pageError], 'preview-error.txt', { type: 'text/plain' }) }]);
      if (files) ask(FIX_WORDS, conversation, undefined, undefined, { files });
    } finally {
      setFixing(false);
    }
  };
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
          {/* board 1b's Reload: main runs the version on view again (ARTIFACTSAPP-20) */}
          {open.tab === 'preview' && window.app?.artifactReload && <button className="icon-button is-28" aria-label="Reload" onClick={() => void window.app?.artifactReload?.()}><RotateCw size={15} strokeWidth={1.6} /></button>}
          <span className="art-tabs" role="tablist" aria-label="Preview or code">
            {TABS.map(({ tab, label }) => (
              <button key={tab} role="tab" aria-selected={open.tab === tab} className={`art-tab${open.tab === tab ? ' is-on' : ''}`} onClick={() => showTab(tab)}>{label}</button>
            ))}
          </span>
          <span className="art-export-wrap" ref={exportWrap}>
            <button ref={exportButton} className="icon-button is-28" aria-label="Export" aria-haspopup="menu" aria-expanded={menu} onClick={() => setMenu(!menu)}><Download size={14} strokeWidth={1.8} /></button>
            {menu && (
              // Download, as Sangam's board 1c has it: obid's own export of the version on view (ARTIFACTSAPP-11)
              <div className="art-export" role="menu" aria-label="Download">
                <p className="art-export-head">Download</p>
                <button role="menuitem" className="art-export-item" onClick={() => void download('zip')}>
                  <span>Project archive (.zip)</span>
                  {artifact.kind === 'design' && <small>Every file, as Claude Design keeps it: opens there unchanged</small>}
                </button>
                <button role="menuitem" className="art-export-item" onClick={() => void download('html')}><span>Standalone HTML</span></button>
              </div>
            )}
          </span>
          <button className="icon-button is-28" aria-label="Close the artifact" onClick={closeArtifact}><X size={14} strokeWidth={2} /></button>
        </div>
        {/* where a download went, or why it didn't */}
        {saved && <p className="art-saved" role="status">{saved}</p>}
        {/* why the last Save was refused, the edits kept (ARTIFACTSAPP-10) */}
        {open.refused && <p className="art-refused" role="alert">{open.refused}</p>}
        {/* what the page on view said broke (ARTIFACTSAPP-24): its text as text, and Ask to fix */}
        {open.tab === 'preview' && pageError && (
          <div className="art-page-error" role="alert">
            <p>This page stopped with an error.</p>
            <pre>{pageError}</pre>
            {conversation && <button className="tc-option is-sm" disabled={fixing || !!waiting[conversation]} onClick={() => void fix()}>{ASK_TO_FIX}</button>}
          </div>
        )}
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
