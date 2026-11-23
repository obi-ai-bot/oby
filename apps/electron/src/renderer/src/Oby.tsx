import { useEffect, useRef, useState } from 'react';
import type { PolicyAnswer } from '../../shared/sphere';
import type { Update } from '../../shared/updates';
import type { LoadWhat } from '../../shared/decode';
import type { LiveMessage, LiveSnapshot } from '../../shared/live-wire';

type Point = { x: number; y: number };
// Settings › Notifications' switches for news that needs nothing (main/notify.ts)
type NotificationKinds = { finished: boolean; merged: boolean; limit: boolean };
declare global {
  interface Window {
    app?: {
      onCursor(fn: (p: Point) => void): () => void;
      chrome: { lightsCenterY: number; lightsRight: number } | null;
      fullScreen: boolean;
      onFullScreen(fn: (on: boolean) => void): () => void;
      // the Mac's menu bar's shortcut items, clicked (shared/menu.ts, shortcuts.ts)
      onShortcut?(fn: (command: string) => void): () => void;
      help?(what: 'reveal-log' | 'copy-diagnostics'): Promise<string | null>;  // the Help menu's items (main/help.ts)
      openExternal(url: string): void;
      // an artifact's version saved where the owner picks (main/artifactExport.ts)
      artifactExport?(id: string, version: number, as: 'zip' | 'html'): Promise<{ saved: string } | { cancelled: true } | { error: string } | null>;
      // an artifact's version shown by Sandpack over a box of the page, in main's own view (main/artifactPreview.ts); null hides it
      artifactPreview?(ask: { id: string; version: number; bounds: { x: number; y: number; width: number; height: number }; edits?: Record<string, string> } | null): Promise<{ shown: true; pageError?: string } | { error: string } | null>;
      // the version on view in the preview run again (main/artifactPreview.ts)
      artifactReload?(): Promise<null>;
      // a picture of an artifact's version, a PNG data: URL, taken in main's preview view (main/artifactPreview.ts)
      artifactThumb?(id: string, version: number): Promise<{ src: string } | { none: true } | null>;
      renderError?(name: string, message: string, componentStack: string): void;  // a render error the root caught (preload, main/runMarker.ts)
      pickFolders?(from?: string): Promise<string[]>;  // the Mac's folder picker (main), several at once, opening in from
      openLoginItems(): void;
      // Oby's alerts (preload, main/notify.ts)
      showing(place: { job?: string; conversation?: string } | null): void;
      onOpen(fn: (place: { job?: string; conversation?: string; settings?: string }) => void): () => void;
      notifications: {
        alerts(on?: boolean): Promise<boolean | null>;
        openSystemSettings(): void;
        // the news that needs nothing, each kind on or off (ALERTS-15)
        kinds(change?: Partial<NotificationKinds>): Promise<NotificationKinds | null>;
      };
      // Oby's own updates (preload, main/updates.ts)
      updates?(ask?: 'check' | 'install' | boolean): Promise<{ auto: boolean; update: Update } | null>;
      onUpdate?(fn: (update: Update) => void): () => void;
      // Settings rows obid has no setting for yet, kept by main (preload, main/settings.ts)
      settings?: { local(): Promise<Record<string, unknown> | null>; keep(key: string, value: unknown): Promise<boolean | null> };
      loginItem?(on?: boolean): Promise<boolean | null>;  // Open at login's login item, as macOS has it (preload, main/settings.ts)
      setupWindow?(on: boolean): number | null;  // the width the window's growing to (null: already there)
      // an Obysphere joined by its invite link, and Leave (preload, main/sphere.ts)
      onSphere?(fn: (joined: { sphere: string; name: string }) => void): () => void;
      sphereLeave?(): Promise<string | null>;
      // an invite link pasted in the app, joined through main's one join path; a task handed to a member's Oby (SPHEREAPP-3, 6)
      sphereJoin?(link: string): Promise<import('../../shared/invite').Joined>;
      sphereHandoff?(ask: import('../../shared/handoff').HandoffAsk): Promise<string | null>;
      // a task's worktree in the owner's Terminal or editor, by its id, and that editor's name (preload, main/worktree.ts)
      openWorktree?(job: string, where: 'terminal' | 'editor'): Promise<boolean>;
      editorName?(): Promise<string | null>;
      // Open on a task's local server, by its job's id and the port obid listed (preload, main/servers.ts)
      openTaskServer?(job: string, port: number): Promise<boolean>;
      // a reply's paths: which are files of its task or conversation, and one opened at its line (Reply.tsx)
      replyFiles?(place: 'task' | 'conversation', id: string, refs: string[]): Promise<boolean[]>;
      openReplyFile?(place: 'task' | 'conversation', id: string, ref: string): Promise<boolean>;
      // the Obysphere's rules, and the members rule added at the version read (preload, main/sphere.ts)
      spherePolicy?(): Promise<PolicyAnswer | null>;
      sphereSetPolicy?(version: number): Promise<PolicyAnswer | null>;
      // the admin's invite links: the list, a new one, a withdrawal (preload, main/sphere.ts, SPHEREAPP-10)
      sphereInvitations?(): Promise<import('../../shared/sphere').InvitesAnswer | null>;
      sphereInvite?(reuse?: boolean): Promise<import('../../shared/sphere').InvitesAnswer | null>;
      sphereWithdraw?(id: string): Promise<import('../../shared/sphere').InvitesAnswer | null>;
      sphereInviteMail?(emails: string[], link: string): Promise<string | null>;
      // Make admin or Remove admin (preload, main/sphere.ts, SPHEREAPP-11)
      sphereAdmin?(owner: string, on: boolean): Promise<import('../../shared/sphere').AdminsAnswer | null>;
      // getting back in with a sign-in: what an address offers, and the sign-in (preload, main/sphere.ts, SPHEREAPP-12)
      sphereRebindOffer?(address: string): Promise<{ sphere: string; host: string; providers: { id: string; name: string }[] | null } | null>;
      sphereRebind?(address: string, provider: string): Promise<string | null>;
      // Add sign-in: whether one is linked, and linking one (preload, main/sphere.ts, SPHEREAPP-13)
      sphereSigninStatus?(): Promise<import('../../shared/sphere').SigninStatus | null>;
      sphereAddSignin?(provider: string): Promise<string | null>;
      // an agent using the Mac: main's pill and frame (preload, main/macPill.ts, BROWSERAPP-19)
      mac?: {
        pill(use: { agent: string; action: string } | null): Promise<boolean | null>;
        onStopped(fn: () => void): () => void;
        onPill(fn: (use: import('./browser/Mac').MacUse) => void): () => void;
        stop(): void;
      };
      // obid's browser, followed by main (preload, main/browser.ts, BROWSERAPP-8): its events, each a
      // function that stops listening
      browser?: {
        follow(fn: (m: { name: string; data: string }) => void): () => void;
        // a password or code into the page's field, filled by main on the owner's click (BROWSERAPP-12): null once filled
        fill(tab: string, field: string, value: string, ask?: string): Promise<string | null>;
        // a <webview> mounted for obid's tab, relayed by main once checked (docs/plans/browser-t3.md): null once relayed
        register(threadId: string, tabId: string, webContentsId: number): Promise<string | null>;
        shown(threadId: string, tabId: string, shown: boolean): void;
        pointer(fn: (p: { threadId: string; tabId: string; phase: 'move' | 'click'; x: number; y: number }) => void): () => void;
        // the owner's Back, Forward, Reload or Stop loading on a tab's own page, by main (BROWSERAPP-30 to 35): null once done
        nav(threadId: string, tabId: string, op: 'back' | 'forward' | 'reload' | 'stop'): Promise<string | null>;
        navState(fn: (s: { threadId: string; tabId: string; url: string; canGoBack: boolean; canGoForward: boolean; loading: boolean; failed: string | null }) => void): () => void;
        // an owner's own tab's <webview>, kept by main as the owner's, never relayed (BROWSERAPP-45): null once kept
        // an owner's own tab about to mount its <webview>, in that partition at that address: main lets
        // one webview attach so (BROWSERAPP-45, 47): null once it will
        ownMount?(tabId: string, partition: string, src: string): Promise<string | null>;
        own?(tabId: string, webContentsId: number): Promise<string | null>;
        // the owner's own tab to an address they typed, by main, a web page only (BROWSERAPP-46): null once it goes
        go?(tabId: string, url: string): Promise<string | null>;
        // Show in Finder for a download in Downloads, by its name (BROWSERAPP-16)
        reveal(name: string): Promise<boolean>;
      };
      // obid, through main (preload, main/obid.ts); its events, from main's one follower (main/live.ts)
      obid: {
        call(method: string, path: string, body?: unknown): Promise<{ status: number; text: string }>;
        confirm(item: string): Promise<{ status: number; text: string }>;
        publish(job: string, action: string, choices: { anyway?: boolean; force?: boolean; to?: string }): Promise<{ status: number; text: string }>;
        // Undo this round: main asks in its own dialog and signs obid's sentence (CHANGES-14)
        undo(job: string, action: string): Promise<{ status: number; text: string }>;
        // A finishing action in a conversation's folder: main asks in its own dialog and signs obid's sentence (WORKSPACE-4, 5)
        finish(conversation: string, folder: string, action: string): Promise<{ status: number; text: string }>;
        limitAnswer(item: string, revision: number, choice: string, account?: string): Promise<{ status: number; text: string }>;
        pairInvite(): Promise<{ status: number; text: string }>;
        live(onSnapshot: (s: LiveSnapshot) => void, onMessage: (m: LiveMessage) => void): () => void;
        load(what: LoadWhat): Promise<true | string | null>;
        // the profile the window has open: true when main changed to it (B00-40)
        profile?(id: string | null): Promise<boolean | null>;
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
