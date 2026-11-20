// The browser panel (docs/contracts/browser-app.md; the design's boards 2 to 17): tabs in the title
// bar's row, the address, who has the tab, the page, and its downloads. It reads the store; its
// buttons call the store's api, the same calls on the demo data and on obid. Moved from the lab
// (tools/obid-spec/lab-ui/browser/src/browser.tsx) as it is.
import type { CSSProperties, ReactNode } from 'react';
import {
  ArrowLeft, ArrowRight, Bug, ClockAlert, Ellipsis, WifiOff, FileDown, Globe, Hand, KeyRound, Lock, Maximize2, Minimize2, Plus, RotateCw, Search, Server, ShieldAlert, X,
} from 'lucide-react';
import { Logo, Sheen, Status, agentName } from './pieces';
import { useBrowser } from './store';
import type { Tab } from './view';

export const YouMark = ({ label }: { label?: string }) => <span className="you-mark" aria-label={label} aria-hidden={label ? undefined : true}>Y</span>;

const Hold = () => <Hand size={14} strokeWidth={1.6} />;

/** The agent's pointer (T3 Code's) with what it is doing beside it. */
export function Pointer({ tab, x, y, ring, labelStyle, size = 20 }: { tab: Tab; x: number; y: number; ring?: boolean; labelStyle?: CSSProperties; size?: number }) {
  if (tab.owner.kind !== 'agent') return null;
  return (
    <div className="agent-pointer" style={{ left: x, top: y }} data-browser="pointer">
      {ring && <span className="agent-ring" />}
      <svg width={size} height={size} viewBox="0 0 24 24" fill="#111" stroke="#fff" strokeWidth="1.6" strokeLinejoin="round" aria-hidden="true"><path d="M4.037 4.688a.495.495 0 0 1 .651-.651l16 6.5a.5.5 0 0 1-.063.947l-6.124 1.58a2 2 0 0 0-1.438 1.435l-1.579 6.126a.5.5 0 0 1-.947.063z" /></svg>
      <span className="agent-label" style={labelStyle}><Logo agent={tab.owner.agent} size={14} />{tab.action}</span>
    </div>
  );
}

/** Your own pointer, while you hold the tab (6). */
export const YouPointer = ({ x, y }: { x: number; y: number }) => (
  <div className="you-pointer" style={{ left: x, top: y }} data-browser="you-pointer">
    <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 3l14 8-6.5 1.5L9.5 19z" fill="#000" stroke="#fff" strokeWidth="1.4" strokeLinejoin="round" /></svg>
  </div>
);

export function Tabs({ full, newTab = true, fullWidth = true, close = true, newTabLabel = 'New tab' }: { full?: boolean; newTab?: boolean; fullWidth?: boolean; close?: boolean; newTabLabel?: string }) {
  const { s, ui, setUi, api } = useBrowser();
  return (
    <div className="br-tabs" role="tablist" aria-label="Tabs">
      {s.tabs.map((t) => {
        const on = t.id === s.active;
        const asked = s.asks.some((a) => a.tab === t.id);
        const dot = t.holder === 'you' && t.owner.kind === 'agent' ? 'held' : asked ? 'needs' : t.dot;
        return (
          <button key={t.id} className={`br-tab${on ? ' is-on' : ''}`} role="tab" aria-selected={on} onClick={() => api.select(t.id)} style={t.basis ? { flexBasis: t.basis } : undefined}>
            {t.owner.kind === 'agent' ? <Logo agent={t.owner.agent} size={14} alt={`${agentName(t.owner.agent)}’s`} /> : <YouMark label="Yours" />}
            <span className="br-tab-name">{t.title}</span>
            {dot && <Status dot={dot} />}
            {on && t.closable !== false && <span className="br-tab-x"><X size={12} strokeWidth={2} /></span>}
          </button>
        );
      })}
      <div className="br-tabs-end">
        {newTab && <button className="icon-button is-28" aria-label={newTabLabel}><Plus size={15} strokeWidth={1.6} /></button>}
        {fullWidth && (full
          ? <button className="icon-button is-28 is-on" aria-label="Back beside the conversation" aria-pressed="true" onClick={() => setUi({ full: false })}><Minimize2 size={14} strokeWidth={1.6} /></button>
          : <button className="icon-button is-28" aria-label="Full width" onClick={() => setUi({ full: true })}><Maximize2 size={14} strokeWidth={1.6} /></button>)}
        {close && <button className="icon-button is-28" aria-label="Close the browser" onClick={() => setUi({ open: false, full: false })}><X size={13} strokeWidth={2} /></button>}
      </div>
    </div>
  );
}

/** "localhost:3000/login" → <b>localhost:3000</b>/login */
export function Path({ url }: { url: string }) {
  const i = url.indexOf('/');
  return <span className="br-path"><b>{i < 0 ? url : url.slice(0, i)}</b>{i < 0 ? '' : url.slice(i)}</span>;
}

/**
 * The address row. Its buttons, as each board has them: `full` back, forward, reload and the page's
 * menu; `three` without the menu; `mini` back and reload (10's small browsers); `loading` back and
 * stop; `off` back, forward and reload, none of them working yet (17).
 */
export function Nav({ tab, set = 'full', back = true, warn, children }: { tab?: Tab; set?: 'full' | 'three' | 'mini' | 'loading' | 'off'; back?: boolean; warn?: boolean; children?: ReactNode }) {
  const icon = warn ? <ShieldAlert size={13} strokeWidth={1.6} /> : !tab ? <Search size={13} strokeWidth={1.6} /> : tab.secure ? <Lock size={12} strokeWidth={1.6} /> : <Globe size={13} strokeWidth={1.6} />;
  const address = (
    <>
      {icon}
      {tab ? <Path url={tab.url} /> : <span className="br-path">Type an address</span>}
      {tab?.computer && <span className="br-where"><Server size={10} strokeWidth={1.6} />on <b>{tab.computer.name}</b></span>}
    </>
  );
  const off = set === 'off';
  return (
    <div className="br-nav" style={set === 'loading' ? { position: 'relative' } : undefined}>
      <button className="icon-button is-28" aria-label="Back" disabled={!back || off}><ArrowLeft size={16} strokeWidth={1.6} /></button>
      {set === 'loading' && <button className="icon-button is-28" aria-label="Stop loading"><X size={14} strokeWidth={1.6} /></button>}
      {(set === 'full' || set === 'three' || off) && <button className="icon-button is-28" aria-label="Forward" disabled><ArrowRight size={16} strokeWidth={1.6} /></button>}
      {set !== 'loading' && <button className="icon-button is-28" aria-label="Reload" disabled={off}><RotateCw size={15} strokeWidth={1.6} /></button>}
      {tab ? <div className={`br-address${warn ? ' is-warn' : ''}`}>{address}</div> : <label className="br-address">{address}</label>}
      {set === 'full' && <button className="icon-button is-28" aria-label="Page options"><Ellipsis size={16} strokeWidth={1.6} /></button>}
      {children}
    </div>
  );
}

/** Who has the tab: the strip under the address (T3 Code's control bar, in Oby's words). */
export function Control({ tab, takeIcon = true, idle }: { tab: Tab; takeIcon?: boolean; idle?: ReactNode }) {
  const { s, ui, api, setUi } = useBrowser();
  if (tab.owner.kind === 'you') {
    return <div className="br-control" role="status" data-browser="control"><span className="br-control-text"><YouMark /><span><b>Your tab.</b> No agent uses it unless you ask.</span></span></div>;
  }
  const who = agentName(tab.owner.agent);
  const logo = <Logo agent={tab.owner.agent} size={14} />;
  const take = <button className={`tc-option is-sm${takeIcon ? ' has-icon' : ''}`} onClick={() => api.take(tab.id)}>{takeIcon && <Hold />}Take control</button>;
  if (ui.card === 'password') {
    return <div className="br-control is-held" role="status" data-browser="control"><span className="br-control-text"><KeyRound size={14} strokeWidth={1.6} /><span><b>You’re filling the password.</b> {who} waits.</span></span></div>;
  }
  if (tab.holder === 'you') {
    return (
      <div className="br-control is-held" role="status" data-browser="control">
        <span className="br-control-text"><YouMark /><span><b>You have this tab.</b> {who} waits until you give it back.</span></span>
        <button className="tc-option is-sm is-primary" onClick={() => { void api.giveBack(tab.id); setUi({ card: null }); }}>Give back</button>
      </div>
    );
  }
  if (s.asks.some((a) => a.tab === tab.id && a.kind === 'signIn')) {
    return <div className="br-control is-needs" role="status" data-browser="control"><span className="br-control-text">{logo}<span><b>{who}</b> asks you to sign in here</span></span>{take}</div>;
  }
  if (tab.state === 'loading') {
    return <div className="br-control" data-browser="control"><span className="br-control-text">{logo}<span><b>{who}</b> · <Sheen>Waiting for the page</Sheen></span></span></div>;
  }
  return (
    <div className="br-control" role="status" data-browser="control">
      <span className="br-control-text">{logo}<span>{tab.action ? <><b>{who}</b> is using this tab · <Sheen>{tab.action}</Sheen></> : idle ?? (tab.ran ? <><b>{who}</b> ran a script in this page · done</> : <><b>{who}</b> opened this tab · idle</>)}</span></span>
      {take}
    </div>
  );
}

/** A page that didn't simply load (board 10's words), in the panel's middle; its way out. */
export function PageState({ tab }: { tab: Tab }) {
  const { api, mode } = useBrowser();
  const host = tab.url.split('/')[0];
  const who = tab.owner.kind === 'agent' ? agentName(tab.owner.agent) : '';
  // On obid the ways out (Try again, Reload, Go back, Go on anyway, a dialog's Leave and Stay) wait for
  // routes obid doesn't have for the owner yet: there, the state's words show alone, and the agent,
  // told every time, acts (BROWSERAPP-14). On the demo data they show as the board does.
  const ways = mode !== 'obid';
  const box = (icon: ReactNode, title: string, text: string, way: ReactNode) => (
    <div className="br-state" data-browser={`page-${tab.state}`}>{icon}<span className="br-state-title">{title}</span><span className="br-state-text">{text}</span>{ways && <div className="tc-options">{way}</div>}</div>
  );
  const again = <button className="tc-option is-sm">Try again</button>;
  switch (tab.state) {
    case 'failed': return box(<WifiOff size={22} strokeWidth={1.6} />, 'This page didn’t load', `${host} refused to connect. Is the app running?`, again);
    case 'timedOut': return box(<ClockAlert size={22} strokeWidth={1.6} />, 'The page took too long', `Nothing came back from ${host} in 30 seconds.`, again);
    case 'certificate': return box(<ShieldAlert size={22} strokeWidth={1.6} />, 'This connection isn’t private', `The certificate for ${host} isn’t trusted. ${who} stopped here and won’t go on by itself.`, <><button className="tc-option is-sm is-primary">Go back</button><button className="tc-option is-sm is-quiet" style={{ color: 'var(--danger)' }}>Go on anyway</button></>);
    case 'crashed': return box(<Bug size={22} strokeWidth={1.6} />, 'This page stopped working', `Its tab crashed, out of memory. ${who} was told and can open it again.`, <button className="tc-option is-sm">Reload</button>);
    case 'dialog': {
      const held = tab.holder === 'you';
      return (
        <div className="br-over" role="dialog" aria-label="The page asks" data-browser="page-dialog">
          <span className="br-over-kind">{host} asks</span>
          <p className="br-over-text">{tab.dialog}</p>
          {ways && <div className="tc-options" style={{ marginTop: 10 }}>
            <button className="tc-option is-sm" disabled={!held} style={held ? undefined : { opacity: 0.4 }}>Leave</button>
            <button className="tc-option is-sm" disabled={!held} style={held ? undefined : { opacity: 0.4 }}>Stay</button>
            {!held && <button className="tc-option is-sm is-primary" style={{ marginLeft: 'auto' }} onClick={() => api.take(tab.id)}>Take control to answer</button>}
          </div>}
        </div>
      );
    }
    default: return null;
  }
}

/**
 * The live page: the stream's newest frame, drawn from the top left at the panel's width. While you
 * hold the tab, your mouse and keys on it go to the page (POST …/input, signed by main right after
 * the very input it saw: BROWSERAPP-10), in the page's own px; never unless you hold it.
 */
function LiveFrame({ tab, url, size }: { tab: Tab; url: string; size: { width: number; height: number } }) {
  const { api } = useBrowser();
  const held = tab.holder === 'you';
  const at = (e: { clientX: number; clientY: number; currentTarget: Element }) => {
    const r = e.currentTarget.getBoundingClientRect();
    const scale = Math.min(r.width / size.width, r.height / size.height) || 1;
    return { x: Math.round((e.clientX - r.left) / scale), y: Math.round((e.clientY - r.top) / scale) };
  };
  const send = (e: Parameters<NonNullable<typeof api.input>>[1]) => { if (held) void api.input?.(tab.id, e); };
  return (
    <div
      className="br-frame" tabIndex={held ? 0 : -1} data-browser="frame-input" aria-label={held ? 'The page: you have it' : undefined}
      style={{ position: 'absolute', inset: 0, outline: 'none', cursor: held ? 'default' : 'not-allowed' }}
      onPointerDown={(e) => { if (!held) return; e.currentTarget.focus(); const p = at(e); send({ type: 'mouse', eventType: 'mouseMoved', ...p }); send({ type: 'mouse', eventType: 'mousePressed', ...p, button: 'left', clickCount: 1 }); }}
      onPointerUp={(e) => send({ type: 'mouse', eventType: 'mouseReleased', ...at(e), button: 'left', clickCount: 1 })}
      onWheel={(e) => send({ type: 'mouse', eventType: 'mouseWheel', ...at(e), deltaX: e.deltaX, deltaY: e.deltaY })}
      onKeyDown={(e) => { if (!held) return; e.preventDefault(); send({ type: 'key', eventType: 'keyDown', key: e.key, code: e.code, text: e.key.length === 1 ? e.key : null }); }}
      onKeyUp={(e) => { if (!held) return; e.preventDefault(); send({ type: 'key', eventType: 'keyUp', key: e.key, code: e.code }); }}
    >
      <img src={url} alt="" data-browser="frame" draggable={false} style={{ display: 'block', width: '100%', height: '100%', objectFit: 'contain', objectPosition: 'top left', pointerEvents: 'none' }} />
    </div>
  );
}

const STOPPED = ['failed', 'timedOut', 'certificate', 'crashed'];

/** The page: the demo's site, or on obid the live stream's frame and the page's own states. */
export function View({ tab, label, page, children, className = '', style }: { tab: Tab; label: string; page: ReactNode; children?: ReactNode; className?: string; style?: CSSProperties }) {
  const { mode, frame } = useBrowser();
  const held = tab.holder === 'you' && tab.owner.kind === 'agent';
  const live = mode === 'obid' ? frame(tab.id) : null;
  return (
    <div className={`br-view${held ? ' is-held' : ''} ${className}`.trim()} role="img" aria-label={label} style={style} data-browser="view">
      {STOPPED.includes(tab.state)
        ? <div className="br-view is-blank" style={{ position: 'absolute', inset: 0 }}><PageState tab={tab} /></div>
        : mode === 'obid'
        ? (live ? <LiveFrame tab={tab} url={live.url} size={live} /> : <div className="br-state"><span className="br-state-text"><Sheen>Waiting for the page</Sheen></span></div>)
        : page}
      {tab.state === 'dialog' && <PageState tab={tab} />}
      {children}
    </div>
  );
}

/** One file on the shelf at the panel's foot (12). */
export function FileRow({ id }: { id: string }) {
  const { s, api } = useBrowser();
  const d = s.downloads.find((x) => x.id === id);
  if (!d) return null;
  const agent = d.by.kind === 'agent' ? d.by.agent : null;
  return (
    <div className="br-file" data-browser={`file-${d.id}`}>
      <FileDown size={18} strokeWidth={1.6} />
      <span className="br-file-text">
        <span className="br-file-name">{d.name}</span>
        <span className="br-file-about">
          {agent && <span className="logo" style={{ display: 'inline-block', verticalAlign: -1, marginRight: 4, width: 11, height: 11 }}><Logo agent={agent} size={11} /></span>}
          {agent ? (d.saved ? `${agentName(agent)} downloaded it · in Downloads` : `${agentName(agent)} downloaded it · kept in its browser, not on your Mac`) : 'You downloaded it · in Downloads'}
        </span>
      </span>
      {agent && !d.saved
        ? <button className="tc-option is-sm" onClick={() => api.saveCopy(d.id)}>Save a copy</button>
        : <button className="tc-option is-sm" onClick={() => api.reveal(d.id)}>Show in Finder</button>}
    </div>
  );
}

export function Panel({ width, full, label = 'Browser', resize = true, children }: { width?: number; full?: boolean; label?: string; resize?: boolean; children: ReactNode }) {
  return (
    <aside className={`br${full ? ' is-full' : ''}`} style={full ? undefined : { width }} aria-label={label} data-browser="browser">
      {resize && !full && <span className="br-resize" aria-hidden="true" />}
      {children}
    </aside>
  );
}
