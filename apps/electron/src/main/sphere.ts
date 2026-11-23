import { app, BrowserWindow, dialog, shell } from 'electron';
import { createServer } from 'node:http';
import { generateKeyPairSync, type KeyObject } from 'node:crypto';
import { keptKey, replaceKey } from './device';
import { obid, type Trusted } from './obid';
import { ipc } from './ipc';
import { takeInput } from './words';
import { boolean, safeInteger, string, type Decoder } from '../shared/decode';
import { NotThisKey, addStarterRule, join, joinWarning, keepMembership, leave, addSignin, getBackIn, listInvitations, makeInvitation, parseInvite, readPolicy, rebindOffer, setAdmin, signinStatus, withdrawInvitation, type AdminsAnswer, type InvitesAnswer, type PolicyAnswer } from '../shared/sphere';
import { withStarterRule } from '../shared/policy';
import { SCRATCH } from '../shared/build';
import { HANDOFF_OPEN, handoffBody, type HandoffAsk } from '../shared/handoff';
import { EMAIL, inviteMail, sphereAddress, type Joined } from '../shared/invite';

// Joining an Obysphere by a private link (docs/contracts/obysphere.md). The owner key, Ed25519, is
// made here in main and kept as the device key is (device.ts): it never leaves main, and the page
// can't hand it anything to sign. A link joins only on the owner's click in this app's own dialog;
// the Obysphere's join page's Open in Oby (oby://join/<host>#…) is how one arrives.
// ponytail: one owner key per Mac, as the Swift app's; the same person on a second Mac is
// another key, which a Sphere refuses for the same owner (SPHERE-2's words say so).

let key: KeyObject | null = null;
const ownerKey = () => (key ??= keptKey('owner-key', () => generateKeyPairSync('ed25519').privateKey));

// oby:// opens this app (electron-builder.yml's protocols); the link may come before the window
let pending: string | null = null;
let opened: ((url: string) => void) | null = null;
app.on('open-url', (e, url) => {
  e.preventDefault();
  if (opened) opened(url); else pending = url;
});

// On the window once it shows, now or when the owner next opens Oby, never brought forward for it:
// Oby opened hidden for an alert (ALERTS-3) stays hidden
function onWindow(f: (w: BrowserWindow) => void) {
  const w = BrowserWindow.getAllWindows()[0];
  if (w) return w.isVisible() || w.isMinimized() ? f(w) : void w.once('show', () => f(w));
  app.once('browser-window-created', (_e, w) => w.once('show', () => f(w)));
}

const NOT_A_LINK = 'It isn’t an Obysphere invite link.';

// The invite links main made this run (SPHEREAPP-10's Make a link, Invite member's click): Send
// invitation mails only one of these
const madeLinks = new Set<string>();
// Send invitation's addresses: 1 to 50, each an email address
const addresses: Decoder<string[]> = (v) =>
  Array.isArray(v) && v.length >= 1 && v.length <= 50 && v.every((e) => typeof e === 'string' && EMAIL.test(e)) ? { value: v as string[] } : undefined;

// obid's owner id now: who a join would act for, or null when obid can't say
async function ownerNow() {
  const a = await obid('GET', '/v1/obid/sphere', undefined, undefined, true).catch(() => null);
  try { return a?.status === 200 ? String((JSON.parse(a.text) as { owner?: unknown }).owner ?? '') || null : null; } catch { return null; }
}

// the handoff form's four fields, each a string; anything more is dropped
export const handoffAsk: Decoder<HandoffAsk> = (v) => {
  const o = v as Record<string, unknown> | null;
  if (!o || typeof o !== 'object' || Array.isArray(o)) return undefined;
  const [to, goal, repo, base] = [o.to, o.goal, o.repo, o.base];
  return [to, goal, repo, base].every((x) => typeof x === 'string') ? { value: { to, goal, repo, base } as HandoffAsk } : undefined;
};

const said502 = (text: string) => { try { return String((JSON.parse(text) as { error?: unknown }).error ?? text); } catch { return text; } };
export async function handOff(ask: HandoffAsk, call: typeof obid = obid): Promise<string | null> {
  const body = handoffBody(ask);
  if (!body) return 'That task can’t go as it is.';
  const r = await call('GET', '/v1/obid/sphere/members').catch(() => null);
  if (r?.status !== 200) return 'Oby can’t reach your Obysphere right now. Try again in a moment.';
  const members = JSON.parse(r.text) as { obi: string; owner: string; accepts?: string[]; sealed?: boolean }[];
  // their Obys that take tasks, one whose words go sealed first
  const takes = members.filter((m) => m.owner === ask.to && m.accepts?.includes('handoff')).sort((a, b) => Number(!!b.sealed) - Number(!!a.sealed));
  if (!takes.length) return members.some((m) => m.owner === ask.to) ? `${ask.to}’s Oby doesn’t take tasks yet.` : `${ask.to} isn’t in your Obysphere.`;
  const sent = await call('POST', '/v1/obid/sphere/requests', { to: takes[0].obi, type: 'handoff', body, expiresIn: HANDOFF_OPEN }).catch(() => null);
  if (!sent) return 'Oby can’t reach obid right now.';
  if (sent.status >= 200 && sent.status < 300) return null;
  const why = said502(sent.text);
  if (why.includes('policy_denied')) return 'This Obysphere’s rules don’t allow handing over tasks yet. An admin can change that.';
  return why;
}

// The browser's way back from a sign-in (SPHEREAPP-12): a listener on 127.0.0.1 only, on a port the
// system picks, for one GET of its path, at most 10 minutes; its query is all it reads. RFC 8252 7.3.
const SIGNIN_PATH = '/oby-signin';
export function signinListener(): Promise<{ returnTo: string; back: Promise<{ state?: string; code?: string; error?: string }>; close: () => void }> {
  return new Promise((ready, refused) => {
    let done: (v: { state?: string; code?: string; error?: string }) => void = () => {};
    const back = new Promise<{ state?: string; code?: string; error?: string }>((r) => { done = r; });
    const server = createServer((req, res) => {
      const u = new URL(req.url ?? '/', 'http://127.0.0.1');
      if (req.method !== 'GET' || u.pathname !== SIGNIN_PATH) { res.writeHead(404).end(); return; }
      res.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
      res.end('<!doctype html><meta charset="utf-8"><title>Oby</title><p style="font:16px -apple-system,sans-serif;margin:40px">You can close this tab and go back to Oby.</p>');
      const q = (k: string) => u.searchParams.get(k) ?? undefined;
      done({ state: q('state'), code: q('code'), error: q('error') });
    });
    const timer = setTimeout(() => done({ error: 'timeout' }), 10 * 60_000);
    const close = () => { clearTimeout(timer); server.close(); };
    server.on('error', refused);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address() as { port: number };
      ready({ returnTo: `http://127.0.0.1:${port}${SIGNIN_PATH}`, back, close });
    });
  });
}

export function sphering(trusted: Trusted, show: () => BrowserWindow) {
  // a scratch install (shared/build.ts) never takes oby:// from the installed Oby
  if (app.isPackaged && !SCRATCH) app.setAsDefaultProtocolClient('oby');
  const said = (w: BrowserWindow, message: string, detail?: string) => dialog.showMessageBox(w, { type: 'warning', message, detail, buttons: ['OK'] });

  // the card obid asks for (its seal key) and the delegation's renewal (SPHERE-6): at launch, after
  // a join, and every 5 minutes, as the Swift app did; obid has no membership event to wait on
  // A membership another key signed is told to the owner, once a launch (SPHERE-6). Never by a build
  // for testing (npm run drive, npm run dev), whose key is its own on the owner's real obid: it logs.
  let told = false;
  const keep = () => keepMembership(obid, ownerKey).catch((e) => {
    if (e instanceof NotThisKey && app.isPackaged) { if (!told) { told = true; onWindow((w) => void said(w, 'Oby can’t keep your Obysphere working.', e.message)); } }
    else console.warn('Obysphere membership:', e instanceof Error ? e.message : e);
  });
  void keep();
  setInterval(() => void keep(), 5 * 60_000);

  // The one join path (SPHEREAPP-3 and 4): a link opened as oby://, or pasted in the app (first
  // run's Obysphere step, Settings › Obysphere). The owner's Join in this native dialog is what
  // joins: a page can't press it. Before any key is made, it says the owner id the Oby will act
  // for (a stranger without gh gets the Mac's login name, which two strangers can share).
  const joinFrom = async (w: BrowserWindow, text: string): Promise<Joined> => {
    const invite = parseInvite(text);
    if (!invite) return { kind: 'not_a_link' };
    const owner = await ownerNow();
    const { response } = await dialog.showMessageBox(w, {
      type: 'question', message: `Join ${invite.name}?`,
      detail: `${invite.by ? `${invite.by} invited you. ` : ''}${joinWarning(invite)}${owner ? `\n\nYou’ll join as ${owner}.` : ''}`,
      buttons: ['Join', 'Cancel'], defaultId: 1, cancelId: 1,
    });
    if (response !== 0) return { kind: 'cancelled' };
    try {
      await join(obid, ownerKey, invite);
      void keep();
      if (!w.isDestroyed()) w.webContents.send('sphere', { sphere: invite.sphere, name: invite.name });
      return { kind: 'joined', name: invite.name };
    } catch (e) {
      return { kind: 'failed', name: invite.name, why: e instanceof Error ? e.message : String(e) };
    }
  };
  opened = async (url) => {
    const w = show();
    const r = await joinFrom(w, url);
    if (r.kind === 'not_a_link') void said(w, 'Oby can’t open this link.', NOT_A_LINK);
    if (r.kind === 'failed') void said(w, `Oby couldn’t join ${r.name}.`, r.why);
  };
  // From the page: the link it holds, only right after a click or key main saw itself (words.ts,
  // takeInput), as the policy's write; the dialog above still asks. Answers what happened, in
  // words the page shows; it opens no other dialog.
  ipc(trusted).handle('sphere-join', [string], async (e, link): Promise<Joined> => {
    if (!takeInput()) return { kind: 'failed', name: '', why: 'Joining takes your own click.' };
    return joinFrom(BrowserWindow.fromWebContents(e.sender) ?? show(), link);
  }, { kind: 'failed', name: '', why: 'Not this app’s page.' } as Joined);

  // A task handed to a member's Oby (SPHEREAPP-6, docs/contracts/handoff.md), through obid's own
  // send (POST /v1/obid/sphere/requests): only right after a click or key main saw itself, one
  // each. Main builds the body from the form's four fields (shared/handoff.ts) and picks the
  // member's Oby that takes tasks. Answers null once sent, or why not in words.
  ipc(trusted).handle('sphere-handoff', [handoffAsk], async (_e, ask) => {
    if (!takeInput()) return 'Handing a task over takes your own click.';
    return handOff(ask);
  }, 'Not this app’s page.');
  if (pending) { const url = pending; pending = null; opened(url); }

  // Leave, from the Obysphere's settings (SPHERE-4): when the Obysphere couldn't be told, or obid
  // couldn't leave, the owner is told why; answers that, or null
  ipc(trusted).handle('sphere-leave', [], async (e) => {
    const w = BrowserWindow.fromWebContents(e.sender) ?? show();
    try {
      const { membership, untold, sphere } = await leave(obid, ownerKey);
      if (untold) void said(w, `You left ${sphere}, but it couldn’t be told.`, `${untold}. It still lists your Oby until its admin removes it.`);
      return untold ?? (membership.state === 'none' ? null : 'obid is still in it.');
    } catch (err) {
      const why = err instanceof Error ? err.message : String(err);
      void said(w, 'Oby couldn’t leave the Obysphere.', why);
      return why;
    }
  }, 'Not this app’s page.');

  // The Obysphere's rules (docs/contracts/sphere-policy.md): read, and the members rule added at the
  // version the page read. The page sends only that version; main reads the rules again, signs and
  // writes. It answers a PolicyAnswer, never a token, and opens no dialog: the page says why.
  const failed = (why: string): PolicyAnswer => ({ sphere: null, policy: null, admin: false, refused: 'failed', why });
  const policy = (f: () => Promise<PolicyAnswer>) => f().catch((e) => failed(e instanceof Error ? e.message : String(e)));
  ipc(trusted).handle('sphere-policy', [], () => policy(() => readPolicy(obid, ownerKey)), null);
  // A write, with the owner key's admin token, only right after a click or key main saw itself
  // (words.ts, takeInput), one write each, as Publish: a script that took the page over can't add
  // the rule by itself (POLICY-4)
  ipc(trusted).handle('sphere-set-policy', [safeInteger], async (_e, version) => (
    takeInput() ? policy(() => addStarterRule(obid, ownerKey, version)) : failed('Adding the rule takes your own click.')), null);

  // Invite links, the admin's (SPHEREAPP-10): each with its own owner token, minted here and never
  // answered. Making and withdrawing one only right after a click or key main saw itself, one each;
  // reading the list takes none. Making one also adds what the members rule lacks (handoff), so a
  // new member can hand off tasks, and says whether it now does.
  const notInvited = (why: string): InvitesAnswer => ({ invitations: [], link: null, refused: 'failed', why });
  const invited = (f: () => Promise<InvitesAnswer>) => f().catch((e) => notInvited(e instanceof Error ? e.message : String(e)));
  ipc(trusted).handle('sphere-invitations', [], () => invited(() => listInvitations(obid, ownerKey)), null);
  ipc(trusted).handle('sphere-invite', [], async () => {
    if (!takeInput()) return notInvited('Making a link takes your own click.');
    return invited(async () => {
      const made = await makeInvitation(obid, ownerKey);
      if (!made.link) return made;
      madeLinks.add(made.link);
      const rules = await readPolicy(obid, ownerKey);
      const now = rules.policy && withStarterRule(rules.policy.rules) ? await addStarterRule(obid, ownerKey, rules.policy.version) : rules;
      return { ...made, handoffs: !!now.policy && !withStarterRule(now.policy.rules) };
    });
  }, null);
  // Send invitation, Invite member's (SPHEREAPP-22): the Obysphere sends no mail, so a new email in
  // the person's own mail app, to the addresses typed, carrying a link main made this run (never one
  // the page wrote), only right after a click or key main saw itself, one each, and only as mailto:.
  // Nothing leaves until the person sends it there. Answers null once the draft opened, or why not.
  ipc(trusted).handle('sphere-invite-mail', [addresses, string], async (_e, emails, link): Promise<string | null> => {
    if (!takeInput()) return 'Sending an invitation takes your own click.';
    const url = madeLinks.has(link) ? inviteMail(emails, link) : null;
    if (!url?.startsWith('mailto:')) return 'That link isn’t one Oby made. Close this and invite again.';
    try { await shell.openExternal(url); return null; } catch { return 'Oby couldn’t open your mail app. Copy the link and send it yourself.'; }
  }, 'Not this app’s page.');
  // Getting back in with a sign-in (SPHEREAPP-12): what the Obysphere at an address offers (no
  // click: two public reads); then, right after a click main saw and the owner's Sign in in main's
  // own dialog, a new owner key, the browser, the rebind, and this Oby's place. The new key replaces
  // the kept one only once the Obysphere has bound it.
  ipc(trusted).handle('sphere-rebind-offer', [string], async (_e, text) => {
    const where = sphereAddress(text);
    return where ? { sphere: where.sphere, host: where.host, providers: await rebindOffer(fetch, where.url) } : null;
  }, null);
  ipc(trusted).handle('sphere-rebind', [string, string], async (e, text, provider): Promise<string | null> => {
    if (!takeInput()) return 'Getting back in takes your own click.';
    const where = sphereAddress(text);
    if (!where) return 'That isn’t an Obysphere’s address.';
    const w = BrowserWindow.fromWebContents(e.sender) ?? show();
    const owner = await ownerNow();
    const { response } = await dialog.showMessageBox(w, {
      type: 'question', message: `Get back into ${where.sphere}${owner ? ` as ${owner}` : ''}?`,
      detail: `You’ll sign in in your browser. Your Obys there under your old key stop working, and your teammates’ Obys ask them to compare your new key. Do this only if you lost the Mac you joined from.`,
      buttons: ['Sign in', 'Cancel'], defaultId: 1, cancelId: 1,
    });
    if (response !== 0) return null;
    const r = await getBackIn(obid, fetch, where, provider, {
      newKey: generateKeyPairSync('ed25519').privateKey,
      listen: signinListener,
      open: async (url) => { if (/^https:\/\//.test(url)) await shell.openExternal(url); },
      keep: (k) => { replaceKey('owner-key', k); key = k; },
    });
    if (r.kind === 'failed') return r.why;
    void keep();
    if (!w.isDestroyed()) w.webContents.send('sphere', { sphere: where.sphere, name: where.sphere });
    return null;
  }, 'Not this app’s page.');

  // Add sign-in (SPHEREAPP-13): whether this owner linked one (no click: two reads), and linking the
  // account they sign in with, right after a click main saw, so they can get back in by themselves
  ipc(trusted).handle('sphere-signin-status', [], () => signinStatus(obid, ownerKey), null);
  ipc(trusted).handle('sphere-add-signin', [string], async (_e, provider) => {
    if (!takeInput()) return 'Adding sign-in takes your own click.';
    const r = await addSignin(obid, ownerKey, fetch, provider, {
      listen: signinListener,
      open: async (url) => { if (/^https:\/\//.test(url)) await shell.openExternal(url); },
    });
    return 'why' in r ? r.why : null;
  }, 'Not this app’s page.');

  // Make admin, Remove admin (SPHEREAPP-11): with the owner's admin token, only right after a click
  // or key main saw itself, one each
  ipc(trusted).handle('sphere-admin', [string, boolean], async (_e, owner, on): Promise<AdminsAnswer> => (
    takeInput() ? setAdmin(obid, ownerKey, owner, on).catch((e) => ({ admins: null, refused: 'failed' as const, why: e instanceof Error ? e.message : String(e) }))
      : { admins: null, refused: 'failed', why: 'Changing who’s admin takes your own click.' }), null);
  ipc(trusted).handle('sphere-withdraw', [string], async (_e, id) => (
    takeInput() ? invited(() => withdrawInvitation(obid, ownerKey, id)) : notInvited('Withdrawing a link takes your own click.')), null);
}
