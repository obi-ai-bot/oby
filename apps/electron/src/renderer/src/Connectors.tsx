import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Copy, Search, X } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { Group, Row, useSetting } from './SettingsView';
import { Spinner } from './SetupCard';
import { onObid } from './lean';
import { api, type Connectors as Seen, type GitHub, type Tetrix } from './obid';
import { TetrixAddress, useTetrixAddress } from './TetrixAddress';
import { say, useLive } from './live';
import { signInWhy } from './signInWhy';

// Settings › Connectors: what's connected, then what's available, with a search over both. On obid
// (lean.ts) the ones obid has, as they stand; elsewhere Sangam's demo list.
export default function Connectors() {
  return onObid ? <Live /> : <Mock />;
}

// A row: its logo, name and line, and its buttons (Disconnect, or what it needs)
type Connector = { name: string; about: string; brand: Brand; connected: boolean; control: React.ReactNode; key?: string };

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
          <Row key={c.key ?? c.name} label={<span className="set-connector"><BrandMark brand={c.brand} /><span className="set-label">{c.name}<span className="set-about">{c.about}</span></span></span>}>
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
// are Settings › Accounts. Read when the page opens, then every 2 seconds while a sign-in is under
// way (obid sends no event for GitHub's). Nothing here holds a token: obid keeps them.
// Each row is just its name, its line and its buttons; what a connector needs beyond that opens
// in a window of its own: Tetrix's server and sign-in (Connect, Edit), GitHub's way out.
// What obid last said about both, kept: the page opens on it at once and reads again behind it.
// Settings reads it as it opens (SettingsView), as obid's first GitHub answer takes a moment (it
// asks gh: ~600ms; then it's quick)
type Read = { github: GitHub; tetrix: Tetrix; seen: Seen };
let last: Read | null = null, lastAt = 0;
let reading: Promise<Read> | null = null;
// A read already on its way is shared; fresh (after a change: a Disconnect, a sign-in started) waits
// for it and reads again, so what it shows is from after the change
function readBoth(fresh = false) {
  if (reading && !fresh) return reading;
  const before = reading?.catch(() => {}) ?? Promise.resolve();
  const mine: Promise<Read> = before
    // the owner's own connectors: none from an obid without them (404), as before any was seen
    .then(() => Promise.all([api.github(), api.tetrix(), api.connectors().catch((): Seen => ({ claude: null }))]))
    .then(([github, tetrix, seen]) => { lastAt = Date.now(); return (last = { github, tetrix, seen }); })
    .finally(() => { if (reading === mine) reading = null; });
  return (reading = mine);
}
// (as Settings opens: each of its pages is a view of its own, so once in 30s, not at each)
export const readConnectors = () => { if (onObid && Date.now() - lastAt > 30_000) void readBoth().catch(() => { /* the page says why when it opens */ }); };

function Live() {
  const [github, setGitHub] = useState<GitHub | null>(last?.github ?? null);
  const [tetrix, setTetrix] = useState<Tetrix | null>(last?.tetrix ?? null);
  const [seen, setSeen] = useState<Seen | null>(last?.seen ?? null);
  const [problem, setProblem] = useState<string | null>(null);
  // each connector's sign-in on its way, apart (gh takes a few seconds to give its code)
  const [busy, setBusy] = useState({ GitHub: false, Tetrix: false });
  // the window open, if any; and why Tetrix's last try didn't take (obid's reason, in words), under its field
  const [open, setOpen] = useState<'tetrix' | 'github-off' | null>(null);
  const [said, setSaid] = useState<string | undefined>();
  const repos = useLive().repos.length;
  const read = (fresh = false) => readBoth(fresh).then(({ github: g, tetrix: t, seen: c }) => { setGitHub(g); setTetrix(t); setSeen(c); setProblem(null); }, (e: Error) => setProblem(e.message));
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

  const own = ownRows(seen);
  return <>
    <List connectors={[gitHub, tetrixRow, ...own]} />
    {!own.length && <p className="set-foot">Your Claude Code’s connectors show here after your first conversation.</p>}
    {open === 'tetrix' && (
      <TetrixWindow tetrix={t} said={said} waiting={!!t.signIn || busy.Tetrix} onClose={() => { setOpen(null); setSaid(undefined); }}
        onConnect={(to) => { setSaid(undefined); act(async () => { try { page(await api.tetrixSignIn(to)); } catch (e) { setSaid(signInWhy('Tetrix', e)); } }, 'Tetrix')(); }}
        onCancel={() => { act(api.tetrixCancel)(); setOpen(null); }} />
    )}
    {/* (closed: read again, in case gh was signed out meanwhile) */}
    {open === 'github-off' && <GitHubOffWindow onClose={() => { setOpen(null); void read(true); }} />}
  </>;
}

// The owner's own Claude Code's MCP servers and claude.ai connectors (docs/contracts/connectors.md,
// CONNECTORS-4): what an own conversation's session named, as obid kept it, after GitHub and Tetrix.
// They're the owner's, in Claude Code and on claude.ai: Oby adds, signs in to and removes none, so a
// claude.ai one waiting for its sign-in opens claude.ai's own page for it.
const CLAUDE_AI = 'claude.ai ';
const CLAUDE_AI_CONNECTORS = 'https://claude.ai/settings/connectors';
function ownRows(seen: Seen | null): Connector[] {
  return (seen?.claude?.servers ?? []).map((s, i) => {
    const fromClaudeAi = s.name.startsWith(CLAUDE_AI);
    const name = fromClaudeAi ? s.name.slice(CLAUDE_AI.length) : s.name;
    const whose = fromClaudeAi ? 'Your claude.ai connector' : 'Your Claude Code’s MCP server';
    const about = s.status === 'connected' ? whose
      : s.status === 'needs-auth' ? (fromClaudeAi ? 'Sign in to it on claude.ai' : 'Sign in to it in Claude Code (/mcp)')
      : s.status === 'pending' ? 'Connecting'
      : s.status === 'failed' ? 'Couldn’t connect'
      : s.status === 'disabled' ? 'Turned off in Claude Code'
      : whose;
    const control = fromClaudeAi && s.status === 'needs-auth'
      ? <button className="set-button set-connect" onClick={() => window.app?.openExternal(CLAUDE_AI_CONNECTORS)}>Connect</button>
      : null;
    return { key: `own-${i}-${s.name}`, name, about, brand: brandOf(name), connected: s.status === 'connected', control };
  });
}

// its logo when the app has one, else its first letter on a circle
function brandOf(name: string): Brand {
  const n = name.toLowerCase();
  const known = n === 'slack' ? BRANDS.slack : n === 'gmail' ? BRANDS.gmail : n === 'google meet' ? BRANDS.meet
    : n.startsWith('zoom') ? BRANDS.zoom : n === 'tetrix' ? BRANDS.tetrix : n.startsWith('deskree') ? BRANDS.deskree
    : n === 'github' ? BRANDS.github : null;
  return known ?? { ...BRANDS.circle, letter: (name.trim()[0] ?? '?').toUpperCase() };
}

// where a sign-in's page is: " at github.com"
function at(url?: string) {
  try { return url ? ` at ${new URL(url).host}` : ''; } catch { return ''; }
}

// what the person calls the Tetrix Oby uses: its address's host
function serverName(t: Tetrix) {
  try { return new URL(t.url ?? '').host; } catch { return 'Tetrix'; }
}

// A window over Settings (a connector's, or asking before a Remove): its logo and title, a ×, Esc
// or a click outside closes it. alert: a question, its lede read out with it (describedBy)
export function Window({ brand, title, onClose, alert, children }: { brand?: Brand; title: string; onClose: () => void; alert?: string; children: React.ReactNode }) {
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
          <span className="connector-window-title">{brand && <BrandMark brand={brand} size={22} />}<h2>{title}</h2></span>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        {children}
      </div>
    </div>,
    document.body,
  );
}

// Which Tetrix Oby signs in to, and the sign-in: the address of the person's own (obid checks it:
// HTTPS, plain HTTP only on 127.0.0.1; Oby offers no Tetrix by name, TETRIX-URL-1), in the field the
// first run's Tetrix row has too. Connecting to another signs Oby in there in place of the one
// before. Continue opens its page in the browser and this waits, until obid says it's in (then it
// closes itself, and the row is under Connected) or Cancel. Setting up has the field on its row
// instead (SetupCard's TetrixRow).
export function TetrixWindow({ tetrix: t, said, waiting, onConnect, onCancel, onClose }: {
  tetrix: Tetrix; said?: string; waiting: boolean; onConnect: (to: { url: string }) => void; onCancel: () => void; onClose: () => void;
}) {
  // the server Continue was for: the window closes once obid is signed in there (an Edit's old
  // server being connected already says nothing); a sign-in that didn't take shows the form again
  const [aim, setAim] = useState<string | null>(null);
  const address = useTetrixAddress(said, { initial: t.url ?? '', onConnect: (url) => { setAim(url); onConnect({ url }); } });
  const same = t.connected && !t.error && address.url === t.url;
  useEffect(() => {
    if (!aim || waiting) return;
    if (t.connected && !t.error && t.url === aim) onClose(); else setAim(null);
  }, [aim, waiting, t.connected, t.error, t.url]);
  const server = (() => { try { return new URL(address.url ?? '').host; } catch { return 'the server'; } })();
  // waiting: closing it ends the sign-in, once obid has one to end (while it starts, there's none yet)
  return (
    <Window brand={BRANDS.tetrix} title={t.connected ? 'Edit Tetrix' : 'Connect Tetrix'} onClose={waiting ? (t.signIn ? onCancel : () => {}) : onClose}>
      {waiting ? <>
        <p className="connector-wait"><Spinner label="Signing in" />Finish signing in on {server}’s page in your browser.</p>
        <div className="modal-actions"><button type="button" className="set-button" disabled={!t.signIn} onClick={onCancel}>Cancel</button></div>
      </> : <>
        <p className="modal-lede">Oby signs in once, and all your agents can use it.</p>
        <label className="modal-label">Your Tetrix URL</label>
        <div className="connector-address"><TetrixAddress {...address} autoFocus /></div>
        <div className="modal-actions">
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="button" className="modal-primary" disabled={!address.ready || same} onClick={address.connect}>Continue in Browser</button>
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

// A window asking before something that can't be undone here (Settings › Accounts' and Accounts'
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
