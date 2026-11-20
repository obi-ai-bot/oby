// An agent using the Mac (docs/contracts/browser-app.md, BROWSERAPP-19 and 20; the design's boards 15
// and 16), moved from the lab (tools/obid-spec/lab-ui/browser/src/desktop.tsx): Oby's pill over the
// screen with Stop (main's own small window, main/macPill.ts), the orange frame around the screen, the
// card in Oby, and the first time, Oby's sheet before macOS asks. Words are the boards'. The lab drew
// the Mac's own screen around them; here the screen is the Mac's.
import { useEffect, useState } from 'react';
import { Hand, Monitor, MousePointer2 } from 'lucide-react';
import { ObyFace } from '../ObyFace';
import { Logo, Sheen, agentName } from './pieces';
import { useBrowser } from './store';
import type { AgentId } from './view';

export type MacUse = { agent: AgentId; action: string };

/** 15: the pill over the screen, in main's own window: who has the Mac, what it's doing, Esc twice, Stop. */
export function MacPill({ use, onStop }: { use: MacUse; onStop: () => void }) {
  return (
    <div className="control-pill" role="status" aria-label={`${agentName(use.agent)} is using your Mac`} data-browser="pill">
      <span style={{ width: 18, height: 18, display: 'block' }}><ObyFace size={18} /></span>
      <span className="pill-sep" aria-hidden="true" />
      <Logo agent={use.agent} size={15} />
      <span><b>{agentName(use.agent)}</b> is using your Mac · <Sheen>{use.action}</Sheen></span>
      <span className="kbd">esc</span><span className="kbd" style={{ marginLeft: -6 }}>esc</span>
      <button className="tc-option is-primary" style={{ height: 30 }} onClick={onStop}>Stop</button>
    </div>
  );
}

/** 15: the card in Oby, over the message box, while an agent has the Mac. */
export function MacCard() {
  const { s, api } = useBrowser();
  if (!s.mac) return null;
  return (
    <div className="decision-bar" data-browser="mac-card">
      <div className="decision-card" style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <Monitor size={16} strokeWidth={1.6} style={{ color: 'var(--needs)' }} />
        <span style={{ flex: 1, color: 'var(--text)', fontSize: 13 }}><b style={{ color: 'var(--ink)', fontWeight: 500 }}>{agentName(s.mac.agent)} has your Mac.</b> Press Esc twice to stop it.</span>
        <button className="tc-option is-sm is-primary" onClick={() => void api.stopMac()}>Stop</button>
      </div>
    </div>
  );
}

/** 16: the first time an agent would use the Mac, Oby says what that means, before macOS asks. */
export function MacSheet() {
  const { s, api } = useBrowser();
  const ask = s.macAsk;
  if (!ask) return null;
  const who = agentName(ask.agent);
  return (
    <div className="scrim" data-browser="mac-sheet">
      <div className="modal" role="dialog" aria-labelledby="mac-sheet-title">
        <h2 className="modal-title" id="mac-sheet-title">Let agents use your Mac?</h2>
        <p className="modal-text">{who} wants to {ask.wants}. Agents use other apps through agent-desktop, the same way you do.</p>
        <ul className="modal-list">
          <li><Monitor size={16} strokeWidth={1.6} /><span><b>It sees the screen</b> while it works, to know where to click.</span></li>
          <li><MousePointer2 size={16} strokeWidth={1.6} /><span><b>It clicks and types</b> in other apps. You see a frame around the screen while it does.</span></li>
          <li><Hand size={16} strokeWidth={1.6} /><span><b>You stop it any time:</b> press Esc twice, or Stop.</span></li>
        </ul>
        <div className="steps"><b>Next, macOS asks you, once.</b> Allow agent-desktop in System Settings › Privacy &amp; Security, under Accessibility and then Screen Recording. Oby never asks macOS for these itself.</div>
        <div className="modal-actions"><button className="set-button" onClick={() => void api.answerMac(false)}>Not now</button><button className="primary-btn" onClick={() => void api.answerMac(true)}>Continue</button></div>
      </div>
    </div>
  );
}

/**
 * The window's side of the pill: while an agent has the Mac, main shows its pill and frame over the
 * screen (window.app.macPill); its Stop, or Esc twice, comes back as a stop.
 */
export function useMacPill() {
  const { s, api } = useBrowser();
  const mac = s.mac;
  useEffect(() => {
    const app = window.app?.mac;
    if (!app) return;
    void app.pill(mac ?? null);
  }, [mac?.agent, mac?.action]);
  useEffect(() => window.app?.mac?.onStopped(() => void api.stopMac()), [api]);
}

/** The pill's own window (?macpill) and the frame's (?macframe), as main opens them. */
export function MacOverlay({ what }: { what: 'pill' | 'frame' }) {
  const [use, setUse] = useState<MacUse | null>(null);
  useEffect(() => {
    document.documentElement.classList.add('is-mac-overlay');
    return window.app?.mac?.onPill(setUse);
  }, []);
  if (what === 'frame') return <div className="mac-glow" aria-hidden="true" data-browser="mac-frame" />;
  return use ? <MacPill use={use} onStop={() => window.app?.mac?.stop()} /> : null;
}
