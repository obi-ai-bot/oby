import { useEffect, useState } from 'react';
import { Check, Search } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { Group, Row, TextField, useSetting } from './SettingsView';
import { onObid } from './lean';
import { api, type GitHub, type Tetrix, type TetrixTarget } from './obid';
import { say, useLive } from './live';
import { tetrixUrl, tetrixUrlProblem } from '../../shared/tetrix-url';

// Settings › Connectors: what's connected, then what's available, with a search over both. On obid
// (lean.ts) the ones obid has, as they stand; elsewhere Sangam's demo list.
export default function Connectors() {
  return onObid ? <Live /> : <Mock />;
}

// A row: its logo, name and line, and its control (Connected, or what it needs); note: a line
// under its group
type Connector = { name: string; about: string; brand: Brand; connected: boolean; control: React.ReactNode; note?: string };

function List({ connectors, after }: { connectors: Connector[]; after?: (shown: Connector[]) => React.ReactNode }) {
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
      <Group key={g.title} i={2 + k} title={g.title} footer={g.items.map((c) => c.note).filter(Boolean).join(' ') || undefined}>
        {g.items.map((c) => (
          <Row key={c.name} label={<span className="set-connector"><BrandMark brand={c.brand} /><span className="set-label">{c.name}<span className="set-about">{c.about}</span></span></span>}>
            {c.control}
          </Row>
        ))}
      </Group>
    ))}
    {!groups.length && <p className="group-empty">No connector matches “{query.trim()}”</p>}
    {after?.(shown)}
  </>;
}

const connected = <span className="set-value"><Check size={14} strokeWidth={1.8} />Connected</span>;

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
    control: names.includes(c.name) ? connected : <button className="set-button" onClick={() => setNames([...names, c.name])}>Connect</button>,
  }))} />;
}

// On obid: GitHub (obid's gh, with GitHub's own device sign-in) and Tetrix (obid is its OAuth
// client), docs/runner-api.md. The launch shows these two only (AGENTS.md); the agents' logins
// are Settings › Accounts. Read when the page opens, then every 2 seconds while a sign-in is under
// way (obid sends no event for GitHub's). Nothing here holds a token: obid keeps them.
function Live() {
  const [github, setGitHub] = useState<GitHub | null>(null);
  const [tetrix, setTetrix] = useState<Tetrix | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // each connector's sign-in on its way, apart (gh takes a few seconds to give its code)
  const [busy, setBusy] = useState({ GitHub: false, Tetrix: false });
  // the Tetrix address typed below, while it isn't the one Oby uses (null: that one)
  const [draft, setDraft] = useState<string | null>(null);
  const repos = useLive().repos.length;
  const read = () => Promise.all([api.github(), api.tetrix()]).then(([g, t]) => { setGitHub(g); setTetrix(t); setProblem(null); }, (e: Error) => setProblem(e.message));
  useEffect(() => { void read(); }, []);
  // signed in somewhere new: the field starts from there again
  useEffect(() => setDraft(null), [tetrix?.url]);
  const signingIn = !!(github?.signIn || tetrix?.signIn);
  useEffect(() => {
    if (!signingIn) return;
    const t = setInterval(read, 2000);
    return () => clearInterval(t);
  }, [signingIn]);
  // only a sign-in's own click clears its busy: a Cancel never puts back a Connect still on its way
  const act = (f: () => Promise<unknown>, signIn?: 'GitHub' | 'Tetrix') => () => {
    if (signIn) setBusy((b) => ({ ...b, [signIn]: true }));
    void f().catch(say).then(read).finally(() => { if (signIn) setBusy((b) => ({ ...b, [signIn]: false })); });
  };
  // a sign-in's page, in the person's browser (main opens https only)
  const open = (page: { url: string }) => window.app?.openExternal(page.url);
  // Cancel only once obid has a sign-in to end (while it starts, there's none yet)
  const waiting = (cancel: (() => Promise<unknown>) | false) => <>
    <button className="set-button" disabled>Waiting…</button>
    {cancel && <button className="set-button" onClick={act(cancel)}>Cancel</button>}
  </>;
  if (problem && !github) return <p className="group-empty">{problem}</p>;
  if (!github || !tetrix) return null;

  const g = github, t = tetrix;
  const gitHub: Connector = { name: 'GitHub', brand: BRANDS.github, connected: g.signedIn, about: 'Repos, pull requests and checks', control: null };
  if (g.signIn || busy.GitHub) {
    gitHub.about = `Finish in your browser${at(g.signIn?.url)}${g.signIn?.userCode ? ` · enter ${g.signIn.userCode}` : ''}`;
    gitHub.control = waiting(!!g.signIn && api.githubCancel);
  } else if (g.signedIn) {
    gitHub.about = `${g.user ?? 'Signed in'}${repos ? ` · ${repos} repositories` : ''}`;
    gitHub.control = connected;
    // obid never signs gh out: it's the person's own login, their terminal's too
    gitHub.note = 'GitHub stays signed in with gh’s own login on this Mac. To sign it out, run gh auth logout in Terminal.';
  } else if (!g.gh) {
    gitHub.about = 'GitHub’s tool, gh, isn’t on this Mac';
  } else {
    gitHub.control = <button className="set-button" onClick={act(async () => open(await api.githubSignIn()), 'GitHub')}>Connect</button>;
  }

  // the Tetrix to connect to: the address typed below, else the one Oby signed in to last (kept
  // after Disconnect). Oby has no built-in Tetrix and no default: with nothing typed yet there's
  // nothing to connect, and the row says so (not an error: Oby runs without Tetrix until then).
  // One Connect at a time: the row's, or, while Oby is connected, the server group's for another one.
  const address = draft ?? t.url ?? '';
  const addressProblem = tetrixUrlProblem(address);
  const chosen: TetrixTarget | null = addressProblem ? null : { url: tetrixUrl(address) };
  const same = !!t.url && tetrixUrl(address) === t.url;
  const connect = (label: string) => <button className="set-button" disabled={!chosen} onClick={chosen ? act(async () => open(await api.tetrixSignIn(chosen)), 'Tetrix') : undefined}>{label}</button>;
  const tetrixRow: Connector = { name: 'Tetrix', brand: BRANDS.tetrix, connected: t.connected, about: t.error ?? 'Project knowledge and blast radius', control: null };
  const disconnect = <button className="set-button is-danger" onClick={act(api.tetrixDisconnect)}>Disconnect</button>;
  const settled = !t.signIn && !busy.Tetrix;
  if (!settled) {
    tetrixRow.about = `Finish in your browser${at(t.signIn?.server)}`;
    tetrixRow.control = waiting(!!t.signIn && api.tetrixCancel);
  } else if (t.connected) {
    // with an error, still connected: a refresh the sign-in server didn't answer
    if (!t.error) tetrixRow.about = `${serverName(t)} · for every agent`;
    tetrixRow.control = <>{t.error ? connect(same ? 'Reconnect' : 'Connect') : connected}{disconnect}</>;
  } else {
    if (!t.error) tetrixRow.about = t.url ? `${serverName(t)} · not signed in` : 'Not connected · enter your Tetrix’s MCP server URL below';
    tetrixRow.control = connect(t.url && same ? 'Reconnect' : 'Connect');
  }

  return <List connectors={[gitHub, tetrixRow]} after={(shown) => shown.includes(tetrixRow) && (
    <TetrixServer address={address} problem={address.trim() ? addressProblem : null} onAddress={setDraft}>
      {settled && t.connected && !t.error && !same && connect('Connect')}
    </TetrixServer>
  )} />;
}

// where a sign-in's page is: " at github.com"
function at(url?: string) {
  try { return url ? ` at ${new URL(url).host}` : ''; } catch { return ''; }
}

// what the person calls the Tetrix Oby uses: its address's host
function serverName(t: Tetrix) {
  try { return new URL(t.url ?? '').host; } catch { return 'Tetrix'; }
}

// Which Tetrix Oby signs in to: the owner's own, by its MCP server's address (shared/tetrix-url.ts
// checks it as the person types; obid checks it again). Oby has no built-in Tetrix and no default.
// Another address signs Oby in there in place of the one before (which obid signs out of, when
// it's another sign-in server's). problem: why the address isn't one yet, under the field;
// children: its Connect, when it's the one to click.
function TetrixServer({ address, problem, onAddress, children }: { address: string; problem: string | null; onAddress: (address: string) => void; children: React.ReactNode }) {
  return (
    <Group i={4} title="Tetrix server" footer={problem ?? 'Oby has no built-in Tetrix: enter yours, its MCP server’s address, starting with https://. Oby signs in there once, for every agent; another address signs Oby in there instead.'}>
      <Row label="MCP server URL"><TextField value={address} onChange={onAddress} placeholder="https://tetrix.example.com/mcp" label="MCP server URL" /></Row>
      {children && <Row label="">{children}</Row>}
    </Group>
  );
}
