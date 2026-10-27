import { contextBridge, ipcRenderer } from 'electron';

type Point = { x: number; y: number };
// obid's answer: its status and body, or (status 0) why it didn't answer
type Answer = { status: number; text: string };

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
  // obid, through main, which holds its token (main/obid.ts): one call, and the event stream's
  // text as it comes, then how it ended. Following again replaces the stream before.
  obid: {
    call: (method: string, path: string, body?: unknown) => ipcRenderer.invoke('obid', method, path, body) as Promise<Answer>,
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
