import { contextBridge, ipcRenderer } from 'electron';
import { FILL_IN, WITHIN_MS, owners } from '../shared/words';
import type { AdminsAnswer, InvitesAnswer, PolicyAnswer, SigninStatus } from '../shared/sphere';
import type { HandoffAsk } from '../shared/handoff';
import type { Joined } from '../shared/invite';
import type { LoadWhat } from '../shared/decode';
import { following, type LiveMessage, type LiveSnapshot } from '../shared/live-wire';

type Point = { x: number; y: number };
// obid's answer: its status and body, or (status 0) why it didn't answer (main/obid.ts, reason)
type Answer = { status: number; text: string; why?: 'waiting' | 'approval' };

// What the owner's own keystrokes and clicks held (shared/words.ts): the text of the box they
// pressed Enter in, and, for a click, the boxes of the form they clicked in (Send) and the label
// of the button they clicked. Only events the person made count (isTrusted: a script's aren't).
// Each held text signs once, within WITHIN_MS. This is the preload's half: main signs only right
// after a keystroke or click it saw itself (main/words.ts), and only the app's own shapes.
const held: { text: string; at: number }[] = [];
const earlier: string[] = [];
const keep = <T>(list: T[], item: T) => { list.push(item); if (list.length > 50) list.shift(); };
const seen = (el: HTMLElement) => el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
function textOf(el: Element | null): string | null {
  if (!(el instanceof HTMLElement) || !seen(el)) return null;
  if (el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && ['text', 'search'].includes(el.type))) return el.value;
  const box = el.isContentEditable ? el.closest('[contenteditable="true"]') : null;
  return box instanceof HTMLElement ? box.innerText : null;
}
// a button's own label, never a whole card's text; a switch, which has no words of its own, by its
// aria-label (Settings › General's browser switches, BROWSERAPP-18)
const labelOf = (el: Element | null) => {
  const b = el?.closest('button');
  if (!b || !seen(b) || b.querySelector('button, input, textarea, [contenteditable="true"]')) return null;
  return b.innerText.trim() ? b.innerText : b.getAttribute('role') === 'switch' ? b.getAttribute('aria-label') : null;
};
function hold(e: Event) {
  if (!e.isTrusted || (e instanceof KeyboardEvent && ((e.key !== 'Enter' && e.key !== ' ') || e.isComposing))) return;
  const target = e.target instanceof Element ? e.target : null, at = Date.now();
  if (e.type === 'change') {
    if (target instanceof HTMLSelectElement && seen(target)) {
      const text = target.selectedOptions[0]?.text;
      if (text?.trim()) keep(held, { text, at });
    }
    return;
  }
  // Enter in a browser card's password or code box presses its Fill in (BROWSERAPP-12); the box's
  // value is never read here
  if (e instanceof KeyboardEvent && e.key === 'Enter' && target instanceof HTMLInputElement && seen(target)
    && (target.type === 'password' || target.autocomplete === 'one-time-code') && target.closest('[data-browser="card"]')) keep(held, { text: FILL_IN, at });
  // the keys that press a focused button hold its label; Enter holds the box it was pressed in
  const texts = e.type === 'keydown' ? [...(e instanceof KeyboardEvent && e.key === ' ' ? [] : [textOf(target)]), labelOf(target)]
    : [labelOf(target), ...[...(target?.closest('form')?.querySelectorAll('textarea, input, [contenteditable="true"]') ?? [])].map(textOf)];
  for (const text of texts) if (text?.trim()) keep(held, { text, at });
}
addEventListener('keydown', hold, true);
addEventListener('pointerdown', hold, true);
addEventListener('change', hold, true);
function signs(method: string, path: string, body: unknown) {
  const now = Date.now();
  const recent = held.filter((h) => now - h.at < WITHIN_MS);
  const used = owners(method, path, body, recent.map((h) => h.text), earlier);
  if (!used) return false;
  // each held text signs once
  for (const h of used.map((i) => recent[i])) held.splice(held.indexOf(h), 1);
  // the words an answer to Oby's question sends again
  if (path === '/v1/plans') keep(earlier, (body as { said: string }).said);
  return true;
}

// Whether the owner's own event just pressed a button with this label (each press once)
function pressed(label: string) {
  const now = Date.now();
  const at = held.findIndex((h) => now - h.at < WITHIN_MS && h.text.trim() === label);
  if (at >= 0) held.splice(at, 1);
  return at >= 0;
}
const pressedConfirm = () => pressed('Confirm');

contextBridge.exposeInMainWorld('app', {
  // where macOS put the traffic lights, in CSS px (null without the native
  // window style); the page lays its title bar out from these
  chrome: ipcRenderer.sendSync('window-chrome') as { lightsCenterY: number; lightsRight: number } | null,
  // whether the window is full screen (no traffic lights), and changes to it
  fullScreen: ipcRenderer.sendSync('full-screen') as boolean,
  onFullScreen(fn: (on: boolean) => void) {
    const listener = (_: unknown, on: boolean) => fn(on);
    ipcRenderer.on('full-screen', listener);
    return () => { ipcRenderer.off('full-screen', listener); };
  },
  // a shortcut item of the Mac's menu bar, clicked (shared/menu.ts): its command (renderer shortcuts.ts)
  onShortcut(fn: (command: string) => void) {
    const listener = (_: unknown, command: string) => fn(command);
    ipcRenderer.on('shortcut', listener);
    return () => { ipcRenderer.off('shortcut', listener); };
  },
  // the Help menu's items, from ⌘K's Actions (main/help.ts, docs/contracts/help.md): null when done,
  // else what went wrong
  help: (what: 'reveal-log' | 'copy-diagnostics') => ipcRenderer.invoke('help', what) as Promise<string | null>,
  // the cursor in window coordinates, even when it's outside the window
  onCursor(fn: (p: Point) => void) {
    const listener = (_: unknown, p: Point) => fn(p);
    ipcRenderer.on('cursor', listener);
    return () => { ipcRenderer.off('cursor', listener); };
  },
  // an artifact's version shown by Sandpack over the page's box for it, in main's view of its own
  // (main/artifactPreview.ts, ARTIFACTSAPP-1); null hides it. Only the id, the version and the box go
  artifactPreview: (ask: { id: string; version: number; bounds: { x: number; y: number; width: number; height: number }; edits?: Record<string, string> } | null) =>
    ipcRenderer.invoke('artifact-preview', ask) as Promise<{ shown: true } | { error: string } | null>,
  // a sign-in page in the person's browser (https only)
  openExternal: (url: string) => ipcRenderer.send('open-external', url),
  // a render error the page's root boundary caught (RenderError.tsx): kept by main, which reloads once
  renderError: (name: string, message: string, componentStack: string) => ipcRenderer.send('render-error', name, message, componentStack),
  // the Mac's folder picker, several folders at once, opening in from: their paths (none: cancelled)
  pickFolders: (from?: string) => ipcRenderer.invoke('pick-folders', from) as Promise<string[]>,
  // setting up (the page's FirstRun.tsx): the window narrower and held at its size, or its own
  // size back (it grows); answered once main has done it, so it happens before the window shows
  setupWindow: (on: boolean): number | null => ipcRenderer.sendSync('setup-window', on),
  // System Settings › General › Login Items, where macOS asks to approve obid
  openLoginItems: () => ipcRenderer.send('open-login-items'),
  // Oby's alerts (main/notify.ts): what the page shows (a job's page, a conversation, {} for
  // Home), so nothing alerts for it; where a clicked alert opens, asked of main whenever it says
  // one was (and once now, for the click that opened the window); and Settings › Notifications
  showing: (place: { job?: string; conversation?: string } | null) => ipcRenderer.send('showing', place),
  onOpen(fn: (place: { job?: string; conversation?: string; settings?: string }) => void) {
    const pull = () => void ipcRenderer.invoke('opened').then((place) => { if (place) fn(place); });
    ipcRenderer.on('open', pull);
    pull();
    return () => { ipcRenderer.off('open', pull); };
  },
  notifications: {
    // on or off; with a value, turned to it
    alerts: (on?: boolean) => ipcRenderer.invoke('notifications', on) as Promise<boolean | null>,
    // the news that needs nothing, each kind on or off; with a change, turned to it (ALERTS-15)
    kinds: (change?: { finished?: boolean; merged?: boolean; limit?: boolean }) =>
      ipcRenderer.invoke('notification-kinds', change) as Promise<{ finished: boolean; merged: boolean; limit: boolean } | null>,
    openSystemSettings: () => ipcRenderer.send('open-notification-settings'),
  },
  // Oby's own updates (main/updates.ts): what About shows and Update automatically, after the
  // owner's click ('check', 'install') or Update automatically turned; and what it shows as it changes
  updates: (ask?: 'check' | 'install' | boolean) => ipcRenderer.invoke('updates', ask),
  onUpdate(fn: (update: unknown) => void) {
    const listener = (_: unknown, update: unknown) => fn(update);
    ipcRenderer.on('update', listener);
    return () => { ipcRenderer.off('update', listener); };
  },
  // Settings rows obid has no setting for yet, kept by main (main/settings.ts): all of them, and
  // one of them kept (null: refused)
  settings: {
    local: () => ipcRenderer.invoke('local-settings') as Promise<Record<string, unknown> | null>,
    keep: (key: string, value: unknown) => ipcRenderer.invoke('local-setting', key, value) as Promise<boolean | null>,
  },
  // Open at login: the app's login item as macOS has it; with a value, turned to it (main/settings.ts; null: none here)
  loginItem: (on?: boolean) => ipcRenderer.invoke('login-item', on) as Promise<boolean | null>,
  // an Obysphere joined by its invite link (main/sphere.ts), and Leave from its settings: main
  // holds the owner key; Leave answers why the Obysphere wasn't told, or null
  onSphere(fn: (joined: { sphere: string; name: string }) => void) {
    const listener = (_: unknown, joined: { sphere: string; name: string }) => fn(joined);
    ipcRenderer.on('sphere', listener);
    return () => { ipcRenderer.off('sphere', listener); };
  },
  // a task's worktree in the owner's Terminal or editor (main/worktree.ts, CHANGES-6): only its id
  // goes; true once opened. The editor's name, for the menu
  openWorktree: (job: string, where: 'terminal' | 'editor') => ipcRenderer.invoke('open-worktree', job, where) as Promise<boolean>,
  editorName: () => ipcRenderer.invoke('worktree-editor') as Promise<string | null>,
  // Open on a task's local server (main/servers.ts, TASKSERVERS-8): only the job's id and the port go;
  // true once main opened http://localhost:<port>, which it does only for a port obid lists
  openTaskServer: (job: string, port: number) => ipcRenderer.invoke('open-task-server', job, port) as Promise<boolean>,
  // a reply's paths (main/worktree.ts, CHATFIRST-39 and 40): which are files of its task's worktree
  // or its conversation's folders, and one opened in the editor at its line; only the words and
  // where the reply is go, main finds the file
  replyFiles: (place: 'task' | 'conversation', id: string, refs: string[]) => ipcRenderer.invoke('reply-files', place, id, refs) as Promise<boolean[]>,
  openReplyFile: (place: 'task' | 'conversation', id: string, ref: string) => ipcRenderer.invoke('open-reply-file', place, id, ref) as Promise<boolean>,
  sphereLeave: () => ipcRenderer.invoke('sphere-leave') as Promise<string | null>,
  // an invite link pasted in the app (first run, Settings › Obysphere), joined through main's one
  // join path, its own dialog included, only right after the owner's click (SPHEREAPP-3, 4)
  sphereJoin: (link: string) => ipcRenderer.invoke('sphere-join', link) as Promise<Joined>,
  // a task handed to a member's Oby, right after the owner's click (SPHEREAPP-6): null once sent, else why not
  sphereHandoff: (ask: HandoffAsk) => ipcRenderer.invoke('sphere-handoff', ask) as Promise<string | null>,
  // the Obysphere's rules (main/sphere.ts, docs/contracts/sphere-policy.md): read, and the members
  // rule added, right after the owner's click. The page sends only the version it read; main decides
  // what it signs. null: refused
  spherePolicy: () => ipcRenderer.invoke('sphere-policy') as Promise<PolicyAnswer | null>,
  sphereSetPolicy: (version: number) => ipcRenderer.invoke('sphere-set-policy', version) as Promise<PolicyAnswer | null>,
  // the admin's invite links (main/sphere.ts, SPHEREAPP-10): the list; a new one and a withdrawal,
  // right after the owner's click. Main holds the owner token; the page gets the link and the list
  sphereInvitations: () => ipcRenderer.invoke('sphere-invitations') as Promise<InvitesAnswer | null>,
  sphereInvite: () => ipcRenderer.invoke('sphere-invite') as Promise<InvitesAnswer | null>,
  sphereWithdraw: (id: string) => ipcRenderer.invoke('sphere-withdraw', id) as Promise<InvitesAnswer | null>,
  // Make admin (on) or Remove admin, right after the owner's click (main/sphere.ts, SPHEREAPP-11)
  sphereAdmin: (owner: string, on: boolean) => ipcRenderer.invoke('sphere-admin', owner, on) as Promise<AdminsAnswer | null>,
  // getting back in with a sign-in (main/sphere.ts, SPHEREAPP-12): what an Obysphere's address
  // offers, and the sign-in itself, right after the owner's click (null once back in, else why not)
  sphereRebindOffer: (address: string) => ipcRenderer.invoke('sphere-rebind-offer', address) as Promise<{ sphere: string; host: string; providers: { id: string; name: string }[] | null } | null>,
  sphereRebind: (address: string, provider: string) => ipcRenderer.invoke('sphere-rebind', address, provider) as Promise<string | null>,
  // Add sign-in (main/sphere.ts, SPHEREAPP-13): whether one is linked, and linking one right after the owner's click
  sphereSigninStatus: () => ipcRenderer.invoke('sphere-signin-status') as Promise<SigninStatus | null>,
  sphereAddSignin: (provider: string) => ipcRenderer.invoke('sphere-add-signin', provider) as Promise<string | null>,
  // an agent using the Mac (main/macPill.ts, BROWSERAPP-19): the window tells main who has it (null:
  // nobody), and hears its Stop; the pill's own window hears who has it and sends its Stop
  mac: {
    pill: (use: { agent: string; action: string } | null) => ipcRenderer.invoke('mac-pill', use) as Promise<boolean | null>,
    onStopped(fn: () => void) {
      const listener = () => fn();
      ipcRenderer.on('mac-stopped', listener);
      return () => { ipcRenderer.off('mac-stopped', listener); };
    },
    onPill(fn: (use: { agent: string; action: string }) => void) {
      const listener = (_: unknown, use: { agent: string; action: string }) => fn(use);
      ipcRenderer.on('mac-pill', listener);
      // who has it now, asked once listening (nothing is missed before it)
      void (ipcRenderer.invoke('mac-pill-now') as Promise<{ agent: string; action: string } | null>).then((use) => { if (use) fn(use); });
      return () => { ipcRenderer.off('mac-pill', listener); };
    },
    stop: () => ipcRenderer.send('mac-pill-stop'),
  },
  // the owner's own shell in a task's or conversation's working copy (main/terminal.ts,
  // docs/contracts/terminal.md): opened right after the owner's click main saw, with only where and
  // the size the page measured (main finds the folder); what's typed, which main lets through only as
  // it saw it typed; a new size; closed; and each shell's output and end
  terminal: {
    open: (place: 'task' | 'conversation', id: string, cols: number, rows: number) =>
      ipcRenderer.invoke('terminal-open', place, id, cols, rows) as Promise<{ term: string; cwd: string; name: string } | { error: string }>,
    write: (term: string, data: string) => ipcRenderer.send('terminal-write', term, data),
    resize: (term: string, cols: number, rows: number) => ipcRenderer.send('terminal-resize', term, cols, rows),
    close: (term: string) => ipcRenderer.send('terminal-close', term),
    onData(fn: (term: string, data: string) => void) {
      const listener = (_: unknown, term: string, data: string) => fn(term, data);
      ipcRenderer.on('terminal-data', listener);
      return () => { ipcRenderer.off('terminal-data', listener); };
    },
    onExit(fn: (term: string, code: number, signal: number | null) => void) {
      const listener = (_: unknown, term: string, code: number, signal: number | null) => fn(term, code, signal);
      ipcRenderer.on('terminal-exit', listener);
      return () => { ipcRenderer.off('terminal-exit', listener); };
    },
  },
  // obid's browser, followed by main, which holds the token (main/browser.ts, BROWSERAPP-8): its events
  // from now on (`open` each time the stream connects again), and a tab's frames while the page draws
  // it; each a function that stops listening
  browser: {
    follow(fn: (m: { name: string; data: string }) => void) {
      const listener = (_: unknown, m: { name: string; data: string }) => fn(m);
      ipcRenderer.on('browser-event', listener);
      void ipcRenderer.invoke('browser-follow');
      return () => { ipcRenderer.off('browser-event', listener); };
    },
    // a password or code into the page's field, filled by main on the owner's press of Fill in (or
    // Enter in the card's box): main checks it saw that input itself; null once filled, else why not
    fill: (tab: string, field: string, value: string, ask?: string) => ipcRenderer.invoke('browser-fill', tab, field, value, ask ?? '', pressed(FILL_IN)) as Promise<string | null>,
    // a <webview> the page mounted for obid's tab, by its webContents id: main checks it's this
    // window's and in the partition obid named, then relays it (docs/plans/browser-t3.md); null once relayed
    register: (threadId: string, tabId: string, webContentsId: number) => ipcRenderer.invoke('browser-register', threadId, tabId, webContentsId) as Promise<string | null>,
    // whether that webview is on screen
    shown: (threadId: string, tabId: string, shown: boolean) => { void ipcRenderer.invoke('browser-shown', threadId, tabId, shown); },
    // the agent's pointer on a tab, to draw over its webview (move, then click); a function that stops listening
    pointer(fn: (p: { threadId: string; tabId: string; phase: 'move' | 'click'; x: number; y: number }) => void) {
      const listener = (_: unknown, p: { threadId: string; tabId: string; phase: 'move' | 'click'; x: number; y: number }) => fn(p);
      ipcRenderer.on('browser-pointer', listener);
      return () => { ipcRenderer.off('browser-pointer', listener); };
    },
    // Show in Finder for a download in Downloads, by its name (main/browser.ts): true once shown
    reveal: (name: string) => ipcRenderer.invoke('browser-reveal', name) as Promise<boolean>,
    watch(tab: string, fn: (data: string) => void) {
      const listener = (_: unknown, m: { tab: string; data: string }) => { if (m.tab === tab) fn(m.data); };
      ipcRenderer.on('browser-frame', listener);
      void ipcRenderer.invoke('browser-watch', tab);
      return () => { ipcRenderer.off('browser-frame', listener); void ipcRenderer.invoke('browser-unwatch', tab); };
    },
  },
  // obid, through main, which holds its token (main/obid.ts): one call; and what main's one follower
  // of obid's events holds (main/live.ts), then each message it sends, and what it reads for the page
  obid: {
    // the owner's words go signed (signs): main adds this Mac's signature
    call: (method: string, path: string, body?: unknown) => ipcRenderer.invoke('obid', method, path, body, signs(method, path, body)) as Promise<Answer>,
    // Confirm on an open confirm item, by its id: main signs it with this Mac's device key
    confirm: (item: string) => ipcRenderer.invoke('obid-confirm', item, pressedConfirm()) as Promise<Answer>,
    // Publish by hand, on the owner's click: main reads obid's sentence again and signs it
    publish: (job: string, action: string, choices: { anyway?: boolean; force?: boolean; to?: string }) =>
      ipcRenderer.invoke('obid-publish', job, action, choices) as Promise<Answer>,
    // Undo this round, on the owner's click: main reads obid's sentence again, asks in its own
    // dialog and signs it (CHANGES-14)
    undo: (job: string, action: string) => ipcRenderer.invoke('obid-undo', job, action) as Promise<Answer>,
    // A finishing action in a conversation's folder (WORKSPACE-4, 5), on the owner's click: main reads
    // obid's sentence for it, asks in its own dialog and signs it
    finish: (conversation: string, folder: string, action: string) =>
      ipcRenderer.invoke('obid-finish', conversation, folder, action) as Promise<Answer>,
    limitAnswer: (item: string, revision: number, choice: string, account?: string) =>
      ipcRenderer.invoke('obid-limit-answer', item, revision, choice, account) as Promise<Answer>,
    // Pair a device: main signs the invitation with this Mac's device key
    pairInvite: () => ipcRenderer.invoke('obid-pair-invite') as Promise<Answer>,
    // main's snapshot, then every message after it once, in order (shared/live-wire.ts); a function
    // that stops listening
    live: (onSnapshot: (s: LiveSnapshot) => void, onMessage: (m: LiveMessage) => void) => following((on) => {
      const listener = (_: unknown, m: LiveMessage) => on(m);
      ipcRenderer.on('obid-live', listener);
      return () => { ipcRenderer.off('obid-live', listener); };
    }, () => ipcRenderer.invoke('obid-snapshot') as Promise<LiveSnapshot | null>, onSnapshot, onMessage),
    // a conversation's turns, a job, or the limit decisions, read and kept by main: true, else why not
    load: (what: LoadWhat) => ipcRenderer.invoke('obid-load', what) as Promise<true | string | null>,
    // the profile the window has open, named on every call and the stream from now on (B00-40)
    profile: (id: string | null) => ipcRenderer.invoke('obid-profile', id) as Promise<boolean | null>,
  },
});
