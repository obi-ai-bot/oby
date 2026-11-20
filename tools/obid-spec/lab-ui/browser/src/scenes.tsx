// The design's 17 states (design/boards, the canvas "Oby's browser"), each a screen made of the
// pieces, its fixture data in fixtures.ts. Every word is the board's. The screens move between
// states the way the boards show: Watch and the globe open the browser (1 → 2), Take control and
// Give back (2 ⇄ 6), Full width and back (2 ⇄ 5), Type the password and Fill in (7 → 8), Save a copy.
import { useState } from 'react';
import { Bug, Check, ChevronDown, ChevronRight, ClockAlert, CodeXml, Download, Folder, Globe, Plus, Search, Server, ShieldAlert, WifiOff, X } from 'lucide-react';
import { ChatsCircleIcon, GearSixIcon } from '@phosphor-icons/react';
import { ObyFace } from '@app/ObyFace';
import { Bar, Code, Composer, Globe17, Log, Logo, LogRow, Oby, Sheen, Status, TaskCard, Thread, Window, You } from './pieces';
import { Control, FileRow, Nav, Panel, Pointer, Tabs, View, YouPointer } from './browser';
import { Card, Note, Options, PasswordCard, SignInCard, Strip, Text } from './cards';
import * as Site from './sites';
import { useLab } from './store';
import type { Tab } from './api/types';

const Chat = ({ children }: { children: React.ReactNode }) => <div className="chat-layer">{children}</div>;

// ---------------------------------------------------------------------------------------------
// 1, 2, 5, 6: the sign-up flow. One conversation; the browser closed, open, full width, or held.

function SignupThread({ signedUp = true, watch }: { signedUp?: boolean; watch?: () => void }) {
  const { active, ui } = useLab();
  const held = active?.holder === 'you';
  return (
    <Thread>
      <You time="2:14 PM">Check the sign-up flow on <Code>localhost:3000</Code> end to end, and tell me where it breaks.</You>
      <Oby time="2:14 PM">On it. Codex is signing up a test account in the browser now.</Oby>
      <div role="listitem" className="msg is-oby" style={{ marginTop: 12 }}><div className="msg-task-work">
        <TaskCard agent="codex" title="Sign-up flow works end to end" meta={held ? 'Waiting for you · Codex · acme-web' : 'Running · Codex · acme-web'} progress={['done', 'current', '']} dot={held ? 'held' : 'working'} />
        <Log>
          <LogRow mark="check">Opened <code>localhost:3000/signup</code> in the browser</LogRow>
          {signedUp && <LogRow mark="check" meta="4s">Signed up as <code>test+0419@example.com</code></LogRow>}
          <LogRow mark="check" meta="1s">Filled Email and Password on <code>/login</code></LogRow>
          {held ? <>
            <p className="lt-answered"><span className="status-dot is-held" style={{ width: 6, height: 6 }} />You took control of the tab · 2:16 PM</p>
            <LogRow mark="x" meta="Refused" refused>Click Sign in · <b>you have the tab</b></LogRow>
            <p className="chat-live" style={{ marginTop: 6, paddingLeft: 2 }}><span className="status-dot is-held" style={{ width: 8, height: 8 }} />Codex waits for the tab</p>
          </> : <>
            {ui.gaveBack && <p className="lt-answered"><Check size={12} strokeWidth={2} />You gave the tab back · 2:17 PM</p>}
            <LogRow mark="working" after={watch && (
              <button className="tc-option is-sm has-icon" style={{ marginLeft: 6, height: 22, padding: '0 9px' }} onClick={watch}><Globe size={12} strokeWidth={1.6} />Watch</button>
            )}><Sheen>{active?.action ?? 'Clicking Sign in'}</Sheen></LogRow>
          </>}
        </Log>
      </div></div>
    </Thread>
  );
}

function GlobeClosed() {
  const { setUi, s } = useLab();
  const [tip, setTip] = useState(false);
  const using = s.tabs.some((t) => t.holder === 'agent' && t.action);
  if (!s.settings.agentsUseBrowser) return null;
  return (
    <div style={{ position: 'relative' }} onMouseEnter={() => setTip(true)} onMouseLeave={() => setTip(false)}>
      <Globe17 label={`Show the browser, ${s.tabs.length} tabs, Codex is using one`} badge={using} onClick={() => setUi({ open: true })} />
      {tip && <span className="tip-under" role="tooltip">Show the browser<kbd>⇧⌘B</kbd></span>}
    </div>
  );
}

const GlobeOpen = () => {
  const { setUi } = useLab();
  return <Globe17 on label="Hide the browser" onClick={() => setUi({ open: false, full: false })} />;
};

/** The sign-up page the active tab shows, and the agent's pointer over it. */
function SignupPage({ tab }: { tab: Tab }) {
  if (tab.title.startsWith('Workspace')) return <Site.Members />;
  if (tab.title.startsWith('Sign up')) return <Site.SignUp />;
  return <Site.SignIn focus={tab.holder === 'you'} />;
}

export function Signup({ signedUp = true, chips = true }: { signedUp?: boolean; chips?: boolean }) {
  const { ui, setUi, active } = useLab();
  if (!active) return null;
  const held = active.holder === 'you';
  const pageLabel = active.title.startsWith('Workspace') ? 'Acme’s members page, Codex clicking Invite' : held ? 'Acme’s sign-in page' : 'The page Codex is on: Acme’s sign-in, filled in';
  if (ui.open && ui.full) {
    return (
      <Window width={1280} height={800}>
        <Panel full label="Browser, full width">
          <Tabs full />
          <Nav tab={active} />
          <Control tab={active} />
          <View tab={active} label={pageLabel} page={<SignupPage tab={active} />}>
            {active.pointer && <Pointer tab={active} x={active.pointer.x} y={active.pointer.y} ring={active.pointer.ring} labelStyle={{ left: 'auto', right: 6 }} />}
            <div className="peek" role="group" aria-label="The conversation, while the browser is full width">
              <Logo agent="codex" size={16} />
              <span className="peek-text"><b>Sign-up flow works end to end</b> · Signed up and in; now inviting a teammate</span>
              <button className="tc-option is-sm" onClick={() => setUi({ full: false })}>Show the conversation</button>
            </div>
          </View>
        </Panel>
      </Window>
    );
  }
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <Bar crumb="Sign-up flow on staging" project="acme-web" end={ui.open ? <GlobeOpen /> : <GlobeClosed />} />
        <Chat>
          <SignupThread signedUp={signedUp} watch={ui.open ? undefined : () => setUi({ open: true })} />
          <Composer chips={chips} />
        </Chat>
      </div>
      {ui.open && (
        <Panel width={640} resize={!held}>
          <Tabs />
          <Nav tab={active} />
          <Control tab={active} />
          <View tab={active} label={pageLabel} page={<SignupPage tab={active} />}>
            {held ? <YouPointer x={380} y={268} /> : active.pointer && <Pointer tab={active} x={active.pointer.x} y={active.pointer.y} ring={active.pointer.ring} />}
          </View>
        </Panel>
      )}
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 3: the side panel and the browser together, several agents' tabs


/** A conversation's row in the side panel, as Sidebar.tsx draws it (its nest, twist, mark, kids). */
function ConvoRow({ name, about, ago, mark, active, open, kids }: { name: string; about: string; ago: string; mark: 'working' | 'needs' | 'held' | 'done'; active?: boolean; open?: boolean; kids?: React.ReactNode }) {
  return (
    <div className={`convo-nest${kids || open !== undefined ? ' has-kids' : ''}`}>
      {open !== undefined && <button className="convo-twist" aria-expanded={open}>{open ? <ChevronDown size={10} strokeWidth={2} /> : <ChevronRight size={10} strokeWidth={2} />}</button>}
      <div role="button" tabIndex={0} className={`connector task convo-row${active ? ' is-active' : ''}`}>
        <span className="connector-text"><span className="connector-name">{name}</span><span className="connector-about">{about}</span></span>
        <span className="sidebar-item-ago">{ago}</span>
        {mark === 'done' ? <span className="convo-done" role="img" aria-label="Latest task: Done"><Check size={11} strokeWidth={2.2} /></span> : <span className={`status-dot is-${mark}`} role="img" />}
      </div>
      {kids && <div className="convo-kids" role="group">{kids}</div>}
    </div>
  );
}
const Kid = ({ agent, text, what, mark }: { agent: 'claude' | 'codex' | 'cursor'; text: string; what: string; mark: 'working' | 'needs' }) => (
  <button className="convo-kid"><Logo agent={agent} size={16} /><span className="convo-kid-text">{text} <em>· {what}</em></span><Status dot={mark} /></button>
);

function Sidebar() {
  return (
    <div className="sidebar-slot"><nav className="sidebar" aria-label="Side panel">
      <div className="rail">
        <button className="rail-button is-on has-dot is-needs" aria-label="Chats"><ChatsCircleIcon size={17} /></button>
        <span className="rail-sep" role="separator" />
        <button className="rail-button" aria-label="Acme, an Obysphere"><span className="sphere-tile">A</span></button>
        <button className="rail-button" aria-label="Create an Obysphere"><Plus size={20} strokeWidth={1.4} /></button>
        <button className="rail-button rail-settings" aria-label="Settings"><GearSixIcon size={20} /></button>
      </div>
      <div className="sidebar-content">
        <div className="sidebar-top"><button className="icon-button" aria-label="Search"><Search size={17} strokeWidth={1.6} /></button></div>
        <div className="sidebar-title-row"><h2 className="sidebar-title"><button className="sidebar-title-button" aria-haspopup="menu">Default<ChevronDown size={16} strokeWidth={1.6} /></button></h2></div>
        <button className="connector chat-own"><ObyFace size={32} /><span className="connector-text"><span className="connector-name">Oby</span><span className="connector-about">Three agents on it. You can watch them in the browser.</span></span></button>
        <button className="sidebar-new chat-new"><Plus size={16} strokeWidth={1.6} />New conversation</button>
        <div className="sidebar-scroll"><div className="sidebar-list">
          <section className="sidebar-group">
            <button className="sidebar-group-head"><Folder size={14} strokeWidth={1.6} /><span style={{ marginLeft: 2 }}>acme-web</span><ChevronDown size={12} strokeWidth={1.6} /></button>
            <div className="sidebar-group-body">
              <ConvoRow name="Launch checklist" about="3 agents working" ago="1m" mark="working" active open kids={<>
                <Kid agent="claude" text="Pricing page matches the plans" what="in the browser" mark="working" />
                <Kid agent="codex" text="Sign-up flow works end to end" what="in the browser" mark="working" />
                <Kid agent="cursor" text="Button stories look right" what="in the browser" mark="working" />
              </>} />
              <ConvoRow name="Dark mode for the dashboard" about="Done. The PR is open, checks are green." ago="2h" mark="done" open={false} />
            </div>
          </section>
          <section className="sidebar-group">
            <button className="sidebar-group-head"><Folder size={14} strokeWidth={1.6} /><span style={{ marginLeft: 2 }}>acme-api</span><ChevronDown size={12} strokeWidth={1.6} /></button>
            <div className="sidebar-group-body">
              <ConvoRow name="Webhook retries" about="Codex asks to run a command" ago="14m" mark="needs" open={false} />
              <ConvoRow name="Invite links expire too soon" about="No. Old links keep their expiry; only new links change." ago="6h" mark="done" open={false} />
            </div>
          </section>
        </div></div>
        <div className="sidebar-archived"><span>Archived</span><span style={{ fontSize: 12 }}>11</span></div>
      </div>
    </nav></div>
  );
}

export function Together() {
  const { active } = useLab();
  if (!active) return null;
  return (
    <Window width={1440} height={900} sidebar={362}>
      <Sidebar />
      <div className="home">
        <Bar crumb="Launch checklist" project="acme-web" bare={false} end={<GlobeOpen />} />
        <Chat>
          <Thread>
            <You time="3:02 PM">Before launch: check sign-up, the pricing page and the button stories.</You>
            <Oby time="3:02 PM">Three agents on it. You can watch them in the browser.</Oby>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 12, gap: 8 }}>
              <TaskCard agent="claude" title="Pricing page matches the plans" meta="Running · Claude Code · acme-web" progress={['current', '']} dot="working" />
              <TaskCard agent="codex" title="Sign-up flow works end to end" meta="Running · Codex · acme-web" progress={['done', 'current', '']} dot="working" />
              <TaskCard agent="cursor" title="Button stories look right" meta="Running · Cursor · acme-web" progress={['current', '']} dot="working" />
              <Log style={{ marginTop: 4 }}><LogRow mark="working">Claude Code: <Sheen>Reading the Plans section</Sheen></LogRow></Log>
            </div>
          </Thread>
          <Composer />
        </Chat>
      </div>
      <Panel width={600}>
        <Tabs fullWidth={false} />
        <Nav tab={active} />
        <Control tab={active} />
        <View tab={active} label="Acme’s pricing page, which Claude Code is reading" page={<Site.Pricing />}>
          {active.pointer && <Pointer tab={active} x={active.pointer.x} y={active.pointer.y} />}
        </View>
      </Panel>
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 4: a narrow window, the conversation keeps 420px

export function Narrow() {
  const { active, ui } = useLab();
  if (!active) return null;
  return (
    <Window width={994} height={670}>
      <div className="home">
        <Bar crumb="Sign-up flow on staging" project="acme-web" crumbStyle={{ maxWidth: 170, overflow: 'hidden', WebkitMaskImage: 'var(--fade-out)', maskImage: 'var(--fade-out)' }} end={<GlobeOpen />} />
        <Chat>
          <Thread>
            <Oby time="2:14 PM">On it. Codex is signing up a test account in the browser now.</Oby>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 12 }}><div className="msg-task-work">
              <TaskCard agent="codex" title="Sign-up flow works end to end" meta="Running · Codex · acme-web" progress={['current', '', '']} dot="working" />
              <Log>
                <LogRow mark="check">Opened <code>localhost:3000/signup</code></LogRow>
                <LogRow mark="working"><Sheen>Typing in Email</Sheen></LogRow>
              </Log>
            </div></div>
          </Thread>
          <Composer />
        </Chat>
      </div>
      {ui.open && (
        <Panel width={574} resize={false}>
          <Tabs />
          <Nav tab={active} set="three" back={false} />
          <Control tab={active} />
          <View tab={active} label="Acme’s sign-up page, Codex typing the email" page={<Site.SignUp />}>
            {active.pointer && <Pointer tab={active} x={active.pointer.x} y={active.pointer.y} />}
          </View>
        </Panel>
      )}
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 7, 8: the agent asks you to sign in; the password card fills the page's field

export function Asks7() {
  const { active, ui, setUi, api, s, mode } = useLab();
  if (!active) return null;
  const ask = s.asks.find((a) => a.tab === active.id);
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <Bar crumb="Invite emails on staging" project="acme-web" end={<GlobeOpen />} />
        <Chat>
          <Thread>
            <You time="4:40 PM">Are invite emails going out on staging? Check the admin’s mail log.</You>
            <Oby time="4:40 PM">Codex is opening the admin now.</Oby>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 12 }}><div className="msg-task-work">
              <TaskCard agent="codex" title="Check the invite mail log" meta={ask || ui.card ? 'Needs you · Codex · acme-web' : 'Running · Codex · acme-web'} progress={[ask || ui.card ? 'needs' : 'current', '']} dot={ask || ui.card ? 'needs' : 'working'} />
              <Log>
                <LogRow mark="check">Opened <code>staging.acme.dev/admin</code> in the browser</LogRow>
                <LogRow mark="check">Typed <code>admin@example.com</code> in Email</LogRow>
                {ask || ui.card ? <LogRow mark="needs">Asked you to sign in</LogRow> : <LogRow mark="working"><Sheen>Clicking Sign in</Sheen></LogRow>}
              </Log>
            </div></div>
          </Thread>
          {(ask || ui.card) && (
            <div className="decision-bar">
              {ui.card === 'password'
                ? <PasswordCard agent="codex" host={ask?.host ?? 'staging.acme.dev'} preview={ui.typed} onCancel={() => { setUi({ card: null, typed: 0 }); if (active.holder === 'you') void api.giveBack(active.id); }} onFill={(v) => { void api.secret(active.id, ask?.field ?? 'Password', v); }} />
                : <SignInCard agent="codex" onTake={() => api.take(active.id)} onType={() => { setUi({ card: 'password' }); if (mode === 'live') void api.take(active.id); }} onSkip={() => undefined} />}
            </div>
          )}
          <Composer />
        </Chat>
      </div>
      <Panel width={640} resize={false}>
        <Tabs />
        <Nav tab={active} />
        <Control tab={active} />
        <View tab={active} label={ui.card === 'password' ? 'The staging admin’s sign-in page, its Password field marked' : 'The staging admin’s sign-in page'} page={<Site.AdminSignIn tab={active.id} />} />
      </Panel>
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 9: every ask, and who has the tab (a sheet of pieces)

export function Asks9() {
  return (
    <div className="lab-win sheet" style={{ width: 1280, height: 800 }} data-lab-window>
      <h1 className="sheet-title">What the agent asks, over the message box</h1>
      <p className="sheet-sub">The decision bar’s card (DecisionBar), one at a time. The orange kind word is the app’s; each card’s words are proposals.</p>
      <div className="sheet-grid" style={{ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' }}>
        <div className="cell"><span className="cell-name">A CAPTCHA <em>· only a person answers it</em></span>
          <Card label="Codex is asking" kind="Check" head="The page wants to know you’re a person" from="Codex"><Text>Codex can’t answer this. Take control and solve it; Codex goes on when you give the tab back.</Text><Options><button className="tc-option is-primary">Take control</button><button className="tc-option is-quiet">Skip this page</button></Options></Card></div>
        <div className="cell"><span className="cell-name">A choice <em>· the agent asks, you pick</em></span>
          <Card label="Claude Code is asking" kind="Question" head="Which plan should I pick at checkout?" from="Claude Code"><Text>The test card works with both. Business adds SSO, which the next step checks.</Text><Options><button className="tc-option is-primary">Team · $12</button><button className="tc-option">Business · $29</button><button className="tc-option is-quiet">I’ll choose in the page</button></Options></Card></div>
        <div className="cell"><span className="cell-name">A one-time code <em>· typed straight into the page</em></span>
          <Card label="A code for the page" kind="Code" blue head={<>The code sent to your phone, for <b style={{ fontWeight: 500 }}>staging.acme.dev</b></>}>
            <div className="secret-row"><span className="code-boxes" aria-label="Six-digit code"><span>4</span><span>8</span><span>1</span><span className="is-on" /><span /><span /></span><button className="tc-option is-primary" style={{ marginLeft: 'auto' }}>Fill in</button><button className="tc-option is-quiet">Cancel</button></div>
            <Note>It goes straight into the page. Codex never sees it.</Note>
          </Card></div>
        <div className="cell"><span className="cell-name">A passkey or a card check <em>· yours alone</em></span>
          <Card label="Codex is asking" kind="Passkey" head="staging.acme.dev asks for your passkey" from="Codex"><Text>Only you can do this. Take control; your Mac asks for Touch ID.</Text><Options><button className="tc-option is-primary">Take control</button><button className="tc-option is-quiet">Skip</button></Options></Card></div>
      </div>
      <div className="sheet-grid" style={{ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', marginTop: 4 }}>
        <div className="cell"><span className="cell-name">Who has the tab <em>· the strip under the address</em></span>
          <div className="strip-box">
            <Strip kind="using" agent="codex" action="Clicking Sign in" />
            <Strip kind="asks" agent="codex" />
            <Strip kind="held" agent="codex" />
            <Strip kind="yours" />
            <Strip kind="idle" agent="claude" />
          </div></div>
        <div className="cell"><span className="cell-name">In the side panel <em>· the row’s line, as today’s asks</em></span>
          <div className="strip-box" style={{ padding: 8, background: 'var(--bg)' }}>
            <ConvoRow name="Invite emails on staging" about="Codex asks you to sign in" ago="1m" mark="needs" open kids={<Kid agent="codex" text="Check the invite mail log" what="asks you to sign in" mark="needs" />} />
            <ConvoRow name="Sign-up flow on staging" about="You have Codex’s tab" ago="now" mark="held" open />
          </div>
          <p className="set-foot" style={{ padding: '0 4px' }}>A Mac notification says the same line when the window isn’t in front, as today’s asks do.</p>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------------------------
// 10: page states, each in a small browser with the log's line under it

function Mini({ tab, children, nav }: { tab: Tab; children: React.ReactNode; nav?: React.ReactNode }) {
  return <div className="mini" data-lab={`state-${tab.state}`}><div className="br">{nav ?? <Nav tab={tab} set="mini" warn={tab.state === 'certificate'} />}{children}</div></div>;
}

export function PageStates() {
  const { s, api } = useLab();
  const by = (state: Tab['state']) => s.tabs.find((t) => t.state === state)!;
  const loading = by('loading'); const failed = by('failed'); const timed = by('timedOut'); const dialog = by('dialog'); const cert = by('certificate'); const crashed = by('crashed');
  const [retried, setRetried] = useState<string | null>(null);
  const told = (children: React.ReactNode) => <p className="told">{children}</p>;
  const danger = (t: string) => <span style={{ color: 'var(--danger)' }}>{t}</span>;
  return (
    <div className="lab-win sheet" style={{ width: 1280, height: 880 }} data-lab-window>
      <h1 className="sheet-title">When a page doesn’t simply load</h1>
      <p className="sheet-sub">Each in the browser panel, with the line the agent’s log shows under it. The agent is told every time; a page’s question waits for you.</p>
      <div className="sheet-grid" style={{ gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
        <div className="cell"><span className="cell-name">Loading</span>
          <Mini tab={loading} nav={<Nav tab={loading} set="loading"><span className="br-loading" style={{ left: 8 }} /></Nav>}>
            <Control tab={loading} />
            <div className="br-view" style={{ background: '#fff' }}><Site.Bone w="40%" top={22} h={14} /><Site.Bone w="70%" /><Site.Bone w="60%" /><Site.Bone w="66%" /></div>
          </Mini>
          {told(<>Log: <Sheen>Opening localhost:3000/dashboard</Sheen></>)}
        </div>
        <div className="cell"><span className="cell-name">Failed to load</span>
          <Mini tab={failed}><div className="br-view is-blank"><div className="br-state">
            <WifiOffIcon />
            <span className="br-state-title">This page didn’t load</span>
            <span className="br-state-text">localhost:3000 refused to connect. Is the app running?</span>
            <div className="tc-options"><button className="tc-option is-sm" onClick={() => setRetried('failed')}>Try again</button></div>
          </div></div></Mini>
          {told(<>Log: ✕ Open localhost:3000/dashboard · {danger('refused to connect')}</>)}
        </div>
        <div className="cell"><span className="cell-name">Timed out</span>
          <Mini tab={timed}><div className="br-view is-blank"><div className="br-state">
            <ClockAlertIcon />
            <span className="br-state-title">The page took too long</span>
            <span className="br-state-text">Nothing came back from api.acme.dev in 30 seconds.</span>
            <div className="tc-options"><button className="tc-option is-sm" onClick={() => setRetried('timedOut')}>Try again</button></div>
          </div></div></Mini>
          {told(<>Log: ✕ Open api.acme.dev/health · {danger('timed out after 30s')}</>)}
        </div>
        <div className="cell"><span className="cell-name">The page asks something <em>· a JavaScript dialog</em></span>
          <Mini tab={dialog}><div className={`br-view${dialog.holder === 'you' ? ' is-held' : ''}`}>
            <Site.Settings />
            <div className="br-over" role="dialog" aria-label="The page asks">
              <span className="br-over-kind">localhost:3000 asks</span>
              <p className="br-over-text">Leave this page? Changes you made may not be saved.</p>
              <div className="tc-options" style={{ marginTop: 10 }}>
                <button className="tc-option is-sm" disabled={dialog.holder !== 'you'} style={dialog.holder === 'you' ? undefined : { opacity: 0.4 }}>Leave</button>
                <button className="tc-option is-sm" disabled={dialog.holder !== 'you'} style={dialog.holder === 'you' ? undefined : { opacity: 0.4 }}>Stay</button>
                {dialog.holder !== 'you' && <button className="tc-option is-sm is-primary" style={{ marginLeft: 'auto' }} onClick={() => api.take(dialog.id)}>Take control to answer</button>}
              </div>
            </div>
          </div></Mini>
          {told(<>Log: <span style={{ color: 'var(--needs)' }}>●</span> The page asked “Leave this page?”, waiting for you</>)}
        </div>
        <div className="cell"><span className="cell-name">Certificate warning</span>
          <Mini tab={cert}><div className="br-view is-blank"><div className="br-state">
            <ShieldAlertIcon />
            <span className="br-state-title">This connection isn’t private</span>
            <span className="br-state-text">The certificate for staging.acme.dev isn’t trusted. Codex stopped here and won’t go on by itself.</span>
            <div className="tc-options"><button className="tc-option is-sm is-primary">Go back</button><button className="tc-option is-sm is-quiet" style={{ color: 'var(--danger)' }}>Go on anyway</button></div>
          </div></div></Mini>
          {told(<>Log: <span style={{ color: 'var(--needs)' }}>●</span> Stopped at an untrusted certificate, waiting for you</>)}
        </div>
        <div className="cell"><span className="cell-name">The page crashed</span>
          <Mini tab={crashed}><div className="br-view is-blank"><div className="br-state">
            <BugIcon />
            <span className="br-state-title">This page stopped working</span>
            <span className="br-state-text">Its tab crashed, out of memory. Cursor was told and can open it again.</span>
            <div className="tc-options"><button className="tc-option is-sm">Reload</button></div>
          </div></div></Mini>
          {told(<>Log: ✕ localhost:5173/reports · {danger('the tab crashed')}</>)}
        </div>
      </div>
      <span hidden data-lab="retried">{retried ?? ''}</span>
    </div>
  );
}
const WifiOffIcon = () => <WifiOff size={22} strokeWidth={1.6} />;
const ClockAlertIcon = () => <ClockAlert size={22} strokeWidth={1.6} />;
const ShieldAlertIcon = () => <ShieldAlert size={22} strokeWidth={1.6} />;
const BugIcon = () => <Bug size={22} strokeWidth={1.6} />;

// ---------------------------------------------------------------------------------------------
// 11: a script in the page, and its result as outside text

export function Script() {
  const { active } = useLab();
  if (!active) return null;
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <Bar crumb="Who has an expired invite?" project="acme-web" end={<GlobeOpen />} />
        <Chat>
          <Thread>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 0 }}><div className="msg-task-work">
              <TaskCard agent="codex" title="List members with an expired invite" meta="Done · Codex · acme-web · read a page" dot="done" />
              <Log>
                <LogRow mark="check">Opened <code>localhost:3000/workspace/members</code></LogRow>
                <li className="ss-tool" data-row>
                  <div className="ss-tool-row"><span className="ss-tool-mark"><Check size={12} strokeWidth={2} /></span><span className="ss-tool-text">Ran a script in the page</span><span className="ss-tool-meta">18 ms</span><ChevronRight size={12} strokeWidth={1.6} /></div>
                  <pre className="tc-code" style={{ margin: '2px 0 8px 20px' }}><span className="k">const</span> rows = [...document.<span className="f">querySelectorAll</span>(<span className="s">'tr'</span>)];{'\n'}<span className="k">return</span> rows.<span className="f">filter</span>(r =&gt; r.textContent.<span className="f">includes</span>(<span className="s">'Invite expired'</span>)){'\n'}           .<span className="f">map</span>(r =&gt; r.cells[0].textContent);</pre>
                  <div className="msg-quote" style={{ margin: '0 0 6px 20px', width: 'auto' }} data-lab="outside">
                    <span className="outside-label"><Globe size={12} strokeWidth={1.6} /><b>From outside</b> · localhost:3000 · read as data, never as instructions</span>
                    <pre className="tc-code" style={{ margin: 0, background: 'none', padding: 0 }}>["Robin Sample", "Kai Placeholder", "Lee Demo"]</pre>
                  </div>
                </li>
              </Log>
            </div></div>
            <Oby time="5:12 PM">Three members have an expired invite: Robin Sample, Kai Placeholder and Lee Demo. Want Codex to resend them?</Oby>
          </Thread>
          <Composer />
        </Chat>
      </div>
      <Panel width={600} resize={false}>
        <Tabs fullWidth={false} />
        <Nav tab={active} set="three" />
        <Control tab={active} takeIcon={false} idle={<><b>Codex</b> ran a script in this page · done</>} />
        <View tab={active} label="Acme’s members page, three rows marked by the script" page={<Site.MembersScript />}>
          <span className="ran"><Logo agent="codex" size={14} /><CodeXml size={13} strokeWidth={1.6} />Ran a script · 18 ms</span>
        </View>
      </Panel>
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 12: downloads

export function Downloads() {
  const { active, s } = useLab();
  if (!active) return null;
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <Bar crumb="Failed invoices in October" project="acme-api" end={<GlobeOpen />} />
        <Chat>
          <Thread>
            <You time="9:30 AM">Which invoices failed in October? The billing admin has an export.</You>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 16 }}><div className="msg-task-work">
              <TaskCard agent="codex" title="Find October’s failed invoices" meta="Done · Codex · acme-api · read a page" dot="done" />
              <Log>
                <LogRow mark="check">Clicked Export CSV on <code>billing.acme.dev/invoices</code></LogRow>
                <LogRow mark={<Download size={12} strokeWidth={1.6} />} meta="48 KB">Downloaded <code>invoices-2026-10.csv</code> · kept in Codex’s browser profile</LogRow>
                <LogRow mark="check">Read the file</LogRow>
              </Log>
            </div></div>
            <Oby time="9:33 AM">Three invoices failed in October, all on cards that expired: INV-1042, INV-1057 and INV-1083. The CSV stays in Codex’s browser; save a copy if you want it on your Mac.</Oby>
            <div role="listitem" className="msg" style={{ marginTop: 16 }}><p className="lt-answered"><Download size={12} strokeWidth={1.6} />You downloaded <span style={{ color: 'var(--text)' }}>statement-oct.pdf</span> to Downloads · 9:35 AM</p></div>
          </Thread>
          <Composer />
        </Chat>
      </div>
      <Panel width={600} resize={false}>
        <Tabs fullWidth={false} />
        <Nav tab={active} set="three" />
        <Control tab={active} takeIcon={false} />
        <View tab={active} label="Acme Billing’s invoices page" page={<Site.Invoices />} />
        <div className="br-shelf" aria-label="Downloads">{s.downloads.map((d) => <FileRow key={d.id} id={d.id} />)}</div>
      </Panel>
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 13: Settings › General, two switches

export function SettingsBrowser() {
  const { s, api } = useLab();
  const sw = (on: boolean, label: string, flip: () => void) => <button className="set-switch" role="switch" aria-checked={on} aria-label={label} onClick={flip} />;
  return (
    <Window width={1200} height={860} sidebar={362}>
      <div className="sidebar-slot"><nav className="sidebar" aria-label="Side panel">
        <div className="rail">
          <button className="rail-button" aria-label="Chats"><ChatsCircleIcon size={17} /></button>
          <span className="rail-sep" role="separator" />
          <button className="rail-button" aria-label="Acme, an Obysphere"><span className="sphere-tile">A</span></button>
          <button className="rail-button" aria-label="Create an Obysphere"><Plus size={20} strokeWidth={1.4} /></button>
          <button className="rail-button rail-settings is-on" aria-label="Settings" aria-pressed="true"><GearSixIcon size={20} weight="fill" /></button>
        </div>
        <div className="sidebar-content">
          <div className="sidebar-title-row"><h2 className="sidebar-title">Settings</h2></div>
          {['General', 'Accounts', 'Obysphere', 'Connectors', 'Repositories', 'Routines', 'About'].map((n) => <button key={n} className={`sidebar-item${n === 'General' ? ' is-active' : ''}`}>{n}</button>)}
        </div>
      </nav></div>
      <div className="home">
        <div className="bar"><span className="crumb is-here">General</span></div>
        <div className="group-layer"><section className="group" aria-label="General"><div className="group-column set-column">
          <h1 className="group-title">General</h1>
          <section className="set-group"><div className="set-card"><div className="set-row"><span className="set-label">Open at login</span>{sw(true, 'Open at login', () => undefined)}</div></div></section>
          <section className="set-group">
            <h2 className="set-head">Permissions on this Mac</h2>
            <div className="set-card">
              <div className="set-row"><span className="set-label">Default mode<span className="set-about">Asks nothing</span></span><span className="set-select" style={{ width: 130 }}>Bypass<ChevronDown size={12} strokeWidth={1.8} /></span></div>
              <div className="set-row"><span className="set-label"><span className="tok-code" style={{ alignSelf: 'flex-start' }}>Bash(npm test:*)</span><span className="set-about">Allowed · acme/web</span></span><button className="set-button is-danger">Remove</button></div>
            </div>
            <p className="set-foot">Default mode sets how often your agents check with you before they edit files or run commands. To change it for one task, use that task’s Mode menu. Always allow adds a rule here; remove it to be asked again.</p>
          </section>
          <section className="set-group is-new-group">
            <h2 className="set-head">Browser<span className="new-tag">New</span></h2>
            <div className="set-card">
              <div className="set-row"><span className="set-label">Agents can use the browser<span className="set-about">They open pages, click and type; you watch beside the conversation and take control any time.</span></span>{sw(s.settings.agentsUseBrowser, 'Agents can use the browser', () => api.setSettings({ agentsUseBrowser: !s.settings.agentsUseBrowser }))}</div>
              <div className="set-row"><span className="set-label">Agents can use this Mac<span className="set-about">Other apps too, through agent-desktop. macOS asks you once.</span></span>{sw(s.settings.agentsUseMac, 'Agents can use this Mac', () => api.setSettings({ agentsUseMac: !s.settings.agentsUseMac }))}</div>
            </div>
            <p className="set-foot">A password you type in Oby goes straight to the page. No agent sees it.</p>
          </section>
        </div></section></div>
      </div>
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 14: another computer

export function OtherComputer() {
  const { active } = useLab();
  if (!active) return null;
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <Bar crumb="Dashboard on devbox" project="acme-web" machine={<button className="project-chip is-machine" aria-label="Computer: devbox, Linux. Switch computer"><Server size={14} strokeWidth={1.6} /><b>devbox</b> · Linux<ChevronDown size={12} strokeWidth={1.8} /></button>} end={<GlobeOpen />} />
        <Chat>
          <Thread>
            <You time="11:05 AM">Start the dev server on devbox and check the dashboard loads.</You>
            <Oby time="11:05 AM">Claude Code is on it, on devbox.</Oby>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 12 }}><div className="msg-task-work">
              <TaskCard agent="claude" title="Dashboard loads on devbox" meta="Running · Claude Code · acme-web · devbox" progress={['done', 'current']} dot="working" />
              <Log>
                <LogRow mark="check" meta="6s">Ran <code>npm run dev</code> on devbox</LogRow>
                <LogRow mark="check">Opened <code>localhost:3000</code> on devbox in the browser</LogRow>
                <LogRow mark="working"><Sheen>Reading the dashboard</Sheen></LogRow>
              </Log>
            </div></div>
          </Thread>
          <Composer />
        </Chat>
      </div>
      <Panel width={620} label="Browser on devbox" resize={false}>
        <Tabs fullWidth={false} newTabLabel={`New tab on ${active.computer?.name}`} />
        <div className="where-bar" data-lab="where"><Server size={13} strokeWidth={1.6} /><span>This browser runs on <b>{active.computer?.name}</b>. Its localhost is {active.computer?.name}’s, not this Mac’s.</span></div>
        <Nav tab={active} set="three" />
        <Control tab={active} takeIcon={false} />
        <View tab={active} label="The dashboard running on devbox" className="is-dark-page" page={<Site.Dashboard />} />
      </Panel>
    </Window>
  );
}

// ---------------------------------------------------------------------------------------------
// 17: first use, no pages yet

export function Empty() {
  const { ui, setUi } = useLab();
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <div className="bar is-bare"><span className="crumb is-here">New conversation</span><div className="bar-end"><GlobeOpen /></div></div>
        <Chat>
          <Thread><Oby time="10:02 AM">What should we work on?</Oby></Thread>
          <Composer placeholder="Message Oby…" />
        </Chat>
      </div>
      <Panel width={600} label="Browser, no pages yet" resize={false}>
        <Tabs fullWidth={false} />
        <Nav set="off" />
        <div className="br-view is-blank" style={{ borderTop: '1px solid var(--divider)' }} data-lab="view">
          {!ui.tipClosed && (
            <div className="br-tip" role="note">
              <Globe size={15} strokeWidth={1.6} />
              <p>Your agents can use this browser by themselves. You watch here and take control any time; passwords you type go straight to the page, never to them. Settings › General turns it off.</p>
              <button className="icon-button is-28" aria-label="Close" onClick={() => setUi({ tipClosed: true })}><X size={13} strokeWidth={2} /></button>
            </div>
          )}
          <p className="empty-words">No pages yet.{'\n'}When an agent opens one, you see it here, live.</p>
        </div>
      </Panel>
    </Window>
  );
}

// 15 and 16 (the Mac) are in desktop.tsx.
