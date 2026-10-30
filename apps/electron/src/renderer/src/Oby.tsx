import { useEffect, useRef, useState } from 'react';
import type { Update } from '../../shared/updates';

type Point = { x: number; y: number };
declare global {
  interface Window {
    app?: {
      onCursor(fn: (p: Point) => void): () => void;
      chrome: { lightsCenterY: number; lightsRight: number } | null;
      fullScreen: boolean;
      onFullScreen(fn: (on: boolean) => void): () => void;
      openExternal(url: string): void;
      pickFolders?(from?: string): Promise<string[]>;  // the Mac's folder picker (main), several at once, opening in from
      openLoginItems(): void;
      // Oby's alerts (preload, main/notify.ts)
      showing(place: { job?: string; conversation?: string } | null): void;
      onOpen(fn: (place: { job?: string; conversation?: string }) => void): () => void;
      notifications: { alerts(on?: boolean): Promise<boolean | null>; openSystemSettings(): void };
      // Oby's own updates (preload, main/updates.ts)
      updates?(ask?: 'check' | 'install' | boolean): Promise<{ auto: boolean; update: Update } | null>;
      onUpdate?(fn: (update: Update) => void): () => void;
      setupWindow?(on: boolean): number | null;  // the width the window's growing to (null: already there)
      // obid, through main (preload, main/obid.ts)
      obid: {
        call(method: string, path: string, body?: unknown): Promise<{ status: number; text: string }>;
        confirm(item: string): Promise<{ status: number; text: string }>;
        limitAnswer(item: string, revision: number, choice: string, account?: string): Promise<{ status: number; text: string }>;
        follow(onChunk: (text: string) => void, onEnd: (a: { status: number; text: string; why?: 'waiting' | 'approval' }) => void): () => void;
      };
    };
  }
}

// Oby in a small frame. It watches the cursor anywhere: the desktop app feeds
// it the screen-wide position; in a browser it gets the page's pointer. Oby
// decides by itself when a still cursor is worth a glance, or no longer. The
// frame stays hidden until Oby's first frame is drawn, so nothing the browser
// paints in it while it loads can flash.
// size: how big Oby is in its frame (oby.html's ?size); onDrawn: once its first frame is drawn;
// simple: Oby alone, no effects round it but the fly (?simple); enter: given, Oby waits under
// the floor until it's true, then hops up out of it (?enter); duck: how far under where it
// stands it stays, in bodies (a hop down, or back up; { depth, soft } rises or sinks there
// smoothly); cue: an expression to play, each time
// its n changes (Agree, a nod; Nope, a shake of the head)
// gravity: how fast its hops are (oby.html's ?gravity; lower is slower). For a little scene of a
// few, each in a frame of its own laid over the others: x, where it starts (bodies from the
// middle); scale, its size (changes grow or shrink it); move, one to make each time its n
// changes (oby.html's { walk }, { shove }, { shoved }); onEvent, what it says ('arrived', 'landed')
type Props = {
  paused?: boolean; glance?: boolean; size?: number; onDrawn?: () => void; simple?: boolean; enter?: boolean;
  duck?: number | { depth: number; soft: true }; cue?: { name: string; n: number }; gravity?: number;
  x?: number; scale?: number; move?: { msg: object; n: number }; onEvent?: (name: string) => void;
};
export default function Oby({ paused = false, glance = false, size = 1.23, onDrawn, simple = false, enter, duck = 0, cue, gravity, x, scale, move, onEvent }: Props) {
  const frame = useRef<HTMLIFrameElement>(null);
  // where the cursor last was, in the frame's coordinates (what a glance looks at)
  const last = useRef<Point | null>(null);
  // hidden (paused): clicks elsewhere in the app aren't Oby's business
  const hidden = useRef(paused);
  hidden.current = paused;

  useEffect(() => {
    const el = frame.current!;
    const send = (msg: object) => el.contentWindow?.postMessage(msg, '*');
    const look = (p: Point) => {
      const r = el.getBoundingClientRect();
      last.current = { x: p.x - r.left, y: p.y - r.top };
      send({ look: last.current });
    };

    // a click anywhere in the window catches Oby's eye,
    // while it's in sight
    const onClick = (e: PointerEvent) => {
      // (one passed on from its own frame is a poke, which the frame handles)
      if (hidden.current || e.target === el) return;
      const r = el.getBoundingClientRect();
      send({ notice: { x: e.clientX - r.left, y: e.clientY - r.top } });
    };
    addEventListener('pointerdown', onClick, true);

    if (window.app) {
      const stop = window.app.onCursor(look);
      return () => { stop(); removeEventListener('pointerdown', onClick, true); };
    }

    // In a browser: the page's pointer, plus the pointer over Oby's own frame
    // (same origin), turned into page coordinates
    const onPage = (e: PointerEvent) => look({ x: e.clientX, y: e.clientY });
    const onFrame = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      look({ x: e.clientX + r.left, y: e.clientY + r.top });
    };
    const hookFrame = () => el.contentWindow?.addEventListener('pointermove', onFrame);
    addEventListener('pointermove', onPage);
    el.addEventListener('load', hookFrame);
    hookFrame();
    return () => {
      removeEventListener('pointerdown', onClick, true);
      removeEventListener('pointermove', onPage);
      el.removeEventListener('load', hookFrame);
      el.contentWindow?.removeEventListener('pointermove', onFrame);
    };
  }, []);

  // A click on Oby's frame is a click in the page too: passed on from the frame
  // (same origin) as a pointerdown on it, so whatever closes on a click outside
  // it (a menu, the message box's panels) hears it as well
  useEffect(() => {
    const el = frame.current!;
    const pass = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, composed: true, clientX: e.clientX + r.left, clientY: e.clientY + r.top, pointerId: e.pointerId, pointerType: e.pointerType, button: e.button, buttons: e.buttons }));
    };
    // (a frame that isn't same-origin, as a packaged build could make it, is left alone)
    const hook = () => { try { el.contentWindow?.addEventListener('pointerdown', pass, true); } catch { /* not ours to reach */ } };
    el.addEventListener('load', hook);
    hook();
    return () => { el.removeEventListener('load', hook); try { el.contentWindow?.removeEventListener('pointerdown', pass, true); } catch { /* as above */ } };
  }, []);

  // Oby says when its first frame is drawn
  const [drawn, setDrawn] = useState(false);
  const onDrawnRef = useRef(onDrawn), onEventRef = useRef(onEvent);
  onDrawnRef.current = onDrawn; onEventRef.current = onEvent;
  // where it starts and its size, as it opens (later sizes: below)
  const [start] = useState(() => `${x !== undefined ? `&x=${x}` : ''}${scale !== undefined ? `&scale=${scale}` : ''}`);
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (e.source !== frame.current?.contentWindow || !e.data?.oby) return;
      if (e.data.oby === 'ready') { setDrawn(true); onDrawnRef.current?.(); }
      onEventRef.current?.(e.data.oby);
    };
    addEventListener('message', onMessage);
    return () => removeEventListener('message', onMessage);
  }, []);

  // ducked (before it comes in, so it comes up to there), then in: up out of the floor
  // (soft: a smooth rise or sink there, no hop)
  const ducked = typeof duck === 'number' ? { depth: duck, soft: false } : duck;
  useEffect(() => { if (drawn) frame.current!.contentWindow?.postMessage({ duck: ducked.depth, soft: ducked.soft }, '*'); }, [ducked.depth, ducked.soft, drawn]);
  // once drawn: the page only hears messages once it's loaded (sent sooner, it'd be lost and Oby
  // would stay under the floor)
  useEffect(() => { if (enter && drawn) frame.current!.contentWindow?.postMessage({ enter: true }, '*'); }, [enter, drawn]);
  useEffect(() => { if (cue?.n) frame.current!.contentWindow?.postMessage({ play: cue.name }, '*'); }, [cue?.n]);
  useEffect(() => { if (drawn && scale !== undefined) frame.current!.contentWindow?.postMessage({ scale }, '*'); }, [scale, drawn]);
  useEffect(() => { if (move?.n) frame.current!.contentWindow?.postMessage(move.msg, '*'); }, [move?.n]);

  // a glance when the host asks (e.g. the pointer arriving on the chips): Oby
  // looks over, like at a click, then goes back to its own business
  useEffect(() => {
    if (glance && last.current) frame.current!.contentWindow?.postMessage({ notice: last.current }, '*');
  }, [glance]);

  // stop drawing while it's out of sight: once it has faded out, at once when
  // back, and at once if the frame reloads while it's hidden
  useEffect(() => {
    const el = frame.current!;
    const post = (p: boolean) => el.contentWindow?.postMessage({ pause: p }, '*');
    if (!paused) { post(false); return; }
    const t = setTimeout(() => post(true), 300);
    const reloaded = () => post(true);
    el.addEventListener('load', reloaded);
    return () => { clearTimeout(t); el.removeEventListener('load', reloaded); };
  }, [paused]);

  return <iframe ref={frame} className="oby-frame" tabIndex={-1} src={`oby/oby.html?embed&size=${size}${simple ? '&simple' : ''}${enter !== undefined ? '&enter' : ''}${gravity ? `&gravity=${gravity}` : ''}${start}`} title="Oby" style={drawn ? undefined : { visibility: 'hidden' }} />;
}
