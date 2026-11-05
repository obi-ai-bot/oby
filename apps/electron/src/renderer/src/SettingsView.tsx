import { Fragment, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Check } from 'lucide-react';
import { BrandMark } from './brands';
import { useScrollThumb } from './useScrollThumb';
import { AGENTS, type Agent } from './mock';
import { MODES, SANDBOX } from './modes';
import { OwnerAvatar } from './ObyFace';
import { useSpheres } from './spheres';
import { everything, onObid } from './lean';
import qrcode from 'qrcode-generator';
import { ApiError, api, qrText, type Invitation, type PairedDevice, type ModelOptions, type Models, type SavedRule, type SignIn, type Status } from './obid';
import Connectors, { ConfirmWindow, Window, readConnectors } from './Connectors';
import { again, sameAs, startAdding, stopAdding, toolRows, whoseAgain, type Found } from './addAccount';
import { UNFINISHED, signInWhy } from './signInWhy';
import { BACKGROUND_LABEL, CONTINUE, LIMIT_REACHED, USING_PLAN, accountNow, addsWithChatGpt, offersChatGpt, waitingOver } from './chatgpt';
import { BackgroundConsent, ChatGptLogo, ManageUsage } from './ChatGptPlan';
import { Menu } from './Menu';
import { liveState, lookAtUsage, patchStatus, say, useLive } from './live';
import { AUTO, TALK_DEFAULT, talkChoices } from './talk';
import { version } from '../../../package.json';
import type { Update } from '../../shared/updates';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;

// Settings' pages, as Wellington's Mac app has them (apps/mac/app/Settings.swift
// on his dev branch): General (with Voice and Notifications, shared with the
// iPhone), Remote access, Accounts, Models, Vault and About; Connectors
// is ours, and Sharing a preview (docs/research/sphere-sharing.md). Each page opens in the view, as a task's page does, from the
// gear at the icons' foot (the account menu, with ?everything), then the side panel's Settings list.
// Sharing, Remote access and Vault are left out of v1 (Sangam, 2026-10-28) and come
// back later: only with ?everything (lean.ts)
// Agents takes Accounts' and Models' place: each agent's accounts with its model and efforts,
// and which one new tasks go to (the old two stay with ?everything)
// On obid, Remote access shows only pairing an iPhone (Pairing), as the Swift Mac app's Settings › Runner had it;
// the owner's order of 2026-11-01: no word a person sees says Runner, so that page went
const LATER = new Set([...(onObid ? [] : ['remote']), 'sharing', 'vault', 'accounts', 'models']);
export const SETTINGS_PAGES = [
  { id: 'general', label: 'General' },
  { id: 'agents', label: 'Agents' },
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
      {/* Speech is English only (the owner's decision of 2026-11-01): no language to pick */}
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

// Remote access: on obid, pairing an iPhone; otherwise Sangam's demo page
const Remote = () => (onObid ? <Pairing /> : <MockRemote />);

// Pair a device, as the Swift Mac app's Settings › Runner had it (apps/mac/app/PairingView.swift):
// while obid is in an Obysphere, an invitation the owner's click makes (main signs it, so the
// phone may approve), its QR code for the iPhone to scan, good for two minutes and once; the
// phone then reaches obid sealed, through the Obysphere (docs/runner-api.md, "obid only: the phone
// relay"). Paired devices, with Revoke.
function Pairing() {
  const sphere = useSpheres().find((s) => s.live);
  const [invitation, setInvitation] = useState<Invitation | null>(null);
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const failed = (e: Error) => setProblem(e.message);
  const load = () => api.pairedDevices().then(setDevices, () => { /* obid may be starting: the list comes next time */ });
  useEffect(() => { void load(); }, []);
  // a phone that paired shows here; ponytail: obid has no event for it, so every 5 s while a QR is out
  useEffect(() => {
    if (!invitation) return;
    const t = setInterval(load, 5000);
    // and gone once it's good for nothing
    const gone = setTimeout(() => setInvitation(null), Math.max(0, Date.parse(invitation.expiresAt) - Date.now()));
    return () => { clearInterval(t); clearTimeout(gone); };
  }, [invitation]);
  const create = () => {
    setBusy(true);
    api.pairInvite().then((i) => { setInvitation(i); setProblem(null); }, failed).finally(() => setBusy(false));
  };
  const revoke = (d: PairedDevice) => api.revokeDevice(d.id).then(() => { setDevices((all) => all.filter((x) => x.id !== d.id)); setProblem(null); }, failed);
  const qr = invitation && (() => { const q = qrcode(0, 'M'); q.addData(qrText(invitation), 'Byte'); q.make(); return q.createDataURL(6, 2); })();
  const expires = (d: PairedDevice) => new Date(d.expiresAt).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  return <>
    <Group i={1} title="Pair a device" footer={sphere ? undefined : 'Join an Obysphere and your iPhone pairs through it, with nothing else to set up.'}>
      {qr && sphere && <Row label={<img className="set-qr" src={qr} width={180} height={180} alt="Pairing QR code" />}
        about={`Through your Obysphere: ${new URL(invitation!.url).host}. Valid for two minutes. The code is single-use.`} />}
      {sphere
        ? <Row label="Through your Obysphere"><button className="set-button" disabled={busy} onClick={create}>{busy ? 'Creating…' : invitation ? 'Create new invitation' : 'Pair a device'}</button></Row>
        : <Row label="No way to reach this Mac yet" />}
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
    </Group>
    <Group i={2} title="Paired devices" footer="Revoke a lost phone here. It will need to pair again before it can reach this Mac.">
      {devices.length === 0 && <Row label={<span className="set-none">None yet</span>} />}
      {devices.map((d) => <Row key={d.id} label={d.name} about={`Expires ${expires(d)}`}><button className="set-button is-danger" onClick={() => void revoke(d)}>Revoke</button></Row>)}
    </Group>
  </>;
}

// Remote access (Sangam's demo): signing in to Oby's account, then reaching this Mac from anywhere
function MockRemote() {
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
// who: who it is, as its tool says (docs/contracts/accounts.md)
// used: this week's share in percent, null while its tool hasn't said
// signedIn: obid's (the mock's are all signed in); label: what obid says about the login, such as
// "Run codex once" while it's signed out
// session: the 5-hour window's use (null: none reported) and when it starts over; weekly: whether
// used is the week's
// chatgpt: signed in with Continue with ChatGPT (docs/contracts/chatgpt-sign-in.md)
export type Account = { chatgpt?: boolean; background?: boolean; signingIn?: boolean; id: string; agent: Agent; name: string; payer: string; who?: string; used: number | null; resets: string; plan?: Plan; signedIn?: boolean; label?: string | null; session?: number | null; sessionResets?: string | null; weekly?: boolean; full?: boolean };
// how full an account (or an agent) is: past 80% yellow, past 90% red
export const fullness = (used: number) => (used > 90 ? 'is-over' : used > 80 ? 'is-high' : '');
// what a card's bar shows: its 5-hour session, else its week
export const barOf = (a: Account) => a.session ?? a.used;
// an account's week in words, under its session's bar: its share when the tool reported one, else
// what obid says (not signed in, none reported yet); a session but no week: says so. Cursor's share
// is of its plan's period, a week or a month, so it names no week (USAGE-6)
export const weekWords = (a: Account) => (a.weekly && a.used != null ? `This week ${a.used}% · ${a.resets.replace(/^Resets/, 'resets')}`
  : a.agent === 'cursor' && a.used != null ? `${a.used}% used · ${a.resets.replace(/^Resets/, 'resets')}`
  : a.weekly === false && a.session != null ? 'No weekly use reported yet' : a.resets);
// a reset this week by its day; further off (a monthly plan's), by its date
const resetDay = (at: string, now: number) => new Date(at).toLocaleDateString([], Date.parse(at) - now < 7 * 86_400_000 ? { weekday: 'long' } : { month: 'short', day: 'numeric' });
// an account as obid's status says it (docs/contracts/usage.md, USAGE-1 and USAGE-6): its week's
// share (else its session's), its 5-hour session (signed in only; a session past its reset reads
// 0 with no week, and with one isn't shown) and when it starts over, and what to say when there's no figure ("No usage" for Cursor, whose
// figure comes from its plan and goes when a read fails)
export function accountRow(a: Status['accounts'][number], now: number): Account {
  // an Ultra plan has no 5-hour window, only the week, whatever an older reading left behind
  const ultra = a.agent === 'claude' && /ultra/i.test(a.plan ?? '');
  const used = a.sevenDay ?? (ultra ? null : a.fiveHour);
  const over = !!a.fiveHourResets && Date.parse(a.fiveHourResets) <= now;
  // a session that ended says nothing beside a week (an old reading, or a plan with no 5-hour window): the week is what's left
  const session = ultra || !a.signedIn || a.fiveHour == null ? null : over ? (a.sevenDay != null ? null : 0) : Math.round(a.fiveHour * 100);
  return {
    id: a.id, agent: a.agent, name: a.name, payer: a.payer ?? 'You', who: [a.email, a.org, a.plan && a.plan[0].toUpperCase() + a.plan.slice(1)].filter(Boolean).join(' · ') || undefined, signedIn: a.signedIn, label: a.label ?? null, used: used == null ? null : Math.round(used * 100),
    chatgpt: a.login === 'chatgpt',
    background: a.backgroundConsent === true,
    signingIn: a.signingIn,
    resets: !a.signedIn ? a.label ?? 'Not signed in' : a.login === 'chatgpt' && a.fullUntil ? LIMIT_REACHED : used == null ? (a.agent === 'cursor' ? 'No usage' : 'No use reported yet')
      : a.sevenDayResets ? `Resets ${resetDay(a.sevenDayResets, now)}` : 'No reset reported',
    session,
    sessionResets: session != null && !over && a.fiveHourResets ? new Date(a.fiveHourResets).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : null,
    weekly: a.sevenDay != null && a.agent !== 'cursor',
    full: !!a.fullUntil && Date.parse(a.fullUntil) > now,
  };
}
const ACCOUNTS: Account[] = [
  { id: 'a1', agent: 'claude', name: 'Deskree', payer: 'Deskree', who: 'doti@deskree.com · Deskree · Team', used: 36, resets: 'Resets Thursday', plan: 'seat', session: 12, sessionResets: '4:10 PM', weekly: true },
  { id: 'a2', agent: 'claude', name: 'Personal', payer: 'Doti', who: 'doti@example.com · Max', used: 82, resets: 'Resets in 2 days', plan: 'personal', session: 86, sessionResets: '2:14 PM', weekly: true },
  { id: 'a3', agent: 'codex', name: 'Deskree', payer: 'Deskree', used: 12, resets: 'Resets Monday', plan: 'seat', session: 5, sessionResets: '6:40 PM', weekly: true },
  { id: 'a4', agent: 'claude', name: 'Deskree API', payer: 'Deskree', used: 8, resets: '$32 of $400 this month', plan: 'api', session: null, weekly: false },
  { id: 'a5', agent: 'cursor', name: 'Personal', payer: 'Doti', used: 24, resets: 'Resets Nov 20', plan: 'personal', session: null, weekly: false },
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
type Signing = 'opening' | (SignIn & { code?: 'sending' | 'sent'; chatgpt?: boolean });
function useSignIns(accounts: Account[], setAccounts: (a: Account[]) => void) {
  // the tools' own logins on this Mac that aren't accounts (found, or removed here): only
  // GET /v1/status has them, read as the page opens and after a change (obid also reads the usage
  // shown here then, which comes in its `accounts` event)
  const [offered, setOffered] = useState<Found[]>(onObid ? [] : [{ agent: 'codex', id: 'codex', name: 'Codex' }]);
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
  // Continue with ChatGPT: the Mac's own login's account can't take it, so a new one does
  const signInChatGpt = (id: string) => tried(id, async () => {
    put(id, 'opening');
    let at = id;
    try {
      const page = await api.signInChatGpt(id).catch(async (e) => {
        if (!(e instanceof ApiError && e.reason === 'ownLogin')) throw e;
        at = (await api.addAccount({ agent: 'codex' })).id;
        put(id);
        put(at, 'opening');
        return api.signInChatGpt(at);
      });
      put(at, { ...page, chatgpt: true });
      window.app?.openExternal(page.url);
    } catch (e) { put(at); throw e; }
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
  // (and a ChatGPT sign-in that ended without, as obid says now: its row says why)
  useEffect(() => {
    for (const a of accounts) {
      const s = signing[a.id];
      if (!s) continue;
      if (a.signedIn) { put(a.id); continue; }
      if (s === 'opening' || !waitingOver(a, s)) continue;
      void accountNow(a.id).then((now) => { if (!now || waitingOver(now, s)) put(a.id); }, () => { /* the next event says */ });
    }
  }, [accounts]);
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
    onConfirm={() => void tried(gone.id, () => api.removeAccount(gone.id).then((said) => {
      put(gone.id); look();
      if (said?.revoked === false) setProblems((p) => ({ ...p, [ADD]: `ChatGPT didn’t confirm ending ${gone.name}’s session. You can disconnect it in ChatGPT settings.` }));
    }))} onClose={() => setRemoving(null)} />;
  return { offered, setOffered, look, signing, put, problems, busy, tried, signIn, signInChatGpt, sendCode, cancel, remove, removeWindow };
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
  if (a.chatgpt) return 'Oby signs it out of ChatGPT and deletes what it kept for it. Its registration stays in your ChatGPT settings until you disconnect it there.';
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
export function useModels(): [(x: Account) => Tuning, ModelOptions, (id: string, change: Partial<Tuning>) => Promise<void>, Models] {
  const status = useLive().status;
  const live = status?.models as Partial<Models> | undefined;
  const [mine, setMine] = useKeptSetting<Record<string, Partial<Tuning>>>('models.mine', {});
  const [options, setOptions] = useState(MOCK_OPTIONS);
  const [claudeModel] = useSetting('claude.model', MOCK_MODELS.claude!.model);
  const [codexModel] = useSetting('codex.model', MOCK_MODELS.codex!.model);
  const [cursorModel] = useSetting('cursor.model', MOCK_MODELS.cursor!.model);
  const mockModels = { claude: claudeModel, codex: codexModel, cursor: cursorModel };
  const kept = keepsAgents(status);
  useEffect(() => { if (kept) api.modelOptions().then(setOptions, () => {}); }, [kept]);
  // each tool's on its own: what obid says it runs over the mock's, so a key obid leaves out isn't lost
  const base = Object.fromEntries((Object.keys(MOCK_MODELS) as Agent[]).map((a) => [a, { ...MOCK_MODELS[a], ...(onObid ? live?.[a] : { model: mockModels[a] }) }])) as Models;
  const of = (x: Account): Tuning => ({ ...(base[x.agent] ?? { model: 'auto' }),
    ...(kept ? tuningOf(status.accounts.find((a) => a.id === x.id)) : mine[x.id]) });
  const tune = async (id: string, change: Partial<Tuning>) => {
    if (!kept) return setMine({ ...mine, [id]: { ...mine[id], ...change } });
    await tuneAccount(id, change);
  };
  return [of, options, tune, base];
}
// an account's choices kept by obid (PATCH /v1/accounts/{id}), over the accounts as they are once
// it answers: an `accounts` event during the call stays, only the choices are the answer's (#1709)
export async function tuneAccount(id: string, change: Partial<Tuning>) {
  const { model, effort, complexEffort } = await api.tuneAccount(id, change);
  patchStatus({ accounts: (liveState().status?.accounts ?? []).map((x) => (x.id === id ? { ...x, model, effort, complexEffort } : x)) });
}
// the account new tasks go to: obid's, else kept here. A change obid refuses is reported as any
// failure of the app's is (live.ts, say), and the menu stays on obid's
function useDefaultAccount(): [string | null, (id: string) => Promise<void>] {
  const status = useLive().status;
  const [chosen, setChosen] = useKeptSetting<string | null>('agent.default', null);
  if (!keepsAgents(status)) return [chosen, async (id) => setChosen(id)];
  return [status.defaultAccount ?? null, async (id) => patchStatus(await api.setDefaultAccount(id))];
}
// who a new conversation talks through: obid's, else kept here (the mock). Asks again after: what a
// first turn would use follows it (talkNext)
export function useTalkDefault(): [string, (id: string) => Promise<void>] {
  const status = useLive().status;
  const [kept, setKept] = useKeptSetting<string>(TALK_DEFAULT, AUTO);
  if (!keepsAgents(status)) return [kept, async (id) => setKept(id)];
  return [status.talkAccount ?? AUTO, async (id) => { patchStatus(await api.setTalkAccount(id === AUTO ? null : id)); lookAtUsage(); }];
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

export function Agents() {
  const [accounts, setAccounts] = useAccounts();
  const [tuning, options, tune] = useModels();
  const [chosen, setChosen] = useDefaultAccount();
  const [talking, setTalking] = useTalkDefault();
  // signing in, as Settings › Accounts does (useSignIns): its states, its problems, the tool's own login
  const { offered, setOffered, look, signing, problems, signIn, signInChatGpt, sendCode, cancel, remove, removeWindow } = useSignIns(accounts, setAccounts);
  const [adding, setAdding] = useState(false);
  const { status } = useLive();
  // the question again, when its switch is turned on: only its own button's click says yes
  const [asking, setAsking] = useState<Account | null>(null);
  const background = (x: Account, given: boolean) => {
    if (onObid) void api.setBackgroundConsent(x.id, given).catch(say);
    else setAccounts(accounts.map((y) => (y.id === x.id ? { ...y, background: given } : y)));
  };
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
      {fallback && (
        <Row label="New conversations talk through">
          <Select wide value={talking} onChange={(v) => void setTalking(v).catch(say)} label="New conversations talk through" options={talkChoices(agents)} />
        </Row>
      )}
    </Group>
    <Group i={2} footer={problems[ADD] ? <span className="set-problem" role="alert">{problems[ADD]}</span> : offered.length ? `Found on this Mac: ${offered.map((l) => AGENTS[l.agent].name).join(', ')}` : undefined}>
      {/* the card's action, not a row's: on the left */}
      <div className="set-row is-action">
        <button className="set-button is-primary" onClick={() => setAdding(true)}>Add account</button>
      </div>
    </Group>
    {agents.map((x, k) => {
      const a = x.agent, t = tuning(x), o = options[a];
      const efforts = a === 'codex' ? o?.models.find((m) => m.id === t.model)?.efforts : o?.efforts;
      const out = x.signedIn === false, sign = signing[x.id], tool = AGENTS[a].name, own = ownLogin(x);
      // Codex with no login on this Mac, or a ChatGPT one that ended
      const plan = a === 'codex' && out && (x.chatgpt || (offersChatGpt(status) && !x.label));
      // the bar: the 5-hour session; the week, in words under it
      const week = weekWords(x), bar = barOf(x);
      return (
        <Group key={x.id} i={3 + k} title={x.name}>
          <div className="set-row set-usage">
            <BrandMark brand={AGENTS[a].brand} size={16} mono />
            <span className="set-label">
              <span className="set-usage-name">
                {tool}
                {!out && !sign && bar != null && <span className="set-value">{bar}% used</span>}
              </span>
              {x.who && <span className="set-about">{x.who}</span>}
              {x.chatgpt && !out && <span className="set-about">{x.full ? LIMIT_REACHED : USING_PLAN} · <ManageUsage /></span>}
              {sign
                ? <span className="set-about">{sign === 'opening' ? `Opening ${tool}’s sign-in page` : sign.code ? `Signing in to ${tool}` : `Finish on ${tool}’s sign-in page${sign.userCode ? ` · enter ${sign.userCode}` : ''}`}</span>
                : out ? <span className="set-about">{x.label ?? 'Not signed in'}{own && !x.label ? `: sign in with ${tool} itself on this Mac` : ''}</span>
                : bar != null ? <>
                    <span className="set-about">{x.session != null ? `5-hour session${x.sessionResets ? ` · resets ${x.sessionResets}` : ''}` : week}</span>
                    <span className="set-meter" role="meter" aria-valuenow={bar} aria-valuemin={0} aria-valuemax={100} aria-label={x.session != null ? `${bar}% of the 5-hour session used` : x.weekly === false ? `${bar}% used` : `${bar}% of the week used`}>
                      <span className={fullness(bar)} style={{ width: `${bar}%` }} />
                    </span>
                    {x.session != null && <span className="set-about is-quiet">{week}</span>}
                  </>
                : <span className="set-about">{week}</span>}
              {sign && sign !== 'opening' && sign.pasteCode && !sign.code && <PasteCode onCode={(code) => sendCode(x.id, sign, code)} />}
              {problems[x.id] && <span className="set-about set-problem" role="alert">{problems[x.id]}</span>}
            </span>
            {/* (obid signs in only the accounts with a folder of their own: not the tool's own login) */}
            {out && (!own || plan) && <span className="set-control set-control-top">
              {sign && sign !== 'opening' && sign.code !== 'sending'
                ? <button className="set-button" onClick={() => cancel(x.id)}>Cancel</button>
                : plan
                  ? <>
                      {!own && !sign && <button type="button" className="chatgpt-link" onClick={() => signIn(x.id)}>Use a code instead</button>}
                      <button className="set-button" disabled={!!sign} onClick={() => signInChatGpt(x.id)}>{sign ? 'Waiting…' : <><ChatGptLogo />{CONTINUE}</>}</button>
                    </>
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
          {x.chatgpt && !out && <Row label={BACKGROUND_LABEL}>
            <Switch on={!!x.background} label={`${x.name}: ${BACKGROUND_LABEL}`} onChange={(on) => (on ? setAsking(x) : background(x, false))} />
          </Row>}
          {/* the card's own action, on the left; red: it can't be undone (its sign-in goes with it) */}
          <div className="set-row is-action">
            <button className="set-button is-danger" onClick={() => remove(x)}>Remove</button>
          </div>
        </Group>
      );
    })}
    {asking && <BackgroundConsent name={asking.name} onClose={() => setAsking(null)} onAnswer={(given) => { background(asking, given); setAsking(null); }} />}
    {adding && <AddAccount accounts={accounts} setAccounts={setAccounts} found={offered}
      onFound={(l) => (onObid ? look() : setOffered(offered.filter((x) => x !== l)))} onClose={() => { setAdding(false); look(); }} />}
    {removeWindow}
  </>;
}

// Add account (docs/contracts/accounts.md): pick the tool, its own sign-in starts at once, and a
// name is optional after it. A login found on this Mac is one click. Words proposed, Sangam's to
// rewrite. (ponytail: off obid, the tool's page is a moment's wait, as first run's)
type Adding = { at: 'tools'; why?: string }
  | { at: 'signing'; chatgpt?: boolean; agent: Agent; account: string; page?: SignIn; sending?: boolean }
  | { at: 'named'; agent: Agent; account: string }
  | { at: 'same'; agent: Agent; words: string };
export function AddAccount({ accounts, setAccounts, found, onFound, onClose }: {
  accounts: Account[]; setAccounts: (a: Account[]) => void; found: Found[]; onFound: (l: Found) => void; onClose: () => void;
}) {
  const [step, setStep] = useState<Adding>({ at: 'tools' });
  const [draft, setDraft] = useState<string | null>(null);
  const [problem, setProblem] = useState<string>();
  const signing = step.at === 'signing' ? step : null;
  const account = step.at === 'signing' || step.at === 'named' ? step.account : null;
  const live = accounts.find((a) => a.id === account);
  // the try under way: one cancelled while its page was still opening is undone once it's there
  const tries = useRef(0);
  const pick = async (agent: Agent, chatgpt = false) => {
    const now = ++tries.current;
    if (!onObid) {
      setStep({ at: 'signing', agent, account: '' });
      setTimeout(() => {
        if (now !== tries.current) return;
        const made = `a${Date.now()}`;
        setAccounts([...accounts, { id: made, agent, name: 'Max', who: 'doti@example.com · Max', payer: 'You', used: 0, resets: 'Resets in 7 days', signedIn: true, session: 0, weekly: true }]);
        setStep({ at: 'named', agent, account: made });
      }, 1600);
      return;
    }
    setStep({ at: 'signing', agent, account: '' });
    try {
      const { account: made, page } = await startAdding(agent, chatgpt);
      if (now !== tries.current) return void stopAdding(made);
      setStep({ at: 'signing', agent, account: made, page, chatgpt });
    } catch (e) { if (now === tries.current) setStep({ at: 'tools', why: signInWhy(AGENTS[agent].name, e) }); }
  };
  const use = async (l: Found) => {
    try {
      if (onObid) await api.addAccount({ agent: l.agent, default: true });
      else setAccounts([...accounts, { id: l.id, agent: l.agent, name: l.name, payer: 'You', used: 12, resets: 'Resets Monday', signedIn: true, session: 5, weekly: true }]);
      onFound(l);
      onClose();
    } catch (e) { setStep({ at: 'tools', why: signInWhy(AGENTS[l.agent].name, e) }); }
  };
  const stop = async (why?: string) => {
    tries.current++;
    if (signing?.account) await stopAdding(signing.account);
    setStep({ at: 'tools', why });
  };
  const send = async (code: string) => {
    if (!signing?.page) return;
    setStep({ ...signing, sending: true });
    try {
      await api.signInCode(signing.account, code.trim());
      setStep({ at: 'named', agent: signing.agent, account: signing.account });
    } catch (e) {
      const same = sameAs(e);
      if (same) setStep({ at: 'same', agent: signing.agent, words: again(accounts, same) });
      else if (e instanceof ApiError && e.status === 422) await stop(signInWhy(AGENTS[signing.agent].name, e));
      // no answer in time: it may still sign in, until its page runs out or Cancel
      else setStep({ ...signing, sending: true });
    }
  };
  const done = async () => {
    const name = draft?.trim();
    if (!name || !live || name === live.name) return onClose();
    try {
      if (onObid) await api.renameAccount(live.id, name);
      else setAccounts(accounts.map((a) => (a.id === live.id ? { ...a, name } : a)));
      onClose();
    } catch (e) { setProblem(e instanceof Error ? e.message : String(e)); }
  };
  // obid's `accounts` event says it's in (Codex and Cursor end by themselves), or that it's gone:
  // then it was another account's login again. (Not in the list yet is no news: it was only just made.)
  const [seen, setSeen] = useState<string | null>(null);
  useEffect(() => {
    if (!account) return;
    if (live) {
      if (seen !== account) setSeen(account);
      if (signing && live.signedIn) setStep({ at: 'named', agent: signing.agent, account });
      // ChatGPT's page said no: why, on the tools again. Signed in with plan use off, it stays.
      else if (signing?.chatgpt && signing.page && waitingOver(live, signing)) {
        void accountNow(account).then((now) => {
          if (now && !waitingOver(now, signing)) return;
          const why = now?.label ?? UNFINISHED;
          if (now?.login === 'chatgpt') setStep({ at: 'tools', why });
          else void stop(why);
        }, () => { /* the next event says */ });
      }
    } else if (seen === account && onObid) {
      const agent = (step as { agent: Agent }).agent;
      void whoseAgain(account).then((same) => setStep(same ? { at: 'same', agent, words: again(accounts, same) } : { at: 'tools' }));
    }
  }, [accounts, account]);
  // a page that ran out (15 minutes) without signing in
  const expires = signing?.page ? Date.parse(signing.page.expiresAt ?? '') : NaN;
  useEffect(() => {
    if (Number.isNaN(expires)) return;
    const timer = setTimeout(() => void stop(UNFINISHED), expires - Date.now());
    return () => clearTimeout(timer);
  }, [expires]);
  const brand = step.at === 'tools' ? undefined : AGENTS[step.agent].brand;
  if (step.at === 'same') return <Window brand={brand} title="Add account" onClose={onClose}>
    <p className="modal-lede" role="status">{step.words}</p>
    <div className="modal-actions"><button type="button" className="modal-primary" onClick={onClose}>Done</button></div>
  </Window>;
  if (step.at === 'named') return <Window brand={brand} title="Add account" onClose={onClose}>
    <p className="modal-lede" role="status">{live?.who ? `Signed in as ${live.who}` : `Signed in to ${AGENTS[step.agent].name}`}</p>
    <form onSubmit={(e) => { e.preventDefault(); void done(); }}>
      <input className="modal-field" value={draft ?? live?.name ?? ''} onChange={(e) => setDraft(e.target.value)} placeholder="Name (optional)" aria-label="Name (optional)" spellCheck={false} />
      {problem && <p className="set-about set-problem" role="alert">{problem}</p>}
      <div className="modal-actions"><button className="modal-primary">Done</button></div>
    </form>
  </Window>;
  if (step.at === 'signing') {
    const tool = AGENTS[step.agent].name, page = step.page;
    return <Window brand={brand} title={`Sign in to ${tool}`} onClose={() => void stop().then(onClose)}>
      <p className="modal-lede" role="status">
        {!page ? (onObid ? `Opening ${tool}’s sign-in page` : `Finish on ${tool}’s sign-in page`) : step.sending ? `Signing in to ${tool}` : `Finish on ${tool}’s sign-in page${page.userCode ? ` · enter ${page.userCode}` : ''}`}
      </p>
      {page?.pasteCode && !step.sending && <PasteCode onCode={(code) => void send(code)} />}
      <div className="modal-actions">
        {page && <button type="button" className="set-button" onClick={() => window.app?.openExternal(page.url)}>Open the page again</button>}
        <button type="button" className="set-button" onClick={() => void stop()}>Cancel</button>
      </div>
    </Window>;
  }
  return <Window title="Add account" onClose={onClose}>
    <div className="set-card">
      {toolRows(accounts, found).map(({ agent, added, found: here }) => (
        <Fragment key={agent}>
          <div className="set-row">
            <BrandMark brand={AGENTS[agent].brand} size={16} mono />
            <span className="set-label">{AGENTS[agent].name}<span className="set-about">{added ? `${added} added` : 'None yet'}</span></span>
            <span className="set-control">
              {addsWithChatGpt(agent, here) && <button type="button" className="set-button" onClick={() => void pick(agent, true)}><ChatGptLogo />{CONTINUE}</button>}
              <button type="button" className={addsWithChatGpt(agent, here) ? 'chatgpt-link' : 'set-button'} onClick={() => void pick(agent)}>{addsWithChatGpt(agent, here) ? 'Use a code instead' : 'Sign in'}</button>
            </span>
          </div>
          {here.map((l) => (
            <div key={l.id} className="set-row">
              <BrandMark brand={AGENTS[agent].brand} size={16} mono />
              <span className="set-label">{AGENTS[agent].name}’s login<span className="set-about">Found on this Mac</span></span>
              <span className="set-control"><button type="button" className="set-button is-primary" onClick={() => void use(l)}>Use it</button></span>
            </div>
          ))}
        </Fragment>
      ))}
    </div>
    {step.why && <p className="set-about set-problem" role="alert">{step.why}</p>}
  </Window>;
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

const PAGES: Record<string, () => React.ReactNode> = { general: General, agents: Agents, sharing: Sharing, remote: Remote, accounts: Accounts, models: Models, connectors: Connectors, vault: Vault, about: About };

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
