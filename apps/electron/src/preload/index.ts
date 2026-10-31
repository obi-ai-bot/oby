import { contextBridge, ipcRenderer } from 'electron';
import { HELD_MS, owners } from '../shared/words';

type Point = { x: number; y: number };
// obid's answer: its status and body, or (status 0) why it didn't answer (main/obid.ts, reason)
type Answer = { status: number; text: string; why?: 'waiting' | 'approval' };

// What the owner's own keystrokes and clicks held (shared/words.ts): the text of the box they
// pressed Enter in, or were in when they clicked (Send, or away from a name being edited), the
// boxes of the form they clicked in, and the label of the button they pressed. Only events the person made count (isTrusted: a script's
// aren't), read here in the preload's own world, which the page's script can't reach or change.
// The page gets main to sign words only by passing this check; anything else goes unsigned.
const held: { text: string; at: number }[] = [];
const earlier: string[] = [];
const keep = (list: unknown[], item: unknown) => { list.push(item); if (list.length > 50) list.shift(); };
function textOf(el: Element | null): string | null {
  if (!(el instanceof HTMLElement) || !el.checkVisibility()) return null;
  if (el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && ['text', 'search'].includes(el.type))) return el.value;
  if (el.isContentEditable) return (el.closest('[contenteditable="true"]') as HTMLElement | null)?.innerText ?? null;
  return null;
}
const labelOf = (el: Element | null) => {
  const b = el?.closest('button, [role="button"], [role="menuitem"], [role="option"]');
  return b instanceof HTMLElement && b.checkVisibility() ? b.innerText : null;
};
function hold(e: Event) {
  if (!e.isTrusted || (e instanceof KeyboardEvent && (e.key !== 'Enter' || e.isComposing))) return;
  const target = e.target instanceof Element ? e.target : null, at = Date.now();
  const form = [...(target?.closest('form')?.querySelectorAll('textarea, input, [contenteditable="true"]') ?? [])];
  for (const text of [textOf(e.type === 'keydown' ? target : document.activeElement), labelOf(target), ...form.map(textOf)]) if (text?.trim()) keep(held, { text, at });
}
addEventListener('keydown', hold, true);
addEventListener('pointerdown', hold, true);
function signs(method: string, path: string, body: unknown) {
  const now = Date.now();
  const mine = owners(method, path, body, held.filter((h) => now - h.at < HELD_MS).map((h) => h.text), earlier);
  // the words an answer to Oby's question sends again
  if (mine && path === '/v1/plans') keep(earlier, (body as { said: string }).said);
  return mine;
}

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
  // the cursor in window coordinates, even when it's outside the window
  onCursor(fn: (p: Point) => void) {
    const listener = (_: unknown, p: Point) => fn(p);
    ipcRenderer.on('cursor', listener);
    return () => { ipcRenderer.off('cursor', listener); };
  },
  // a sign-in page in the person's browser (https only)
  openExternal: (url: string) => ipcRenderer.send('open-external', url),
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
  onOpen(fn: (place: { job?: string; conversation?: string }) => void) {
    const pull = () => void ipcRenderer.invoke('opened').then((place) => { if (place) fn(place); });
    ipcRenderer.on('open', pull);
    pull();
    return () => { ipcRenderer.off('open', pull); };
  },
  notifications: {
    // on or off; with a value, turned to it
    alerts: (on?: boolean) => ipcRenderer.invoke('notifications', on) as Promise<boolean | null>,
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
  // an Obysphere joined by its invite link (main/sphere.ts), and Leave from its settings: main
  // holds the owner key; Leave answers why the Obysphere wasn't told, or null
  onSphere(fn: (joined: { sphere: string; name: string }) => void) {
    const listener = (_: unknown, joined: { sphere: string; name: string }) => fn(joined);
    ipcRenderer.on('sphere', listener);
    return () => { ipcRenderer.off('sphere', listener); };
  },
  sphereLeave: () => ipcRenderer.invoke('sphere-leave') as Promise<string | null>,
  // obid, through main, which holds its token (main/obid.ts): one call, and the event stream's
  // text as it comes, then how it ended. Following again replaces the stream before.
  obid: {
    // the owner's words go signed (signs): main adds this Mac's signature
    call: (method: string, path: string, body?: unknown) => ipcRenderer.invoke('obid', method, path, body, signs(method, path, body)) as Promise<Answer>,
    // Confirm on an open confirm item, by its id: main signs it with this Mac's device key
    confirm: (item: string) => ipcRenderer.invoke('obid-confirm', item) as Promise<Answer>,
    limitAnswer: (item: string, revision: number, choice: string, account?: string) =>
      ipcRenderer.invoke('obid-limit-answer', item, revision, choice, account) as Promise<Answer>,
    follow(onChunk: (text: string) => void, onEnd: (a: Answer) => void) {
      const chunk = (_: unknown, text: string) => onChunk(text);
      const end = (_: unknown, a: Answer) => { stop(); onEnd(a); };
      const stop = () => { ipcRenderer.off('obid-chunk', chunk); ipcRenderer.off('obid-end', end); };
      ipcRenderer.on('obid-chunk', chunk);
      ipcRenderer.on('obid-end', end);
      ipcRenderer.send('obid-follow');
      return stop;
    },
  },
});
