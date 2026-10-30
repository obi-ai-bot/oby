import { useEffect, useState, useSyncExternalStore } from 'react';
import { Check } from 'lucide-react';
import { BrandMark } from './brands';
import { useScrollThumb } from './useScrollThumb';
import { AGENTS, type Agent } from './mock';
import { MODES, SANDBOX } from './modes';
import { OwnerAvatar } from './ObyFace';
import { useSpheres } from './spheres';
import { everything, onObid } from './lean';
import { ApiError, api, type ModelOptions, type Models, type SavedRule, type SignIn, type Status } from './obid';
import Connectors, { ConfirmWindow, readConnectors } from './Connectors';
import { Menu } from './Menu';
import { patchStatus, say, useLive } from './live';
import { version } from '../../../package.json';
import type { Update } from '../../shared/updates';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;

// Settings' pages, as Wellington's Mac app has them (apps/mac/app/Settings.swift
// on his dev branch): General (with Voice and Notifications, shared with the
// iPhone), Runner, Remote access, Accounts, Models, Vault and About; Connectors
// is ours, and Sharing a preview (docs/research/sphere-sharing.md). Each page opens in the view, as a task's page does, from the
// gear at the icons' foot (the account menu, with ?everything), then the side panel's Settings list.
// Runner, Sharing, Remote access and Vault are left out of v1 (Sangam, 2026-10-28) and come
// back later: only with ?everything (lean.ts)
// Agents takes Accounts' and Models' place: each agent's accounts with its model and efforts,
// and which one new tasks go to (the old two stay with ?everything)
const LATER = new Set(['runner', 'sharing', 'remote', 'vault', 'accounts', 'models']);
export const SETTINGS_PAGES = [
  { id: 'general', label: 'General' },
  { id: 'agents', label: 'Agents' },
  { id: 'runner', label: 'Runner' },
  { id: 'sharing', label: 'Sharing' },
  { id: 'remote', label: 'Remote access' },
  { id: 'accounts', label: 'Accounts' },
  { id: 'models', label: 'Models' },
  { id: 'connectors', label: 'Connectors' },
  { id: 'vault', label: 'Vault' },
  { id: 'about', label: 'About' },
].filter((p) => everything || !LATER.has(p.id));
export const settingsLabel = (id: string) => SETTINGS_PAGES.find((p) => p.id === id)?.label ?? 'Settings';

// What's set, kept while the app runs (a mock: nothing is saved anywhere)
const values = new Map<string, unknown>();
const listeners = new Set<() => void>();
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export function useSetting<T>(key: string, initial: T): [T, (v: T) => void] {
  const value = useSyncExternalStore(
    subscribe,
    () => (values.has(key) ? values.get(key) : initial) as T,
  );
  return [value, (v) => { values.set(key, v); listeners.forEach((l) => l()); }];
}
// The same, kept across launches (localStorage, oby.setting.<key>): what's set here and nowhere
// else yet (obid has no such setting), so it isn't lost each time the app quits
export function useKeptSetting<T>(key: string, initial: T): [T, (v: T) => void] {
  const kept = readKept<T>(key);
  const [value] = useSetting(key, kept === undefined ? initial : kept);
  return [value, (v) => keep(key, v)];
}
function readKept<T>(key: string): T | undefined {
  if (!values.has(key)) {
    try { const kept = localStorage.getItem(`oby.setting.${key}`); if (kept != null) values.set(key, JSON.parse(kept)); } catch { /* the default */ }
  }
  return values.get(key) as T | undefined;
}
function keep<T>(key: string, v: T) {
  try { localStorage.setItem(`oby.setting.${key}`, JSON.stringify(v)); } catch { /* this launch only */ }
  values.set(key, v);
  listeners.forEach((l) => l());
}

const EFFORTS = [['low', 'Low'], ['medium', 'Medium'], ['high', 'High'], ['xhigh', 'Extra high'], ['max', 'Max']] as const;

export default function SettingsView({ page, ...frame }: {
  page: string;
  leaving: boolean;
  covered?: boolean;
  onClose: () => void;
  onGone: () => void;
}) {
  // Connectors' GitHub and Tetrix, read as Settings opens, so that page opens on them at once
  useEffect(readConnectors, []);
  const Page = PAGES[page] ?? General;
  return <SettingsPage title={settingsLabel(page)} label={`${settingsLabel(page)} settings`} {...frame}><Page /></SettingsPage>;
}

// A Settings page in the view (the app's, an Obysphere's, its Knowledge): its title over
// its groups, rising in; Esc goes back
export function SettingsPage({ title, label = title, leaving, covered = false, onClose, onGone, children }: {
  title: string;
  label?: string;  // what a screen reader calls it, when the title alone isn't enough
  leaving: boolean;
  covered?: boolean;
  onClose: () => void;
  onGone: () => void;
  children: React.ReactNode;
}) {
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLElement>();

  // a timer, not animationend: animations don't run while the window is hidden
  useEffect(() => {
    if (!leaving) return;
    const t = setTimeout(onGone, LEAVE_MS);
    return () => clearTimeout(t);
  }, [leaving, onGone]);

  useEffect(() => {
    if (covered) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !(e.target as Element).closest?.('input, select')) onClose(); };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [onClose, covered]);

  return (
    <div className={`group-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <section ref={list} className="group" aria-label={label} onScroll={onScroll}>
        <div className="group-column set-column">
          <h1 className="group-title rise" style={{ '--i': 0 } as React.CSSProperties}>{title}</h1>
          {children}
        </div>
      </section>
      <div {...thumbProps} />
    </div>
  );
}

// General: opening at login, how much this Mac's own jobs ask, Voice, Notifications
function General() {
  const [login, setLogin] = useSetting('login', true);
  const [speaks, setSpeaks] = useSetting('voice.speaks', true);
  const [language, setLanguage] = useSetting('voice.language', 'system');
  return <>
    <Group i={1}>
      <Row label="Open at login"><Switch on={login} onChange={setLogin} label="Open at login" /></Row>
    </Group>
    <Group i={2} title="Permissions on this Mac" footer="Default mode sets how often your agents check with you before they edit files or run commands. To change it for one task, use that task’s Mode menu. Always allow adds a rule here; remove it to be asked again.">
      {onObid ? <MacPermissions /> : <MockMacPermissions />}
    </Group>
    {/* Voice: back later, with talking to Oby (only with ?everything) */}
    {everything && <Group i={3} title="Voice" footer="When you talk to Oby with fn, it answers out loud and, after fn + Space, listens again for your answer until you press fn or say “stop”. Typed messages are answered in writing. Pressing fn stops it mid-sentence.">
      <Row label="Speak replies to spoken requests"><Switch on={speaks} onChange={setSpeaks} label="Speak replies to spoken requests" /></Row>
      <Row label="Speech language">
        <Select value={language} onChange={setLanguage} label="Speech language" options={[['system', 'Mac language'], ['en-US', 'English'], ['pt-BR', 'Português (Brasil)'], ['pt-PT', 'Português (Portugal)']]} />
      </Row>
    </Group>}
    <Notifications />
  </>;
}

// On obid, its own: the mode new jobs get, and the rules Always allow saved; Remove forgets one
function MacPermissions() {
  const [mode, setMode] = useState<string | null>(null);
  const [rules, setRules] = useState<SavedRule[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const failed = (e: Error) => setProblem(e.message);
  useEffect(() => {
    Promise.all([api.permissions(), api.rules()]).then(([p, r]) => { setMode(p.mode); setRules(r); }, failed);
  }, []);
  const change = (m: string) => api.setPermissions(m).then((p) => { setMode(p.mode); setProblem(null); }, failed);
  const forget = (r: Rule) => api.forgetRule(r.id!).then((left) => { setRules(left); setProblem(null); }, failed);
  return <>
    {/* Sangam's four; obid's old Sandbox only while it's the one saved, until another is picked */}
    {mode && <ModeRow mode={mode} setMode={change} modes={mode === SANDBOX.id ? [...MODES, SANDBOX] : MODES} />}
    <RuleRows rules={rules.map((r) => ({ ...r, decision: r.decision === 'deny' ? 'Denied' : 'Allowed' }))} onRemove={forget} />
    {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
  </>;
}

// The mock's (?everything), kept while the app runs
function MockMacPermissions() {
  const [mode, setMode] = useSetting('mac.mode', 'bypass');
  return <>
    <ModeRow mode={mode} setMode={setMode} />
    <Rules id="mac.rules" initial={[
      { decision: 'Allowed', repo: 'deskree-inc/oby', rule: 'Bash(npm test:*)' },
      { decision: 'Denied', repo: 'deskree-inc/tetrix-ee', rule: 'Bash(git push --force:*)' },
    ]} />
  </>;
}

// Notifications: on obid, Oby's alerts on this Mac on or off (main/notify.ts keeps it in the app's
// own folder), and macOS's own settings for them (allowed, banners, sounds, Focus); otherwise
// Sangam's demo row
function Notifications() {
  const bridge = onObid ? window.app?.notifications : undefined;
  const [alerts, setAlerts] = useState<boolean | null>(null);
  useEffect(() => { void bridge?.alerts().then(setAlerts); }, []);
  const turn = (on: boolean) => { setAlerts(on); void bridge?.alerts(on).then(setAlerts); };
  return (
    <Group i={everything ? 4 : 3} title="Notifications" footer="Oby alerts you when a job needs you, like a question or a failed run. Click an alert to open the job.">
      {bridge ? <>
        <Row label="Alerts"><Switch on={alerts ?? true} onChange={turn} label="Alerts" /></Row>
        <Row label="Banners, sounds and Focus"><button className="set-button" onClick={bridge.openSystemSettings}>Open System Settings</button></Row>
      </> : <Row label="Alerts"><Value>Allowed</Value></Row>}
    </Group>
  );
}

// Runner: the always-on runner and how this Mac reaches it, keeping this Mac
// awake, the runner's permissions, the checks this Mac runs for it, watches,
// and the key that publishes
function Runner() {
  const [here, setHere] = useSetting('runner.here', true);
  const [url, setUrl] = useSetting('runner.url', '');
  const [awake, setAwake] = useSetting('runner.awake', true);
  const [mode, setMode] = useSetting('runner.mode', 'acceptEdits');
  const [merge, setMerge] = useSetting('checks.merge', true);
  const [apple, setApple] = useSetting('checks.apple', false);
  return <>
    <Group i={1} title="Runner" footer={here ? 'The runner is Oby’s login item on this Mac, and Oby follows it on 127.0.0.1.' : 'Oby follows the runner at this address: the island lists its jobs.'}>
      <Row label="Runner"><Segmented value={here ? 'mac' : 'remote'} onChange={(v) => setHere(v === 'mac')} options={[['mac', 'This Mac'], ['remote', 'Remote']]} label="Runner" /></Row>
      {here
        ? <Row label="Login item"><Value>On</Value></Row>
        : <Row label="URL"><TextField value={url} onChange={setUrl} placeholder="http://tetrix-lab:8790" label="Runner URL" /></Row>}
      <Row label="Status"><Value>{here || url ? <><span className="set-dot" />Connected</> : 'Off'}</Value></Row>
      <Row label="Job worktrees"><Value>3.2 GB</Value></Row>
      <Row label="Last cleanup"><Value>Today at 9:12 AM</Value></Row>
    </Group>
    <Group i={2} title="This Mac" footer="On power, Oby prevents idle sleep while This Mac is enabled and this setting is on. A running job always keeps this Mac awake.">
      <Row label="Keep my Mac available"><Switch on={awake} onChange={setAwake} label="Keep my Mac available" /></Row>
      <Row label="Availability"><Value>{awake ? 'Available, on power' : 'Sleeps when idle'}</Value></Row>
    </Group>
    <Group i={3} title="Permissions" footer="Ask: the agent asks before it edits or runs anything. Always allow saves a rule, so it doesn’t ask again; remove the rule to be asked again.">
      <ModeRow mode={mode} setMode={setMode} />
      <Rules id="runner.rules" initial={[{ decision: 'Allowed', repo: 'deskree-inc/oberon', rule: 'Bash(swift test:*)' }]} />
    </Group>
    <Group i={4} title="Checks on this Mac">
      <Row label="Run merge checks on this Mac" about="When Oby’s review approves a pull request, this Mac runs its tests and builds the Mac and iPhone apps, one at a time, before Oby merges it into dev.">
        <Switch on={merge} onChange={setMerge} label="Run merge checks on this Mac" />
      </Row>
      <Row label="Build and test agents’ work on this Mac" about="When an agent on the runner asks, this Mac builds and tests the Mac and iPhone apps at its latest commit, before Oby’s review has read it, and the agent gets the compiler’s errors and failing tests.">
        <Switch on={apple} onChange={setApple} label="Build and test agents’ work on this Mac" />
      </Row>
    </Group>
    <Group i={5} title="Apple Watch without iPhone" footer="Away from your iPhone, a paired watch reaches the runner through your Cloudflare Tunnel. Pair one in Oby’s Settings on your iPhone, with Oby open on the watch: pairing asks for Face ID.">
      <Row label={<span className="set-none">None yet</span>} />
    </Group>
    <Group i={6} title="Publishing" footer="Oby opens a pull request by itself once a job’s check passes and the secret scan is clean. When that failed, publish by hand here or on your iPhone: this Mac’s key, like the iPhone’s, signs only after Touch ID. Oby merges only into dev, once its review approves and your Mac’s checks pass.">
      <Row label="Publishing key"><Value>Registered</Value></Row>
    </Group>
  </>;
}

// Remote access: signing in to Oby's account, then reaching this Mac from anywhere
function Remote() {
  const [on, setOn] = useSetting('remote.on', true);
  const [forgot, setForgot] = useSetting('remote.forgot', false);
  return <>
    <Group i={1} title="Oby account" footer="Remote access goes through Oby’s control plane, which signs you in with a code by email and gives this Mac a hostname of its own.">
      <Row label="Signed in as"><Value>doti@deskree.com</Value></Row>
    </Group>
    <Group i={2} title="Remote access" footer="On, this Mac runs its Cloudflare Tunnel outbound, with no port to open: away from home, your iPhone reaches the gateway, and nothing else, at the hostname. Off keeps the hostname for next time. Forget removes this Mac from your account, and the hostname stops answering.">
      <Row label="Reach this Mac from anywhere"><Switch on={on && !forgot} onChange={(v) => { setOn(v); setForgot(false); }} label="Reach this Mac from anywhere" /></Row>
      {!forgot && <Row label="Hostname"><Value>doti-mbp-7f3k.oby.tetrix.pro</Value></Row>}
      <Row label="Status"><Value>{forgot ? 'Not set up' : on ? <><span className="set-dot" />Connected</> : 'Off'}</Value></Row>
      {!forgot && <Row label=""><button className="set-button is-danger" onClick={() => setForgot(true)}>Forget This Mac</button></Row>}
    </Group>
  </>;
}

// Accounts: the logins Oby spreads jobs across, who pays for each, how much each has used
// plan: what kind of login it is, which decides whether a part of it can be
// shared (SHARING below)
// used: this week's share in percent, null while its tool hasn't said
// signedIn: obid's (the mock's are all signed in); label: what obid says about the login, such as
// "Run codex once" while it's signed out
// session: the 5-hour window's use (null: none reported) and when it starts over; weekly: whether
// used is the week's
export type Account = { id: string; agent: Agent; name: string; payer: string; used: number | null; resets: string; plan?: Plan; signedIn?: boolean; label?: string | null; session?: number | null; sessionResets?: string | null; weekly?: boolean };
// how full an account (or an agent) is: past 80% yellow, past 90% red
export const fullness = (used: number) => (used > 90 ? 'is-over' : used > 80 ? 'is-high' : '');
// an account's week in words, under its session's bar: its share when the tool reported one, else
// what obid says (not signed in, none reported yet, Cursor's none); a session but no week: says so
export const weekWords = (a: Account) => (a.weekly && a.used != null ? `This week ${a.used}% · ${a.resets.replace(/^Resets/, 'resets')}`
  : a.weekly === false && a.session != null ? 'No weekly use reported yet' : a.resets);
// an account as obid's status says it (docs/contracts/usage.md, USAGE-1 and USAGE-6): its week's
// share (else its session's), its 5-hour session (signed in only; a session past its reset reads
// 0) and when it starts over, and what to say when there's no figure
export function accountRow(a: Status['accounts'][number], now: number): Account {
  const used = a.agent === 'cursor' ? null : a.sevenDay ?? a.fiveHour;
  const over = !!a.fiveHourResets && Date.parse(a.fiveHourResets) <= now;
  const session = a.agent === 'cursor' || !a.signedIn || a.fiveHour == null ? null : over ? 0 : Math.round(a.fiveHour * 100);
  return {
    id: a.id, agent: a.agent, name: a.name, payer: a.payer ?? 'You', signedIn: a.signedIn, label: a.label ?? null, used: used == null ? null : Math.round(used * 100),
    resets: !a.signedIn ? a.label ?? 'Not signed in' : a.agent === 'cursor' ? 'Cursor doesn’t report usage' : used == null ? 'No use reported yet'
      : a.sevenDayResets ? `Resets ${new Date(a.sevenDayResets).toLocaleDateString([], { weekday: 'long' })}` : 'No reset reported',
    session,
    sessionResets: session != null && !over && a.fiveHourResets ? new Date(a.fiveHourResets).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : null,
    weekly: a.sevenDay != null,
  };
}
const ACCOUNTS: Account[] = [
  { id: 'a1', agent: 'claude', name: 'Deskree', payer: 'Deskree', used: 36, resets: 'Resets Thursday', plan: 'seat', session: 12, sessionResets: '4:10 PM', weekly: true },
  { id: 'a2', agent: 'claude', name: 'Personal', payer: 'Doti', used: 82, resets: 'Resets in 2 days', plan: 'personal', session: 86, sessionResets: '2:14 PM', weekly: true },
  { id: 'a3', agent: 'codex', name: 'Deskree', payer: 'Deskree', used: 12, resets: 'Resets Monday', plan: 'seat', session: 5, sessionResets: '6:40 PM', weekly: true },
  { id: 'a4', agent: 'claude', name: 'Deskree API', payer: 'Deskree', used: 8, resets: '$32 of $400 this month', plan: 'api', session: null, weekly: false },
];
// the accounts and their use (the 5-hour session, and the week): here, and in the message box's usage panel (the gauge)
export const useAccounts = () => useSetting<Account[]>('accounts', onObid ? [] : ACCOUNTS);
// obid's accounts and their use, in place of the mock's (live.ts)
export const replaceAccounts = (next: Account[]) => { values.set('accounts', next); listeners.forEach((l) => l()); };
// Each tool's own login as obid lists it (core/crates/obid/src/accounts.rs): the person's, with no
// folder of obid's, so obid never signs it in (the tool itself does)
const OWN_LOGIN: Record<Agent, string> = { claude: 'default', codex: 'codex', cursor: 'cursor' };
const ownLogin = (a: Account) => OWN_LOGIN[a.agent] === a.id;
// where an error of Add an account's shows (a row's shows on its row, by the account's id)
const ADD = 'add';
// On obid they're obid's (docs/runner-api.md, "obid only: accounts"): Add Account adds one (obid
// makes its folder) and opens its tool's own sign-in page at once; Sign In opens it again, Remove
// removes it. Each tool keeps its own login; Oby only learns that it worked. Leaving the page
// mid-sign-in leaves it under way on obid: Sign In picks the same one back up.
// Signing accounts in, as Settings › Accounts and › Agents both do: the tools' own logins obid
// offers, each account's sign-in under way and what went wrong where (an account's row, or ADD:
// adding one)
type Signing = 'opening' | (SignIn & { code?: 'sending' | 'sent' });
function useSignIns(accounts: Account[], setAccounts: (a: Account[]) => void) {
  // the tools' own logins on this Mac that aren't accounts (found, or removed here): only
  // GET /v1/status has them, read as the page opens and after a change (obid also reads the usage
  // shown here then, which comes in its `accounts` event)
  const [offered, setOffered] = useState<NonNullable<Status['availableLogins']>>([]);
  const look = () => { if (onObid) api.status().then((s) => setOffered(s.availableLogins ?? []), () => {}); };
  useEffect(look, []);
  // sign-ins under way, by account: opening, then its page (with Codex's code, or Claude Code's to
  // paste), then that code on its way ('sending') or sent with no answer in time ('sent')
  const [signing, setSigning] = useState<Record<string, Signing>>({});
  const put = (id: string, v?: Signing) => setSigning(({ [id]: _, ...rest }) => (v ? { ...rest, [id]: v } : rest));
  // what obid said went wrong, where it went wrong: on the account's row, or under Add an account
  const [problems, setProblems] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const tried = async (where: string, f: () => Promise<unknown>) => {
    setProblems(({ [where]: _, ...rest }) => rest);
    if (where === ADD) setBusy(true);
    try { await f(); } catch (e) { setProblems((p) => ({ ...p, [where]: e instanceof Error ? e.message : String(e) })); } finally { if (where === ADD) setBusy(false); }
  };
  const signIn = (id: string) => tried(id, async () => {
    put(id, 'opening');
    try {
      const page = await api.signIn(id);
      put(id, page);
      window.app?.openExternal(page.url);
    } catch (e) { put(id); throw e; }
  });
  // Claude Code's code: signed in; refused (422: that sign-in is over, so Sign In opens a new link);
  // or no answer in time (504): it may still sign in, until its page runs out or Cancel
  const sendCode = (id: string, page: SignIn, code: string) => tried(id, async () => {
    put(id, { ...page, code: 'sending' });
    try {
      await api.signInCode(id, code.trim());
      put(id);
    } catch (e) {
      if (e instanceof ApiError && e.status === 422) { put(id); await api.endSignIn(id).catch(() => {}); } else put(id, { ...page, code: 'sent' });
      throw e;
    }
  });
  const cancel = (id: string) => { put(id); void tried(id, () => api.endSignIn(id)); };
  // signed in (Codex and Cursor end by themselves; obid's `accounts` event says so)
  useEffect(() => { for (const a of accounts) if (a.signedIn && signing[a.id]) put(a.id); }, [accounts]);
  // a sign-in whose page ran out (15 minutes) without signing in: Sign In again
  useEffect(() => {
    const timers = Object.entries(signing).flatMap(([id, s]) => {
      const end = typeof s === 'object' ? Date.parse(s.expiresAt ?? '') : NaN;
      return Number.isNaN(end) ? [] : [setTimeout(() => put(id), end - Date.now())];
    });
    return () => timers.forEach(clearTimeout);
  }, [signing]);
  // taking one off, after its window says what that does (by id: one gone meanwhile has no window)
  const [removing, setRemoving] = useState<string | null>(null);
  const remove = (a: Account) => (onObid ? setRemoving(a.id) : setAccounts(accounts.filter((x) => x.id !== a.id)));
  const gone = accounts.find((x) => x.id === removing);
  const removeWindow = gone && <ConfirmWindow brand={AGENTS[gone.agent].brand} title={`Remove ${gone.name}?`} text={removeNote(gone)} action="Remove"
    onConfirm={() => void tried(gone.id, () => api.removeAccount(gone.id).then(() => { put(gone.id); look(); }))} onClose={() => setRemoving(null)} />;
  return { offered, look, signing, put, problems, busy, tried, signIn, sendCode, cancel, remove, removeWindow };
}

function Accounts() {
  const [accounts, setAccounts] = useAccounts();
  const { offered, look, signing, problems, busy, tried, signIn, sendCode, cancel, remove, removeWindow } = useSignIns(accounts, setAccounts);
  const [agent, setAgent] = useState<Account['agent']>('claude');
  const [name, setName] = useState('');
  const [payer, setPayer] = useState('');
  const add = () => tried(ADD, async () => {
    const n = name.trim(), p = payer.trim();
    const a = onObid ? await api.addAccount({ agent, name: n, ...(p ? { payer: p } : {}) }) : null;
    setName(''); setPayer('');
    // its sign-in's errors show on its row
    if (a) return void signIn(a.id);
    setAccounts([...accounts, { id: `a${Date.now()}`, agent, name: n, payer: p || 'You', used: 0, resets: 'Resets in 7 days' }]);
  });
  const roomiest = accounts.filter((a) => a.used != null).sort((a, b) => a.used! - b.used!)[0];
  return <>
    <Group i={1} title="Accounts on this Mac">
      <Row label={roomiest ? <span className="set-plain">Most room: {roomiest.name} on {AGENTS[roomiest.agent].name}, {100 - roomiest.used!}% left this week</span> : <span className="set-none">None yet</span>} />
    </Group>
    {(['claude', 'codex', 'cursor'] as const).filter((g) => accounts.some((a) => a.agent === g)).map((g, k) => (
      <Group key={g} i={2 + k} title={AGENTS[g].name}>
        {accounts.filter((a) => a.agent === g).map((a) => {
          const s = signing[a.id], tool = AGENTS[g].name, own = ownLogin(a);
          return (
            <div key={a.id} className="set-row set-usage">
              <BrandMark brand={AGENTS[g].brand} size={16} mono />
              <span className="set-label">
                {a.name}
                <span className="set-about">{!s ? `Paid by ${a.payer} · ${a.resets}${own && a.signedIn === false && !a.label ? `: sign in with ${tool} itself on this Mac` : ''}`
                  : s === 'opening' ? `Opening ${tool}’s sign-in page`
                  : s.code ? `Signing in to ${tool}`
                  : `Finish on ${tool}’s sign-in page${s.userCode ? ` · enter ${s.userCode}` : ''}`}</span>
                {s !== 'opening' && s?.pasteCode && !s.code && <PasteCode onCode={(code) => sendCode(a.id, s, code)} />}
                {problems[a.id] && <span className="set-about set-problem" role="alert">{problems[a.id]}</span>}
                {a.used != null && <span className="set-meter" role="meter" aria-valuenow={a.used} aria-valuemin={0} aria-valuemax={100} aria-label={`${a.used}% used`}>
                  <span className={fullness(a.used)} style={{ width: `${a.used}%` }} />
                </span>}
              </span>
              <span className="set-control set-control-top">
                {a.used != null && <span className="set-value">{a.used}% used</span>}
                {/* (obid signs in only accounts with a folder of their own) */}
                {a.signedIn === false && !own && (s && s !== 'opening' && s.code !== 'sending'
                  ? <button className="set-button" onClick={() => cancel(a.id)}>Cancel</button>
                  : <button className="set-button" disabled={!!s} onClick={() => signIn(a.id)}>{s ? 'Waiting…' : 'Sign In'}</button>)}
                <button className="set-button is-danger" onClick={() => remove(a)}>Remove</button>
              </span>
            </div>
          );
        })}
      </Group>
    ))}
    <Group i={5} title="Add an account" footer={problems[ADD] && <span className="set-problem" role="alert">{problems[ADD]}</span>}>
      <Row label="Tool"><Select value={agent} onChange={(v) => setAgent(v as Account['agent'])} label="Tool" options={(Object.keys(AGENTS) as Agent[]).map((id) => [id, AGENTS[id].name] as const)} /></Row>
      <Row label="Name"><TextField value={name} onChange={setName} placeholder="Personal" label="Account name" /></Row>
      <Row label="Paid by"><TextField value={payer} onChange={setPayer} placeholder="Wellington" label="Paid by" /></Row>
      <Row label="">
        {onObid && offered.some((l) => l.agent === agent) && (
          <button className="set-button" disabled={busy} onClick={() => tried(ADD, () => api.addAccount({ agent, default: true }).then(look))}>Use {AGENTS[agent].name}’s Own Login</button>
        )}
        <button className="set-button" disabled={!name.trim() || busy} onClick={add}>Add Account</button>
      </Row>
    </Group>
    {removeWindow}
  </>;
}

// What removing one of obid's accounts does, and what it leaves (the Swift app's words), under
// its window's Remove X?
function removeNote(a: Account) {
  const tool = AGENTS[a.agent].name, provider = { claude: 'Anthropic', codex: 'OpenAI', cursor: 'Cursor' }[a.agent];
  if (ownLogin(a)) return `Oby stops using ${tool}’s own login on this Mac; nothing of it is deleted. Bring it back with Use ${tool}’s Own Login.`;
  // one obid made has eight hex digits; any other is in obid's configuration, whose folder stays
  const made = /^[0-9a-f]{8}$/.test(a.id);
  return `Oby stops using it, and deletes the folder it made for it, if any. Your account with ${provider} stays signed in there until you sign it out.`
    + (a.agent === 'claude' ? ' Its Keychain item stays too; Keychain Access removes it.' : '')
    + (made ? '' : ' It’s in obid’s configuration, so it can’t be added back here.');
}

// Claude Code's sign-in ends on a page that shows a code: pasted here (Settings › Accounts, the
// setup's card), it goes to its login (obid's)
export function PasteCode({ onCode }: { onCode: (code: string) => void }) {
  const [code, setCode] = useState('');
  return (
    <form className="setup-code" onSubmit={(e) => { e.preventDefault(); onCode(code); }}>
      <input className="set-field" value={code} onChange={(e) => setCode(e.target.value)} placeholder="Paste the code from the page" aria-label="Sign-in code" spellCheck={false} autoFocus />
      <button className="set-button" disabled={!code.trim()}>Send</button>
    </form>
  );
}

// Sharing (a preview for Sangam): this Mac's spare time, and a part of an
// account's usage, for the people in your Obysphere. Both off until you turn
// them on.
//
// This Mac: their jobs run here in a sealed box of their own (its cores,
// memory and disk capped), with their own AI accounts: their logins stay on
// their Macs, and the box reaches their account only through their Oby. Only
// when you're away or in the hours you pick, never on battery. Stop hands a
// job back to its owner's Oby with its notes.
//
// Your usage: only an account the company pays by use (an API key) can be
// shared: the providers' terms keep a personal plan, and each Team seat, to
// one person (SHARING). A shared job runs here, on that account, and the
// person gets the results; the key never leaves this Mac.
type Plan = 'personal' | 'seat' | 'api';
const SHARING: Record<Plan, string> = {
  personal: 'A personal plan is for you alone: its terms don’t let anyone else’s work run on it.',
  seat: 'A Team seat is one person’s. For more, Deskree’s admin can add extra usage to someone’s own seat.',
  api: '',
};
type Guest = { id: string; who: string; title: string; repo: string; agent: Agent; took: string; uses: string };
const GUESTS: Guest[] = [
  { id: 'g1', who: 'Sangam', title: 'Fix the login retry', repo: 'deskree-inc/tetrix-ee', agent: 'claude', took: '12 min', uses: '3 cores · 5.1 GB' },
];
// who: nobody, everyone or one person; share: % of the account's monthly budget
type Grant = { account: string; who: string; share: number; used: number };
function Sharing() {
  const members = useSpheres()[0]?.members ?? [];
  const [accounts] = useAccounts();
  const [mac, setMac] = useSetting('share.mac', false);
  const [when, setWhen] = useSetting('share.when', 'away');
  const [who, setWho] = useSetting('share.who', 'ask');
  const [chosen, setChosen] = useSetting<string[]>('share.chosen', []);
  const [jobs, setJobs] = useSetting('share.jobs', '1');
  const [size, setSize] = useSetting('share.size', 'small');
  const [work, setWork] = useSetting('share.work', 'any');
  const [guests, setGuests] = useSetting('share.guests', GUESTS);
  const [grants, setGrants] = useSetting<Grant[]>('share.grants', [{ account: 'a4', who: 'Sangam', share: 30, used: 9 }]);
  const grant = (account: string, patch: Partial<Grant>) => {
    const had = grants.some((g) => g.account === account);
    setGrants(had ? grants.map((g) => (g.account === account ? { ...g, ...patch } : g)) : [...grants, { account, who: 'nobody', share: 20, used: 0, ...patch }]);
  };
  const shareable = accounts.filter((a) => a.plan === 'api');
  const kept = accounts.filter((a) => a.plan !== 'api');
  return <>
    <Group i={1} title="This Mac" footer={mac
      ? 'Their jobs run in a sealed box on this Mac, with their own AI accounts: they can’t see your files, logins, Keychain or network, and their logins never come here. When you’re back, their jobs pause, and after 10 minutes go back to their Obys with their notes.'
      : 'Let people in Deskree run jobs on this Mac while you’re not using it, with their own AI accounts.'}>
      <Row label="Share this Mac with Deskree"><Switch on={mac} onChange={setMac} label="Share this Mac with Deskree" /></Row>
      {mac && <>
        <Row label="When" about="Away: no keyboard or mouse for 10 minutes. Never on battery, in Low Power Mode or when it’s hot.">
          <Select value={when} onChange={setWhen} label="When" options={[['away', 'When I’m away'], ['nights', 'Nights and weekends'], ['both', 'Away, nights and weekends']]} />
        </Row>
        <Row label="Who">
          <Select value={who} onChange={setWho} label="Who" options={[['ask', 'Everyone, ask me first'], ['everyone', 'Everyone in Deskree'], ['chosen', 'Only people I choose']]} />
        </Row>
        {who === 'chosen' && members.map((m) => (
          <div key={m} className="set-row set-member">
            <OwnerAvatar name={m} size={28} badge={false} />
            <span className="set-label">{m}</span>
            <Switch on={chosen.includes(m)} onChange={(on) => setChosen(on ? [...chosen, m] : chosen.filter((x) => x !== m))} label={`Share with ${m}`} />
          </div>
        ))}
        <Row label="Jobs at once"><Select value={jobs} onChange={setJobs} label="Jobs at once" options={[['1', '1'], ['2', '2'], ['3', '3']]} /></Row>
        <Row label="Each job gets">
          <Select value={size} onChange={setSize} label="Each job gets" options={[['small', '4 cores · 8 GB · 30 GB of disk'], ['large', '6 cores · 12 GB · 60 GB of disk']]} />
        </Row>
        <Row label="Work">
          <Select value={work} onChange={setWork} label="Work" options={[['any', 'Any job in Deskree’s repositories'], ['reviews', 'Reviews and checks only']]} />
        </Row>
      </>}
    </Group>
    {mac && (
      <Group i={2} title="On this Mac now" footer="Every job someone runs here is in Deskree’s signed log, with who ran it and for how long.">
        {guests.length ? guests.map((g) => (
          <div key={g.id} className="set-row set-member">
            <OwnerAvatar name={g.who} size={28} badge={false} />
            <span className="set-label">{g.who} · {g.title}<span className="set-about">{AGENTS[g.agent].name} on {g.who}’s account · {g.repo} · {g.took} · {g.uses}</span></span>
            <button className="set-button is-danger set-member-action" onClick={() => setGuests(guests.filter((x) => x !== g))}>Stop</button>
          </div>
        )) : <Row label={<span className="set-none">Nobody right now</span>} />}
        {guests.length > 1 && <Row label=""><button className="set-button is-danger" onClick={() => setGuests([])}>Stop All</button></Row>}
      </Group>
    )}
    <Group i={3} title="Share your usage" footer="Only an account Deskree pays by use can be shared. A shared job runs on this Mac, on that account: they get the results and the pull request, and its key never leaves this Mac. Once their share is used, Oby takes no more of their jobs on it and finishes the one running.">
      {shareable.length ? shareable.map((a) => {
        const g = grants.find((x) => x.account === a.id) ?? { account: a.id, who: 'nobody', share: 20, used: 0 };
        const whom = g.who === 'everyone' ? 'Everyone' : g.who;
        return (
          <div key={a.id} className="set-row set-usage">
            <BrandMark brand={AGENTS[a.agent].brand} size={16} mono />
            <span className="set-label">
              {AGENTS[a.agent].name} · {a.name}
              <span className="set-about">{g.who === 'nobody' ? `Not shared · ${a.resets}` : `${whom} used ${g.used}% of ${g.share}% this month`}</span>
              {g.who !== 'nobody' && (
                <span className="set-meter" role="meter" aria-valuenow={g.used} aria-valuemin={0} aria-valuemax={g.share} aria-label={`${g.used}% of ${g.share}% used`}>
                  <span className={fullness((100 * g.used) / g.share)} style={{ width: `${(100 * g.used) / g.share}%` }} />
                </span>
              )}
            </span>
            <span className="set-control set-control-top set-share">
              <Select value={g.who} onChange={(v) => grant(a.id, { who: v })} label={`Share ${a.name} with`} options={[['nobody', 'Nobody'], ['everyone', 'Everyone'], ...members.map((m) => [m, m] as const)]} />
              {g.who !== 'nobody' && <Select value={String(g.share)} onChange={(v) => grant(a.id, { share: Number(v) })} label="How much of its month" options={[10, 20, 30, 50, 75, 100].map((n) => [String(n), `${n}% of its month`] as const)} />}
            </span>
          </div>
        );
      }) : <Row label={<span className="set-none">No account Deskree pays by use</span>} />}
    </Group>
    <Group i={4} title="Kept for you">
      {kept.map((a) => (
        <div key={a.id} className="set-row set-usage">
          <BrandMark brand={AGENTS[a.agent].brand} size={16} mono />
          <span className="set-label">{AGENTS[a.agent].name} · {a.name}<span className="set-about">{SHARING[a.plan ?? 'personal']}</span></span>
        </div>
      ))}
    </Group>
  </>;
}

// Models: which model and effort new jobs on this Mac use, for each tool
function Models() {
  const [claude, setClaude] = useSetting('claude.model', 'claude-opus-5-5');
  const [claudeEffort, setClaudeEffort] = useSetting('claude.effort', 'xhigh');
  const [claudeComplex, setClaudeComplex] = useSetting('claude.complexEffort', 'xhigh');
  const [codex, setCodex] = useSetting('codex.model', 'gpt-6.1-sol');
  const [codexEffort, setCodexEffort] = useSetting('codex.effort', 'high');
  const [codexComplex, setCodexComplex] = useSetting('codex.complexEffort', 'high');
  const [cursor, setCursor] = useSetting('cursor.model', 'auto');
  const codexEfforts = EFFORTS.filter(([id]) => id !== 'max');
  return <>
    <Group i={1} title="Claude Code on this Mac">
      <Row label="Model"><Select value={claude} onChange={setClaude} label="Claude Code model" options={[['claude-fable-5-1', 'Fable 5.1'], ['claude-opus-5-5', 'Opus 5.5'], ['claude-sonnet-5-5', 'Sonnet 5.5'], ['claude-haiku-5-5', 'Haiku 5.5']]} /></Row>
      <Row label="Effort"><Select value={claudeEffort} onChange={setClaudeEffort} label="Claude Code effort" options={EFFORTS} /></Row>
      <Row label="Complex job effort"><Select value={claudeComplex} onChange={setClaudeComplex} label="Claude Code complex job effort" options={EFFORTS} /></Row>
    </Group>
    <Group i={2} title="Codex on this Mac">
      <Row label="Model"><Select value={codex} onChange={setCodex} label="Codex model" options={[['gpt-6.1-sol', 'gpt-6.1-sol'], ['gpt-6-astra', 'GPT-6-Astra']]} /></Row>
      <Row label="Effort"><Select value={codexEffort} onChange={setCodexEffort} label="Codex effort" options={codexEfforts} /></Row>
      <Row label="Complex job effort"><Select value={codexComplex} onChange={setCodexComplex} label="Codex complex job effort" options={codexEfforts} /></Row>
    </Group>
    <Group i={3} title="Cursor on this Mac" footer="New jobs on this Mac use these. Oby’s brain stays on Haiku: it plans and answers in the conversation, so it should be quick.">
      <Row label="Model"><Select value={cursor} onChange={setCursor} label="Cursor model" options={[['auto', 'Auto'], ['claude-opus-5-5', 'Opus 5.5'], ['gpt-6.1-sol', 'gpt-6.1-sol']]} /></Row>
    </Group>
  </>;
}

// Agents: which agent new tasks go to; each agent added (an account: its 5-hour session and its
// week) with the model and efforts its jobs run on, and its Remove; then Add an agent. obid keeps
// the default agent and the models (#1573, #1574; docs/contracts/agents.md) once its status says
// defaultAccount; an older obid, and the mock, keep them here (agent.default, models.mine)
const EFFORT_NAMES: Record<string, string> = { low: 'Low', medium: 'Medium', high: 'High', xhigh: 'Extra high', max: 'Max', ultra: 'Ultra' };
const MOCK_MODELS: Models = {
  claude: { model: 'claude-opus-5-5', effort: 'xhigh', complexEffort: 'xhigh' },
  codex: { model: 'gpt-6.1-sol', effort: 'high', complexEffort: 'high' },
  cursor: { model: 'auto' },
};
const ALL_EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max'];
const MOCK_OPTIONS: ModelOptions = {
  claude: { models: [{ id: 'claude-fable-5-1', name: 'Fable 5.1' }, { id: 'claude-opus-5-5', name: 'Opus 5.5' }, { id: 'claude-sonnet-5-5', name: 'Sonnet 5.5' }, { id: 'claude-haiku-5-5', name: 'Haiku 5.5' }], efforts: ALL_EFFORTS, complexEfforts: ALL_EFFORTS },
  codex: { models: [{ id: 'gpt-6.1-sol', name: 'gpt-6.1-sol', efforts: ALL_EFFORTS.slice(0, 4) }, { id: 'gpt-6-astra', name: 'GPT-6-Astra', efforts: ALL_EFFORTS.slice(0, 4) }], complexEfforts: ALL_EFFORTS.slice(0, 4) },
  cursor: { models: [{ id: 'auto', name: 'Auto' }, { id: 'claude-opus-5-5', name: 'Opus 5.5' }, { id: 'gpt-6.1-sol', name: 'gpt-6.1-sol' }], complexEfforts: [] },
};
type Tuning = { model: string; effort?: string; complexEffort?: string };
// obid keeps Agents' choices (its status says defaultAccount, even null)
const keepsAgents = (s: Status | null | undefined): s is Status => onObid && !!s && 'defaultAccount' in s;
// an account's own choices, as obid lists them
const tuningOf = (a: { model?: string; effort?: string; complexEffort?: string } | undefined): Partial<Tuning> =>
  Object.fromEntries((['model', 'effort', 'complexEffort'] as const).filter((k) => a?.[k] != null).map((k) => [k, a![k]]));
// each agent's model and efforts (an account's: Personal and Work on Claude Code each their own),
// and the choices. Each starts from what obid's status says the tool runs; obid keeps a change on
// the account and offers the choices; an older obid and the mock keep it here, with the mock's
// choices
function useModels(): [(x: Account) => Tuning, ModelOptions, (id: string, change: Partial<Tuning>) => Promise<void>] {
  const status = useLive().status;
  const live = status?.models as Partial<Models> | undefined;
  const [mine, setMine] = useKeptSetting<Record<string, Partial<Tuning>>>('models.mine', {});
  const [options, setOptions] = useState(MOCK_OPTIONS);
  const kept = keepsAgents(status);
  useEffect(() => { if (kept) api.modelOptions().then(setOptions, () => {}); }, [kept]);
  // each tool's on its own: what obid says it runs over the mock's, so a key obid leaves out isn't lost
  const base = Object.fromEntries((Object.keys(MOCK_MODELS) as Agent[]).map((a) => [a, { ...MOCK_MODELS[a], ...(onObid ? live?.[a] : undefined) }])) as Models;
  const of = (x: Account): Tuning => ({ ...(base[x.agent] ?? { model: 'auto' }),
    ...(kept ? tuningOf(status.accounts.find((a) => a.id === x.id)) : mine[x.id]) });
  const tune = async (id: string, change: Partial<Tuning>) => {
    if (!kept) return setMine({ ...mine, [id]: { ...mine[id], ...change } });
    const a = await api.tuneAccount(id, change);
    patchStatus({ accounts: status.accounts.map((x) => (x.id === a.id ? a : x)) });
  };
  return [of, options, tune];
}
// the account new tasks go to: obid's, else kept here. A change obid refuses is reported as any
// failure of the app's is (live.ts, say), and the menu stays on obid's
function useDefaultAccount(): [string | null, (id: string) => Promise<void>] {
  const status = useLive().status;
  const [chosen, setChosen] = useKeptSetting<string | null>('agent.default', null);
  if (!keepsAgents(status)) return [chosen, async (id) => setChosen(id)];
  return [status.defaultAccount ?? null, async (id) => patchStatus(await api.setDefaultAccount(id))];
}
// What was chosen here before obid kept it (agent.default, models.mine): moved to obid once its
// status says it keeps them, then forgotten here, so nobody's choice is lost (AGENTS-7). A choice
// for an account that's gone goes; one obid refused stays here, tried again at the next connection.
// A default obid has already (set on another device) wins. Whether anything moved.
export async function moveKeptChoices(status: Status): Promise<boolean> {
  if (!keepsAgents(status)) return false;
  const ids = new Set(status.accounts.map((a) => a.id));
  let moved = false;
  const chosen = readKept<string | null>('agent.default');
  if (chosen != null) {
    if (status.defaultAccount == null && ids.has(chosen)) {
      try { await api.setDefaultAccount(chosen); moved = true; keep('agent.default', null); } catch { /* next time */ }
    } else keep('agent.default', null);
  }
  const mine = readKept<Record<string, Partial<Tuning>>>('models.mine');
  if (mine && Object.keys(mine).length) {
    const left: Record<string, Partial<Tuning>> = {};
    for (const [id, t] of Object.entries(mine)) {
      if (!ids.has(id)) continue;
      try { await api.tuneAccount(id, tuningOf(t)); moved = true; } catch { left[id] = t; }
    }
    keep('models.mine', left);
  }
  return moved;
}
// a pop-up's choices, the one set kept even when the choices don't (yet) have it
const choices = (list: { id: string; name: string }[], set: string | undefined) =>
  [...(set && !list.some((m) => m.id === set) ? [{ id: set, name: set }] : []), ...list].map((m) => [m.id, m.name] as const);
const effortChoices = (list: string[] | undefined, set: string | undefined) =>
  choices((list ?? []).map((e) => ({ id: e, name: EFFORT_NAMES[e] ?? e })), set);

function Agents() {
  const [accounts, setAccounts] = useAccounts();
  const [tuning, options, tune] = useModels();
  const [chosen, setChosen] = useDefaultAccount();
  // signing in, as Settings › Accounts does (useSignIns): its states, its problems, the tool's own login
  const { offered, look, signing, problems, busy, tried, signIn, sendCode, cancel, remove, removeWindow } = useSignIns(accounts, setAccounts);
  // Add an agent: a provider, and an alias of your own for it (several of one are fine)
  const [agent, setAgent] = useState<Agent>('claude');
  const [name, setName] = useState('');
  const add = () => tried(ADD, async () => {
    const n = name.trim();
    const a = onObid ? await api.addAccount({ agent, name: n }) : null;
    setName('');
    // its sign-in's errors show on its card
    if (a) return void signIn(a.id);
    setAccounts([...accounts, { id: `a${Date.now()}`, agent, name: n, payer: 'You', used: 0, resets: 'Resets in 7 days', signedIn: true }]);
  });
  const ids = Object.keys(AGENTS) as Agent[];
  // one card per agent you added, in the providers' order
  const agents = ids.flatMap((a) => accounts.filter((x) => x.agent === a));
  const ready = agents.filter((x) => x.signedIn !== false);
  const fallback = ready.find((x) => x.id === chosen) ?? ready[0];
  return <>
    <Group i={1} title="Default agent">
      {fallback
        ? <Row label="New tasks go to">
            <Select wide value={fallback.id} onChange={(v) => void setChosen(v).catch(say)} label="Default agent" options={ready.map((x) => [x.id, `${x.name} · ${AGENTS[x.agent].name}`] as const)} />
          </Row>
        : <Row label={<span className="set-none">No agents yet</span>} />}
    </Group>
    {agents.map((x, k) => {
      const a = x.agent, t = tuning(x), o = options[a];
      const efforts = a === 'codex' ? o?.models.find((m) => m.id === t.model)?.efforts : o?.efforts;
      const out = x.signedIn === false, sign = signing[x.id], tool = AGENTS[a].name, own = ownLogin(x);
      // the bar: the 5-hour session; the week, in words under it
      const week = weekWords(x);
      return (
        <Group key={x.id} i={2 + k} title={x.name}>
          <div className="set-row set-usage">
            <BrandMark brand={AGENTS[a].brand} size={16} mono />
            <span className="set-label">
              <span className="set-usage-name">
                {tool}
                {!out && !sign && x.session != null && <span className="set-value">{x.session}% used</span>}
              </span>
              {sign
                ? <span className="set-about">{sign === 'opening' ? `Opening ${tool}’s sign-in page` : sign.code ? `Signing in to ${tool}` : `Finish on ${tool}’s sign-in page${sign.userCode ? ` · enter ${sign.userCode}` : ''}`}</span>
                : out ? <span className="set-about">{x.label ?? 'Not signed in'}{own && !x.label ? `: sign in with ${tool} itself on this Mac` : ''}</span>
                : x.session != null ? <>
                    <span className="set-about">5-hour session{x.sessionResets ? ` · resets ${x.sessionResets}` : ''}</span>
                    <span className="set-meter" role="meter" aria-valuenow={x.session} aria-valuemin={0} aria-valuemax={100} aria-label={`${x.session}% of the 5-hour session used`}>
                      <span className={fullness(x.session)} style={{ width: `${x.session}%` }} />
                    </span>
                    <span className="set-about is-quiet">{week}</span>
                  </>
                : <span className="set-about">{week}</span>}
              {sign && sign !== 'opening' && sign.pasteCode && !sign.code && <PasteCode onCode={(code) => sendCode(x.id, sign, code)} />}
              {problems[x.id] && <span className="set-about set-problem" role="alert">{problems[x.id]}</span>}
            </span>
            {/* (obid signs in only the accounts with a folder of their own: not the tool's own login) */}
            {out && !own && <span className="set-control set-control-top">
              {sign && sign !== 'opening' && sign.code !== 'sending'
                ? <button className="set-button" onClick={() => cancel(x.id)}>Cancel</button>
                : <button className="set-button" disabled={!!sign} onClick={() => signIn(x.id)}>{sign ? 'Waiting…' : 'Sign In'}</button>}
            </span>}
          </div>
          {/* the model and, after it in grey, how hard it thinks (one menu for both); complex tasks get an effort of their own */}
          <Row label="Model">
            <ModelPicker label={`${x.name} model and effort`} models={choices(o?.models ?? [], t.model)} model={t.model} onModel={(v) => void tune(x.id, { model: v }).catch(say)}
              efforts={a === 'cursor' ? [] : effortChoices(efforts, t.effort)} effort={t.effort} onEffort={(v) => void tune(x.id, { effort: v }).catch(say)} />
          </Row>
          {a !== 'cursor' && <Row label="Effort on complex tasks">
            <Select wide value={t.complexEffort ?? ''} onChange={(v) => void tune(x.id, { complexEffort: v }).catch(say)} label={`${x.name} complex task effort`} options={effortChoices(o?.complexEfforts, t.complexEffort)} />
          </Row>}
          {/* the card's own action, on the left; red: it can't be undone (its sign-in goes with it) */}
          <div className="set-row is-action">
            <button className="set-button is-danger" onClick={() => remove(x)}>Remove</button>
          </div>
        </Group>
      );
    })}
    <Group i={2 + agents.length} title="Add an agent" footer={problems[ADD] ? <span className="set-problem" role="alert">{problems[ADD]}</span> : 'The alias is only for you, to tell your agents apart.'}>
      <Row label="Provider"><Select wide value={agent} onChange={(v) => setAgent(v as Agent)} label="Provider" options={ids.map((a) => [a, AGENTS[a].name] as const)} /></Row>
      <Row label="Alias"><TextField wide value={name} onChange={setName} placeholder="Work, Personal…" label="Alias" /></Row>
      {/* the card's action, not a row's: on the left */}
      <div className="set-row is-action">
        <button className="set-button is-primary" disabled={!name.trim() || busy} onClick={() => void add()}>{busy ? 'Adding…' : 'Add Agent'}</button>
        {/* the tool's own login, found on this Mac (or taken off here): back in one click, no alias needed */}
        {onObid && offered.some((l) => l.agent === agent) && (
          <button className="set-button" disabled={busy} onClick={() => tried(ADD, () => api.addAccount({ agent, default: true }).then(look))}>Use {AGENTS[agent].name}’s Own Login</button>
        )}
      </div>
    </Group>
    {removeWindow}
  </>;
}

// Vault: the secrets Oby keeps in the keychain, by name, never their values
function Vault() {
  const [names, setNames] = useSetting('vault', ['figma-token', 'sentry-dsn', 'staging-api-key']);
  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  return <>
    <Group i={1} title="Secrets">
      {names.length
        ? names.map((n) => (
          <Row key={n} label={<span className="tok tok-code">{n}</span>}>
            <button className="set-button is-danger" onClick={() => setNames(names.filter((x) => x !== n))}>Remove</button>
          </Row>
        ))
        : <Row label={<span className="set-none">No secrets yet</span>} />}
    </Group>
    <Group i={2} title="Add a secret" footer="Oby uses a secret only in its own calls, for a connection you made. No agent sees it, and the secret scan stops anything outgoing that holds it, plain or encoded.">
      <Row label="Name"><TextField value={name} onChange={setName} placeholder="staging-api-key" label="Secret name" /></Row>
      <Row label="Value"><TextField value={value} onChange={setValue} placeholder="Typed once, never shown again" label="Secret value" secret /></Row>
      <Row label="">
        <button className="set-button" disabled={!name.trim() || !value} onClick={() => {
          setNames([...new Set([...names, name.trim()])].sort());
          setName(''); setValue('');
        }}>Add Secret</button>
      </Row>
    </Group>
  </>;
}

// About in the Mac app: the version, and Oby's own updates in Sangam's rows (main/updates.ts
// decides, shared/updates.ts; docs/contracts/updates.md)
function Updates() {
  const app = window.app!;
  const [kept, setKept] = useState<{ auto: boolean; update: Update } | null>(null);
  useEffect(() => {
    void app.updates!().then(setKept);
    return app.onUpdate!((update) => setKept((k) => k && { ...k, update }));
  }, []);
  const ask = (a: 'check' | 'install' | boolean) => {
    if (typeof a === 'boolean') setKept((k) => k && { ...k, auto: a });
    void app.updates!(a).then(setKept);
  };
  const auto = kept?.auto ?? true, u: Update = kept?.update ?? { state: 'idle' };
  const found = u.state === 'available' || u.state === 'downloading' || u.state === 'ready' ? u : null;
  return (
    <Group i={1} footer={auto ? 'Oby downloads updates in the background and installs them when you quit.' : undefined}>
      <Row label="Version"><Value>{version}</Value></Row>
      <Row label="Update automatically"><Switch on={auto} onChange={ask} label="Update automatically" /></Row>
      {found && (
        <Row label={`Oby ${found.version} is available`} about={found.date ? `Released ${new Date(found.date).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}` : undefined}>
          <button className="set-button is-primary" disabled={found.state === 'downloading' && found.restart} onClick={() => ask('install')}>Update and Restart</button>
        </Row>
      )}
      {/* the card's own action: on the left, always there to look again (Wellington, 2026-10-30), and
          what the last look found beside it */}
      <div className="set-row is-action">
        <button className="set-button" disabled={u.state !== 'idle' && u.state !== 'current'} onClick={() => ask('check')}>Check for Updates</button>
        {u.state === 'current' && <Value>Oby is up to date</Value>}
      </div>
    </Group>
  );
}

function About() {
  // On obid, the real updater (Updates). The rest is the mock's, in a browser: checking says it's
  // up to date while it updates itself; with that off, it finds 0.9.3
  const [auto, setAuto] = useSetting('updates.auto', true);
  const [found, setFound] = useState<'none' | 'current' | 'update'>('none');
  if (onObid && window.app?.updates) return <Updates />;
  if (onObid) return <Group i={1}><Row label="Version"><Value>{version}</Value></Row></Group>;
  return <>
    <Group i={1} footer={auto ? 'Oby downloads updates in the background and installs them when you quit.' : undefined}>
      <Row label="Version"><Value>0.9.2 (412)</Value></Row>
      <Row label="Update automatically"><Switch on={auto} onChange={(on) => { setAuto(on); setFound('none'); }} label="Update automatically" /></Row>
      {found === 'update' && (
        <Row label="Oby 0.9.3 is available" about="Released Oct 27">
          <button className="set-button is-primary" onClick={() => setFound('current')}>Update and Restart</button>
        </Row>
      )}
      {/* the card's own action: on the left, always there to look again, and what it found beside it */}
      <div className="set-row is-action">
        <button className="set-button" disabled={found === 'update'} onClick={() => setFound(auto ? 'current' : 'update')}>Check for Updates</button>
        {found === 'current' && <Value>Oby is up to date</Value>}
      </div>
    </Group>
  </>;
}

const PAGES: Record<string, () => React.ReactNode> = { general: General, agents: Agents, runner: Runner, sharing: Sharing, remote: Remote, accounts: Accounts, models: Models, connectors: Connectors, vault: Vault, about: About };

// The default mode, with what it lets the agent do under it
function ModeRow({ mode, setMode, modes = MODES }: { mode: string; setMode: (m: string) => void; modes?: typeof MODES }) {
  return (
    <Row label="Default mode" about={modes.find((m) => m.id === mode)?.help}>
      <Select value={mode} onChange={setMode} label="Default mode" options={modes.map((m) => [m.id, m.label] as const)} />
    </Row>
  );
}

// The rules Always allow (or a deny) saved, each with Remove
type Rule = { id?: string; decision: 'Allowed' | 'Denied'; repo: string; rule: string };
function Rules({ id, initial }: { id: string; initial: Rule[] }) {
  const [rules, setRules] = useSetting(id, initial);
  return <RuleRows rules={rules} onRemove={(r) => setRules(rules.filter((x) => x !== r))} />;
}
function RuleRows({ rules, onRemove }: { rules: Rule[]; onRemove: (r: Rule) => void }) {
  // none saved yet: no row (the group's note says how one gets here)
  if (!rules.length) return null;
  return rules.map((r) => (
    <Row key={r.id ?? `${r.repo} ${r.rule}`} label={<span className="tok tok-code">{r.rule}</span>} about={`${r.decision} · ${r.repo}`}>
      <button className="set-button is-danger" onClick={() => onRemove(r)}>Remove</button>
    </Row>
  ));
}

// A titled group of rows on a card, a note under it
export function Group({ title, footer, i, children }: { title?: string; footer?: React.ReactNode; i: number; children: React.ReactNode }) {
  return (
    <section className="set-group rise" style={{ '--i': i } as React.CSSProperties}>
      {title && <h2 className="set-head">{title}</h2>}
      <div className="set-card">{children}</div>
      {footer && <p className="set-foot">{footer}</p>}
    </section>
  );
}

// One setting: its name (and a line on it) on the left, its control on the right
export function Row({ label, about, children }: { label: React.ReactNode; about?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="set-row">
      <span className="set-label">
        {label}
        {about && <span className="set-about">{about}</span>}
      </span>
      {children && <span className="set-control">{children}</span>}
    </div>
  );
}

// A model and, after it in grey, its effort: one button, and one menu with the models, then
// Effort (its pick on the right), which opens the efforts beside it, as macOS's submenus do: on
// hover, or a click. none: models only, as Cursor's (Settings › Agents)
function ModelPicker({ label, models, model, onModel, efforts, effort, onEffort }: {
  label: string;
  models: readonly (readonly [string, string])[]; model: string; onModel: (v: string) => void;
  efforts: readonly (readonly [string, string])[]; effort?: string; onEffort: (v: string) => void;
}) {
  const [at, setAt] = useState<DOMRect | null>(null);
  // the efforts open beside Effort: to the right, else the left where the window ends there; as
  // high as fits (offset: from Effort's top)
  const [sub, setSub] = useState<{ left: boolean; top: number } | null>(null);
  const openSub = (e: React.SyntheticEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect(), tall = efforts.length * 30 + 14, wide = 160;
    setSub({ left: r.right + 12 + wide > innerWidth - 8, top: Math.min(-8, innerHeight - 12 - tall - r.top) });
  };
  const shown = efforts.find(([id]) => id === effort)?.[1];
  const close = () => { setAt(null); setSub(null); };
  const pick = (f: (v: string) => void, v: string) => { f(v); close(); };
  return <>
    <button type="button" className={`set-picker${at ? ' is-on' : ''}`} aria-label={`${label}: ${models.find(([id]) => id === model)?.[1] ?? model}${shown ? `, ${shown}` : ''}`} aria-haspopup="menu" aria-expanded={!!at}
      onClick={(e) => (at ? close() : setAt(e.currentTarget.getBoundingClientRect()))}>
      <span className="set-picker-text">
        {models.find(([id]) => id === model)?.[1] ?? model}
        {shown && <span className="set-picker-sub">{shown}</span>}
      </span>
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M2.5 4 5 6.5 7.5 4" /></svg>
    </button>
    {at && (
      <Menu at={at} onClose={close} width={200} height={30 * models.length + (efforts.length ? 73 : 47)}>
        <span className="menu-label">Model</span>
        {models.map(([id, name]) => (
          <button key={id} role="menuitemradio" aria-checked={id === model} onPointerEnter={() => setSub(null)} onClick={() => pick(onModel, id)}>
            {name}{id === model && <Check size={14} strokeWidth={1.8} className="menu-check" />}
          </button>
        ))}
        {efforts.length > 0 && <>
          <hr />
          <span className="menu-sub-anchor">
            <button aria-haspopup="menu" aria-expanded={!!sub} className={sub ? 'is-on' : ''} onPointerEnter={openSub} onClick={openSub}>
              Effort<span className="menu-sub-value">{shown}</span>
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M4 2.5 6.5 5 4 7.5" /></svg>
            </button>
            {/* in the menu's own box, so a click on it isn't a click outside the menu */}
            {sub && (
              <span className={`row-menu row-submenu${sub.left ? ' is-left' : ''}`} style={{ top: sub.top }} role="menu" aria-label="Effort">
                {efforts.map(([id, name]) => (
                  <button key={id} role="menuitemradio" aria-checked={id === effort} onClick={() => pick(onEffort, id)}>
                    {name}{id === effort && <Check size={14} strokeWidth={1.8} className="menu-check" />}
                  </button>
                ))}
              </span>
            )}
          </span>
        </>}
      </Menu>
    )}
  </>;
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (on: boolean) => void; label: string }) {
  return <button role="switch" aria-checked={on} aria-label={label} className="set-switch" onClick={() => onChange(!on)} />;
}

// A pop-up menu: the system's own (a native select)
// wide: 240px, as every control on Settings › Agents is
export function Select({ value, options, onChange, label, wide = false }: { value: string; options: readonly (readonly [string, string])[]; onChange: (v: string) => void; label: string; wide?: boolean }) {
  return (
    <span className={`set-select${wide ? ' is-wide' : ''}`}>
      <select value={value} onChange={(e) => onChange(e.target.value)} aria-label={label}>
        {options.map(([id, name]) => <option key={id} value={id}>{name}</option>)}
      </select>
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M2.5 4 5 6.5 7.5 4" /></svg>
    </span>
  );
}

export function Segmented({ value, options, onChange, label }: { value: string; options: [string, string][]; onChange: (v: string) => void; label: string }) {
  return (
    <span className="set-segmented" role="radiogroup" aria-label={label}>
      {options.map(([id, name]) => (
        <button key={id} type="button" role="radio" aria-checked={value === id} className={value === id ? 'is-on' : ''} onClick={() => onChange(id)}>{name}</button>
      ))}
    </span>
  );
}

export function TextField({ value, onChange, placeholder, label, secret = false, wide = false }: { value: string; onChange: (v: string) => void; placeholder: string; label: string; secret?: boolean; wide?: boolean }) {
  return <input className={`set-field${wide ? ' is-wide' : ''}`} type={secret ? 'password' : 'text'} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} aria-label={label} spellCheck={false} />;
}

// A read-only value (like everything on a Settings page, it can be selected and copied)
export function Value({ children }: { children: React.ReactNode }) {
  return <span className="set-value">{children}</span>;
}
