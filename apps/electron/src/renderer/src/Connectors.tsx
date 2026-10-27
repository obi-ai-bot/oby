import { useEffect, useState } from 'react';
import { Check, Search } from 'lucide-react';
import { BRANDS, BrandMark, type Brand } from './brands';
import { Group, Row, Select, TextField, useSetting } from './SettingsView';
import { onObid } from './lean';
import { api, type GitHub, type Tetrix, type TetrixTarget } from './obid';
import { say, useLive } from './live';

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
  // the connector whose sign-in is on its way (gh takes a few seconds to give its code)
  const [busy, setBusy] = useState<'GitHub' | 'Tetrix' | null>(null);
  const repos = useLive().repos.length;
  const read = () => Promise.all([api.github(), api.tetrix()]).then(([g, t]) => { setGitHub(g); setTetrix(t); setProblem(null); }, (e: Error) => setProblem(e.message));
  useEffect(() => { void read(); }, []);
  const signingIn = !!(github?.signIn || tetrix?.signIn);
  useEffect(() => {
    if (!signingIn) return;
    const t = setInterval(read, 2000);
    return () => clearInterval(t);
  }, [signingIn]);
  const act = (f: () => Promise<unknown>, signingIn?: 'GitHub' | 'Tetrix') => () => {
    if (signingIn) setBusy(signingIn);
    void f().catch(say).then(read).finally(() => setBusy(null));
  };
  // a sign-in's page, in the person's browser (main opens https only)
  const open = (page: { url: string }) => window.app?.openExternal(page.url);
  const waiting = (cancel: () => Promise<unknown>) => <>
    <button className="set-button" disabled>Waiting…</button>
    <button className="set-button" onClick={act(cancel)}>Cancel</button>
  </>;
  if (problem && !github) return <p className="group-empty">{problem}</p>;
  if (!github || !tetrix) return null;

  const g = github, t = tetrix;
  const gitHub: Connector = { name: 'GitHub', brand: BRANDS.github, connected: g.signedIn, about: 'Repos, pull requests and checks', control: null };
  if (g.signIn || busy === 'GitHub') {
    gitHub.about = `Finish in your browser${g.signIn?.userCode ? ` · enter ${g.signIn.userCode}` : ''}`;
    gitHub.control = waiting(api.githubCancel);
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

  // the server Oby signed in to last (kept after Disconnect), else the first preset
  const current: TetrixTarget | null = t.preset ? { preset: t.preset } : t.url ? { url: t.url } : t.presets[0] ? { preset: t.presets[0].id } : null;
  const signIn = (to: TetrixTarget) => act(async () => open(await api.tetrixSignIn(to)), 'Tetrix');
  const tetrixRow: Connector = { name: 'Tetrix', brand: BRANDS.tetrix, connected: t.connected, about: t.error ?? 'Project knowledge and blast radius', control: null };
  const disconnect = <button className="set-button is-danger" onClick={act(api.tetrixDisconnect)}>Disconnect</button>;
  if (t.signIn || busy === 'Tetrix') {
    tetrixRow.about = 'Finish in your browser';
    tetrixRow.control = waiting(api.tetrixCancel);
  } else if (t.connected) {
    // with an error, still connected: a refresh the sign-in server didn't answer
    if (!t.error) tetrixRow.about = `${serverName(t)} · for every agent`;
    tetrixRow.control = <>{t.error ? current && <button className="set-button" onClick={signIn(current)}>Reconnect</button> : connected}{disconnect}</>;
  } else {
    tetrixRow.control = <button className="set-button" disabled={!current} onClick={current ? signIn(current) : undefined}>Connect</button>;
  }

  return <List connectors={[gitHub, tetrixRow]} after={(shown) => shown.includes(tetrixRow) && (
    <TetrixServer key={t.url ?? ''} tetrix={t} busy={!!t.signIn || busy === 'Tetrix'} onConnect={(to) => signIn(to)()} />
  )} />;
}

// what the person calls the Tetrix Oby uses: its preset's name, else its address's host
function serverName(t: Tetrix) {
  const preset = t.presets.find((p) => p.id === t.preset);
  if (preset) return preset.name;
  try { return new URL(t.url ?? '').host; } catch { return 'Tetrix'; }
}

// Which Tetrix Oby signs in to: a preset, or another server's MCP address (obid checks it: HTTPS,
// plain HTTP only on 127.0.0.1). Connecting to another signs Oby in there in place of the one before.
function TetrixServer({ tetrix: t, busy, onConnect }: { tetrix: Tetrix; busy: boolean; onConnect: (to: TetrixTarget) => void }) {
  const [pick, setPick] = useState(t.preset ?? (t.url ? 'url' : t.presets[0]?.id ?? 'url'));
  const [url, setUrl] = useState(t.preset ? '' : t.url ?? '');
  const to: TetrixTarget | null = pick === 'url' ? (url.trim() ? { url: url.trim() } : null) : { preset: pick };
  const same = pick === 'url' ? !t.preset && url.trim() === t.url : pick === t.preset;
  return (
    <Group i={4} title="Tetrix server" footer="Oby signs in to Tetrix once, for every agent. Connecting to another server signs Oby in there instead.">
      <Row label="Server">
        <Select value={pick} onChange={setPick} label="Tetrix server" options={[...t.presets.map((p) => [p.id, p.name] as const), ['url', 'Another server']]} />
      </Row>
      {pick === 'url' && <Row label="Address"><TextField value={url} onChange={setUrl} placeholder="https://tetrix.example.com/mcp" label="Tetrix server address" /></Row>}
      <Row label="">
        <button className="set-button" disabled={!to || busy || (same && t.connected && !t.error)} onClick={() => to && onConnect(to)}>Connect</button>
      </Row>
    </Group>
  );
}
