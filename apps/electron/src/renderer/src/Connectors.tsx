import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Copy, Search, X } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { Group, Row, Select, useSetting } from './SettingsView';
import { Spinner } from './SetupCard';
import { onObid } from './lean';
import { api, type GitHub, type Tetrix, type TetrixTarget } from './obid';
import { say, useLive } from './live';

// Settings › Connectors: what's connected, then what's available, with a search over both. On obid
// (lean.ts) the ones obid has, as they stand; elsewhere Sangam's demo list.
export default function Connectors() {
  return onObid ? <Live /> : <Mock />;
}

// A row: its logo, name and line, and its buttons (Disconnect, or what it needs)
type Connector = { name: string; about: string; brand: Brand; connected: boolean; control: React.ReactNode };

function List({ connectors }: { connectors: Connector[] }) {
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  const shown = connectors.filter((c) => !q || `${c.name} ${c.about}`.toLowerCase().includes(q));
  const groups = [
    { title: 'Connected', items: shown.filter((c) => c.connected) },
    { title: 'Available', items: shown.filter((c) => !c.connected) },
  ].filter((g) => g.items.length);
  return <>
    <label className="archive-search set-search rise" style={{ '--i': 1 } as React.CSSProperties}>
      <Search size={16} strokeWidth={1.6} />
      <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search connectors" aria-label="Search connectors" />
    </label>
    {groups.map((g, k) => (
      <Group key={g.title} i={2 + k} title={g.title}>
        {g.items.map((c) => (
          <Row key={c.name} label={<span className="set-connector"><BrandMark brand={c.brand} /><span className="set-label">{c.name}<span className="set-about">{c.about}</span></span></span>}>
            {c.control}
          </Row>
        ))}
      </Group>
    ))}
    {!groups.length && <p className="group-empty">No connector matches “{query.trim()}”</p>}
  </>;
}

// its group says it's connected: the row has the way out, Connect's width (app.css, .set-connect)
const disconnectButton = (onClick?: () => void) => <button className="set-button set-connect" disabled={!onClick} onClick={onClick}>Disconnect</button>;

// The services Oby reaches (placeholders until these are real)
const CONNECTORS: { name: string; about: string; brand: Brand; connected: boolean }[] = [
  { name: 'Deskree Ops', about: 'Projects, deploys and incidents', brand: BRANDS.deskree, connected: true },
  { name: 'Tetrix', about: 'Project knowledge and blast radius', brand: BRANDS.tetrix, connected: true },
  { name: 'GitHub', about: 'Repos, pull requests and checks', brand: BRANDS.github, connected: true },
  { name: 'Claude', about: 'Run jobs in Claude Code', brand: BRANDS.claude, connected: true },
  { name: 'ChatGPT', about: 'Run jobs in Codex, ask ChatGPT', brand: BRANDS.openai, connected: true },
  { name: 'Cursor', about: 'Run jobs in Cursor', brand: BRANDS.cursor, connected: false },
  { name: 'Slack', about: 'Read and send messages', brand: BRANDS.slack, connected: true },
  { name: 'Gmail', about: 'Read and manage your email', brand: BRANDS.gmail, connected: false },
  { name: 'Google Meet', about: 'Join meetings and keep the notes', brand: BRANDS.meet, connected: true },
  { name: 'Zoom', about: 'Join meetings and keep the notes', brand: BRANDS.zoom, connected: false },
];
function Mock() {
  const [names, setNames] = useSetting('connected', CONNECTORS.filter((c) => c.connected).map((c) => c.name));
  return <List connectors={CONNECTORS.map((c) => ({
    ...c, connected: names.includes(c.name),
    control: names.includes(c.name)
      ? disconnectButton(() => setNames(names.filter((n) => n !== c.name)))
      : <button className="set-button set-connect" onClick={() => setNames([...names, c.name])}>Connect</button>,
  }))} />;
}

// On obid: GitHub (obid's gh, with GitHub's own device sign-in) and Tetrix (obid is its OAuth
// client), docs/runner-api.md. The launch shows these two only (AGENTS.md); the agents' logins
// are Settings › Agents. Read when the page opens, then every 2 seconds while a sign-in is under
// way (obid sends no event for GitHub's). Nothing here holds a token: obid keeps them.
// Each row is just its name, its line and its buttons; what a connector needs beyond that opens
// in a window of its own: Tetrix's server and sign-in (Connect, Edit), GitHub's way out.
// What obid last said about both, kept: the page opens on it at once and reads again behind it.
// Settings reads it as it opens (SettingsView), as obid's first GitHub answer takes a moment (it
// asks gh: ~600ms; then it's quick)
let last: { github: GitHub; tetrix: Tetrix } | null = null, lastAt = 0;
let reading: Promise<{ github: GitHub; tetrix: Tetrix }> | null = null;
// A read already on its way is shared; fresh (after a change: a Disconnect, a sign-in started) waits
// for it and reads again, so what it shows is from after the change
function readBoth(fresh = false) {
  if (reading && !fresh) return reading;
  const before = reading?.catch(() => {}) ?? Promise.resolve();
  const mine: Promise<{ github: GitHub; tetrix: Tetrix }> = before
    .then(() => Promise.all([api.github(), api.tetrix()]))
    .then(([github, tetrix]) => { lastAt = Date.now(); return (last = { github, tetrix }); })
    .finally(() => { if (reading === mine) reading = null; });
  return (reading = mine);
}
// (as Settings opens: each of its pages is a view of its own, so once in 30s, not at each)
export const readConnectors = () => { if (onObid && Date.now() - lastAt > 30_000) void readBoth().catch(() => { /* the page says why when it opens */ }); };

function Live() {
  const [github, setGitHub] = useState<GitHub | null>(last?.github ?? null);
  const [tetrix, setTetrix] = useState<Tetrix | null>(last?.tetrix ?? null);
  const [problem, setProblem] = useState<string | null>(null);
  // each connector's sign-in on its way, apart (gh takes a few seconds to give its code)
  const [busy, setBusy] = useState({ GitHub: false, Tetrix: false });
  // the window open, if any
  const [open, setOpen] = useState<'tetrix' | 'github-off' | null>(null);
  const repos = useLive().repos.length;
  const read = (fresh = false) => readBoth(fresh).then(({ github: g, tetrix: t }) => { setGitHub(g); setTetrix(t); setProblem(null); }, (e: Error) => setProblem(e.message));
  useEffect(() => { void read(); }, []);
  const signingIn = !!(github?.signIn || tetrix?.signIn);
  useEffect(() => {
    if (!signingIn) return;
    const t = setInterval(read, 2000);
    return () => clearInterval(t);
  }, [signingIn]);
  // only a sign-in's own click clears its busy: a Cancel never puts back a Connect still on its way
  const act = (f: () => Promise<unknown>, signIn?: 'GitHub' | 'Tetrix') => () => {
    if (signIn) setBusy((b) => ({ ...b, [signIn]: true }));
    void f().catch(say).then(() => read(true)).finally(() => { if (signIn) setBusy((b) => ({ ...b, [signIn]: false })); });
  };
  // a sign-in's page, in the person's browser (main opens https only)
  const page = (p: { url: string }) => window.app?.openExternal(p.url);
  if (problem && !github) return <p className="group-empty">{problem}</p>;
  if (!github || !tetrix) return null;

  const g = github, t = tetrix;
  const gitHub: Connector = { name: 'GitHub', brand: BRANDS.github, connected: g.signedIn, about: 'Repos, pull requests and checks', control: null };
  if (g.signIn || busy.GitHub) {
    gitHub.about = `Finish in your browser${at(g.signIn?.url)}${g.signIn?.userCode ? ` · enter ${g.signIn.userCode}` : ''}`;
    gitHub.control = <>
      <button className="set-button set-connect" disabled>Waiting…</button>
      {g.signIn && <button className="set-button set-connect" onClick={act(api.githubCancel)}>Cancel</button>}
    </>;
  } else if (g.signedIn) {
    gitHub.about = `${g.user ?? 'Signed in'}${repos ? ` · ${repos} repositories` : ''}`;
    gitHub.control = disconnectButton(() => setOpen('github-off'));
  } else if (!g.gh) {
    gitHub.about = 'GitHub’s tool, gh, isn’t on this Mac';
  } else {
    gitHub.control = <button className="set-button set-connect" onClick={act(async () => page(await api.githubSignIn()), 'GitHub')}>Connect</button>;
  }

  const tetrixRow: Connector = { name: 'Tetrix', brand: BRANDS.tetrix, connected: t.connected, about: t.error ?? 'Project knowledge and blast radius', control: null };
  if (t.connected) {
    // with an error, still connected: a refresh the sign-in server didn't answer (Edit signs in again)
    if (!t.error) tetrixRow.about = `${serverName(t)} · for all your agents`;
    tetrixRow.control = <>
      <button className={`set-button set-connect${t.error ? '' : ' is-narrow'}`} onClick={() => setOpen('tetrix')}>{t.error ? 'Reconnect' : 'Edit'}</button>
      {disconnectButton(act(api.tetrixDisconnect))}
    </>;
  } else {
    tetrixRow.control = <button className="set-button set-connect" onClick={() => setOpen('tetrix')}>Connect</button>;
  }

  return <>
    <List connectors={[gitHub, tetrixRow]} />
    {open === 'tetrix' && (
      <TetrixWindow tetrix={t} waiting={!!t.signIn || busy.Tetrix} onClose={() => setOpen(null)}
        onConnect={(to) => act(async () => page(await api.tetrixSignIn(to)), 'Tetrix')()}
        onCancel={() => { act(api.tetrixCancel)(); setOpen(null); }} />
    )}
    {/* (closed: read again, in case gh was signed out meanwhile) */}
    {open === 'github-off' && <GitHubOffWindow onClose={() => { setOpen(null); void read(true); }} />}
  </>;
}

// where a sign-in's page is: " at github.com"
function at(url?: string) {
  try { return url ? ` at ${new URL(url).host}` : ''; } catch { return ''; }
}

// what the person calls the Tetrix Oby uses: its preset's name, else its address's host
function serverName(t: Tetrix) {
  const preset = t.presets.find((p) => p.id === t.preset);
  if (preset) return preset.name;
  try { return new URL(t.url ?? '').host; } catch { return 'Tetrix'; }
}

// A window over Settings (a connector's, or asking before a Remove): its logo and title, a ×, Esc
// or a click outside closes it. alert: a question, its lede read out with it (describedBy)
function Window({ brand, title, onClose, alert, children }: { brand: Brand; title: string; onClose: () => void; alert?: string; children: React.ReactNode }) {
  // (its latest onClose, listened for once)
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal is-small" role={alert ? 'alertdialog' : 'dialog'} aria-modal="true" aria-label={title} aria-describedby={alert}>
        <header className="modal-head">
          <span className="connector-window-title"><BrandMark brand={brand} size={22} /><h2>{title}</h2></span>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        {children}
      </div>
    </div>,
    document.body,
  );
}

// Which Tetrix Oby signs in to, and the sign-in: a preset (config.json's tetrixPresets, none by
// default: GET /v1/tetrix's presets), or the owner's own Tetrix's MCP address (obid checks it: HTTPS, plain HTTP
// only on 127.0.0.1). Connecting to another signs Oby in there in place of the one before. Picked,
// Continue opens its page in the browser and this waits, until obid says it's in (then it closes
// itself, and the row is under Connected) or Cancel. Setting up opens it too (SetupCard's
// TetrixRow), so the person gives their Tetrix's address there.
export function TetrixWindow({ tetrix: t, waiting, onConnect, onCancel, onClose }: {
  tetrix: Tetrix; waiting: boolean; onConnect: (to: TetrixTarget) => void; onCancel: () => void; onClose: () => void;
}) {
  // starting from the one Oby uses (or used last), else the first preset
  const [pick, setPick] = useState(t.preset ?? (t.url ? 'url' : t.presets[0]?.id ?? 'url'));
  const [url, setUrl] = useState(t.preset ? '' : t.url ?? '');
  // the server Continue was for: the window closes once obid is signed in there (an Edit's old
  // server being connected already says nothing); a sign-in that didn't take shows the form again
  const [aim, setAim] = useState<TetrixTarget | null>(null);
  const chosen: TetrixTarget | null = pick === 'url' ? (url.trim() ? { url: url.trim() } : null) : { preset: pick };
  const same = t.connected && !t.error && (pick === 'url' ? !t.preset && url.trim() === t.url : pick === t.preset);
  useEffect(() => {
    if (!aim || waiting) return;
    const there = 'preset' in aim ? t.preset === aim.preset : !t.preset && t.url === aim.url;
    if (t.connected && !t.error && there) onClose(); else setAim(null);
  }, [aim, waiting, t.connected, t.error, t.preset, t.url]);
  const server = pick === 'url' ? (() => { try { return new URL(url).host; } catch { return 'the server'; } })() : t.presets.find((p) => p.id === pick)?.name ?? 'Tetrix';
  // waiting: closing it ends the sign-in, once obid has one to end (while it starts, there's none yet)
  return (
    <Window brand={BRANDS.tetrix} title={t.connected ? 'Edit Tetrix' : 'Connect Tetrix'} onClose={waiting ? (t.signIn ? onCancel : () => {}) : onClose}>
      {waiting ? <>
        <p className="connector-wait"><Spinner label="Signing in" />Finish signing in on {server}’s page in your browser.</p>
        <div className="modal-actions"><button type="button" className="set-button" disabled={!t.signIn} onClick={onCancel}>Cancel</button></div>
      </> : <>
        <p className="modal-lede">Oby signs in once, and all your agents can use it.</p>
        <label className="modal-label">Server</label>
        <span className="connector-select"><Select value={pick} onChange={setPick} label="Tetrix server" options={[...t.presets.map((p) => [p.id, p.name] as const), ['url', 'Another server']]} /></span>
        {pick === 'url' && <input className="modal-field connector-address" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://tetrix.example.com/mcp" aria-label="Tetrix server address" autoComplete="off" spellCheck={false} autoFocus />}
        <div className="modal-actions">
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="button" className="modal-primary" disabled={!chosen || same} onClick={() => { if (chosen) { setAim(chosen); onConnect(chosen); } }}>Continue in Browser</button>
        </div>
      </>}
    </Window>
  );
}

// A window with a command to run in Terminal, and its copy button: what Oby can't do itself with
// gh's login (it's the person's own): GitHub's way out here, board columns (GroupView)
export function CommandWindow({ brand, title, text, command, onClose }: { brand: Brand; title: string; text: string; command: string; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const back = useRef(0);
  useEffect(() => () => clearTimeout(back.current), []);
  // (a refused clipboard: nothing changes, the command's there to select)
  const copy = () => void navigator.clipboard.writeText(command).then(() => {
    setCopied(true);
    clearTimeout(back.current);
    back.current = window.setTimeout(() => setCopied(false), 1500);
  }, () => {});
  return (
    <Window brand={brand} title={title} onClose={onClose}>
      <p className="modal-lede">{text}</p>
      <div className="connector-command">
        <code>{command}</code>
        <button type="button" className="icon-button" aria-label={copied ? 'Copied' : 'Copy'} title={copied ? 'Copied' : 'Copy'} onClick={copy}>
          {copied ? <Check size={15} strokeWidth={1.8} /> : <Copy size={15} strokeWidth={1.6} />}
        </button>
      </div>
      <div className="modal-actions"><button type="button" className="set-button" onClick={onClose}>Done</button></div>
    </Window>
  );
}

// A window asking before something that can't be undone here (Settings › Agents' and Accounts'
// Remove): what it does, Cancel (focused, as SphereSettings' confirm), and the red button that does it
export function ConfirmWindow({ brand, title, text, action, onConfirm, onClose }: {
  brand: Brand; title: string; text: string; action: string; onConfirm: () => void; onClose: () => void;
}) {
  const what = useId();
  return (
    <Window brand={brand} title={title} onClose={onClose} alert={what}>
      <p id={what} className="modal-lede">{text}</p>
      <div className="modal-actions">
        <button type="button" className="set-button" autoFocus onClick={onClose}>Cancel</button>
        <button type="button" className="modal-primary is-danger" onClick={() => { onClose(); onConfirm(); }}>{action}</button>
      </div>
    </Window>
  );
}

// GitHub's way out: Oby uses gh's own login on this Mac and never signs it out (it's the
// person's, their terminal's too), so this says how
const GitHubOffWindow = ({ onClose }: { onClose: () => void }) => (
  <CommandWindow brand={BRANDS.github} title="Disconnect GitHub?" command="gh auth logout" onClose={onClose}
    text="Oby uses the GitHub sign-in already on this Mac. To disconnect, sign it out in Terminal. Other apps that use it are signed out too." />
);
