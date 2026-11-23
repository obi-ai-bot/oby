// Where the browser sits in the window (docs/contracts/browser-app.md): the panel beside the
// conversation (2), with the side panel (3), narrow (4), across the window (5) or with no pages yet
// (17); and the globe at the title bar's end that shows and hides it (1). Built from the lab's
// screens (tools/obid-spec/lab-ui/browser/src/scenes.tsx), whose conversations were the boards'
// own: here the conversation is the app's.
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { CodeXml, Server } from 'lucide-react';
import { Control, FileRow, Nav, Panel, Pointer, Tabs, View, YouPointer } from './Panel';
import { Globe17, Logo, agentName } from './pieces';
import { useBrowser } from './store';
import { StartPage } from './Servers';
import { isOwn } from './view';
import { demoPage } from './demo';
import { shown } from './live';
import { useLive } from '../live';
import { registerCommands, shortcutLabel } from '../shortcuts';
import './browser.css';

/** The globe at the bar's end: closed, it says how many tabs and who's using one; open, it hides the panel. */
export function BrowserGlobe() {
  const { offered, ui, setUi, s } = useBrowser();
  const [tip, setTip] = useState(false);
  const globe = offered && s.settings.agentsUseBrowser;
  // ⇧⌘B (SHORTCUTS-12): the globe's own click, while there's a globe
  useEffect(() => (globe ? registerCommands({ 'browser.show': () => setUi({ open: true }) }) : undefined), [globe]);
  if (!globe) return null;
  if (ui.open) return <Globe17 on label="Hide the browser" onClick={() => setUi({ open: false, full: false })} />;
  const using = s.tabs.find((t) => t.holder === 'agent' && t.action);
  const who = using?.owner.kind === 'agent' ? `, ${agentName(using.owner.agent)} is using one` : '';
  return (
    <div style={{ position: 'relative' }} onMouseEnter={() => setTip(true)} onMouseLeave={() => setTip(false)}>
      <Globe17 label={`Show the browser, ${s.tabs.length} ${s.tabs.length === 1 ? 'tab' : 'tabs'}${who}`} badge={!!using} onClick={() => setUi({ open: true })} />
      {tip && <span className="tip-under" role="tooltip">Show the browser<kbd>{shortcutLabel('browser.show')}</kbd></span>}
    </div>
  );
}

/** The panel, as wide as Home lets it be (browser/fit.ts). */
export function BrowserPane({ width }: { width: number }) {
  const { offered, ui, setUi, s, active, peek: demoPeek, mode } = useBrowser();
  const { jobs } = useLive();
  const view = useRef<HTMLDivElement>(null);
  const [viewWidth, setViewWidth] = useState(0);
  useLayoutEffect(() => {
    const el = view.current?.querySelector('.br-view');
    if (!el) return;
    const ro = new ResizeObserver(() => setViewWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  });
  if (!offered || !ui.open) return null;

  // 17: no pages yet; the address takes what the owner types, and the agents' servers show (BROWSERAPP-45, 47)
  if (!active) {
    return (
      <Panel width={width} label="Browser, no pages yet" resize={false}>
        <Tabs fullWidth={false} />
        <Nav set="off" />
        <StartPage />
      </Panel>
    );
  }
  // New tab: a blank tab of the owner's own, its address ready, the agents' servers under it
  if (isOwn(active) && !active.src) {
    return (
      <Panel width={width} full={ui.full} label="Browser, new tab" resize={false}>
        <Tabs full={ui.full} />
        <Nav tab={active} set="off" />
        <StartPage tab={active.id} />
      </Panel>
    );
  }

  const held = active.holder === 'you';
  // the conversation folded to one line: the tab's task, and what its agent is at (on obid, its job's title)
  const peek = demoPeek ?? (mode === 'obid' ? { agent: active.owner, title: (active.job && jobs[active.job]?.title) || active.title, line: active.action ?? shown(active.url) } : null);
  const where = active.computer;
  // board 10's loading row: on obid while the tab's own page loads, as main says (BROWSERAPP-32)
  const loading = mode === 'obid' ? active.pageLoading === true : active.state === 'loading';
  const { page, label } = mode === 'demo' ? demoPage(active) : { page: null, label: active.title };
  // the agent's label turns to the pointer's left near the page's right edge (5)
  const flip = active.pointer && viewWidth > 0 && active.pointer.x > viewWidth - 220;
  const pointer = active.pointer && <Pointer tab={active} x={active.pointer.x} y={active.pointer.y} ring={active.pointer.ring} labelStyle={flip ? { left: 'auto', right: 6 } : undefined} />;
  return (
    <div ref={view} style={{ display: 'contents' }}>
      <Panel width={width} full={ui.full} label={ui.full ? 'Browser, full width' : where ? `Browser on ${where.name}` : 'Browser'} resize={!ui.full && !held}>
        <Tabs full={ui.full} newTabLabel={where ? `New tab on ${where.name}` : 'New tab'} />
        {where && <div className="where-bar" data-browser="where"><Server size={13} strokeWidth={1.6} /><span>This browser runs on <b>{where.name}</b>. Its localhost is {where.name}’s, not this Mac’s.</span></div>}
        {loading
          ? <Nav tab={active} set="loading"><span className="br-loading" style={{ left: 8 }} /></Nav>
          : <Nav tab={active} warn={active.state === 'certificate'} />}
        <Control tab={active} />
        <View tab={active} label={label} page={page} className={where ? 'is-dark-page' : ''}>
          {held ? mode === 'demo' && <YouPointer x={380} y={268} /> : pointer}
          {!held && active.ran && active.owner.kind === 'agent' && (
            <span className="ran"><Logo agent={active.owner.agent} size={14} /><CodeXml size={13} strokeWidth={1.6} />Ran a script{active.ran.ms !== undefined ? ` · ${active.ran.ms} ms` : ''}</span>
          )}
          {ui.full && peek && (
            <div className="peek" role="group" aria-label="The conversation, while the browser is full width">
              {peek.agent.kind === 'agent' && <Logo agent={peek.agent.agent} size={16} />}
              <span className="peek-text"><b>{peek.title}</b> · {peek.line}</span>
              <button className="tc-option is-sm" onClick={() => setUi({ full: false })}>Show the conversation</button>
            </div>
          )}
        </View>
        {s.downloads.length > 0 && <div className="br-shelf" aria-label="Downloads">{s.downloads.map((d) => <FileRow key={d.id} id={d.id} />)}</div>}
      </Panel>
    </div>
  );
}
