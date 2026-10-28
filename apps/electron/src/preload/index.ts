import { contextBridge, ipcRenderer } from 'electron';

type Point = { x: number; y: number };
// obid's answer: its status and body, or (status 0) why it didn't answer (main/obid.ts, reason)
type Answer = { status: number; text: string; why?: 'waiting' | 'approval' };

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
  // obid, through main, which holds its token (main/obid.ts): one call, and the event stream's
  // text as it comes, then how it ended. Following again replaces the stream before.
  obid: {
    call: (method: string, path: string, body?: unknown) => ipcRenderer.invoke('obid', method, path, body) as Promise<Answer>,
    // Confirm on an open confirm item, by its id: main signs it with this Mac's device key
    confirm: (item: string) => ipcRenderer.invoke('obid-confirm', item) as Promise<Answer>,
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
