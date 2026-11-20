// 15 and 16, the Mac itself (proposal): while an agent controls the Mac, Oby's pill over the screen
// with Stop (and Esc twice), an orange frame, and a card in Oby; the first time, Oby explains before
// macOS asks for agent-desktop's permissions. Drawn here as their own pages; the real Stop is proven
// with agent-desktop's nodes (workflows/lab/ui/15-desktop.json), skipped without its permissions.
import { useEffect, useState } from 'react';
import { Hand, Monitor, MousePointer2 } from 'lucide-react';
import { ObyFace } from '@app/ObyFace';
import { Composer, Log, Logo, LogRow, Sheen, TaskCard, Thread, Window, You } from './pieces';

/** 15: an agent has the Mac. Esc twice, or Stop, ends it: the pill and the frame go. */
export function Desktop() {
  const [stopped, setStopped] = useState(false);
  useEffect(() => {
    let last = 0;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      const now = performance.now();
      if (now - last < 600) setStopped(true);
      last = now;
    };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, []);
  return (
    <div className="oby mac lab-win" style={{ width: 1440, height: 900 }} data-lab-window data-stopped={stopped || undefined}>
      <div className="mac-menu"><b>TextEdit</b><span>File</span><span>Edit</span><span>Format</span><span>View</span><span>Window</span><span>Help</span><span className="mac-right"><span>Thu 2:31 PM</span></span></div>
      {!stopped && <div className="mac-glow" aria-hidden="true" data-lab="glow" />}
      {!stopped && (
        <div className="control-pill" role="status" aria-label="Claude Code is using your Mac" data-lab="pill">
          <span style={{ width: 18, height: 18, display: 'block' }}><ObyFace size={18} /></span>
          <span className="pill-sep" aria-hidden="true" />
          <Logo agent="claude" size={15} />
          <span><b>Claude Code</b> is using your Mac · <Sheen>Typing in TextEdit</Sheen></span>
          <span className="kbd">esc</span><span className="kbd" style={{ marginLeft: -6 }}>esc</span>
          <button className="tc-option is-primary" style={{ height: 30 }} onClick={() => setStopped(true)}>Stop</button>
        </div>
      )}
      <div className="mac-app doc" style={{ left: 160, top: 120, width: 700, height: 520 }}>
        <div className="doc-bar"><i /><i /><i /><span>Release checklist.txt</span></div>
        <div className="doc-body">
          <p><b>Release 0.9 checklist</b></p>
          <p>1. Sign-up flow checked on staging</p>
          <p>2. Pricing page matches the plans</p>
          <p>3. Button stories look right</p>
          <p>4. Invite emails go out<span className="caret-doc" /></p>
        </div>
      </div>
      {!stopped && (
        <div className="agent-pointer" style={{ left: 420, top: 330 }}>
          <svg width="22" height="22" viewBox="0 0 24 24" fill="#111" stroke="#fff" strokeWidth="1.6" strokeLinejoin="round" aria-hidden="true"><path d="M4.037 4.688a.495.495 0 0 1 .651-.651l16 6.5a.5.5 0 0 1-.063.947l-6.124 1.58a2 2 0 0 0-1.438 1.435l-1.579 6.126a.5.5 0 0 1-.947.063z" /></svg>
          <span className="agent-label"><Logo agent="claude" size={14} />Typing in TextEdit</span>
        </div>
      )}
      <div className="mac-app mini-oby" style={{ left: 900, top: 300, width: 500, height: 520, background: 'var(--bg)' }}>
        <div className="app" style={{ '--column': '460px', height: '100%' } as React.CSSProperties}>
          <div className="home">
            <div className="bar is-bare" style={{ paddingLeft: 84 }}><span className="crumb is-here">Release checklist</span></div>
            <div className="chat-layer">
              <Thread>
                <You time="2:30 PM">Add the invite emails to my checklist in TextEdit.</You>
                <div role="listitem" className="msg is-oby" style={{ marginTop: 16 }}><div className="msg-task-work">
                  <TaskCard agent="claude" title="Update the checklist" meta="Running · Claude Code · on your Mac" dot="working" />
                  <Log>
                    <LogRow mark="check">Brought TextEdit to the front</LogRow>
                    <LogRow mark="working"><Sheen>Typing in TextEdit</Sheen></LogRow>
                  </Log>
                </div></div>
              </Thread>
              {!stopped && (
                <div className="decision-bar" style={{ paddingBottom: 16 }}>
                  <div className="decision-card" style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <Monitor size={16} strokeWidth={1.6} style={{ color: 'var(--needs)' }} />
                    <span style={{ flex: 1, color: 'var(--text)', fontSize: 13 }}><b style={{ color: 'var(--ink)', fontWeight: 500 }}>Claude Code has your Mac.</b> Press Esc twice to stop it.</span>
                    <button className="tc-option is-sm is-primary" onClick={() => setStopped(true)}>Stop</button>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/** 16: the first time, Oby explains; macOS asks next, for agent-desktop. */
export function DesktopPermission() {
  const [answer, setAnswer] = useState<'continue' | 'not-now' | null>(null);
  return (
    <Window width={1280} height={800}>
      <div className="home">
        <div className="bar is-bare"><span className="crumb is-here">Release checklist</span></div>
        <div className="chat-layer">
          <Thread>
            <You time="2:30 PM">Add the invite emails to my checklist in TextEdit.</You>
            <div role="listitem" className="msg is-oby" style={{ marginTop: 16 }}><div className="msg-task-work">
              <TaskCard agent="claude" title="Update the checklist" meta="Needs you · Claude Code · on your Mac" dot="needs" />
              <Log><LogRow mark="needs">Asked to use your Mac</LogRow></Log>
            </div></div>
          </Thread>
          <Composer usage={false} />
        </div>
      </div>
      {!answer && (
        <div className="scrim" data-lab="scrim">
          <div className="modal" role="dialog" aria-labelledby="perm-title" style={{ animation: 'none' }}>
            <h2 className="modal-title" id="perm-title">Let agents use your Mac?</h2>
            <p className="modal-text">Claude Code wants to type in TextEdit. Agents use other apps through agent-desktop, the same way you do.</p>
            <ul className="modal-list">
              <li><Monitor size={16} strokeWidth={1.6} /><span><b>It sees the screen</b> while it works, to know where to click.</span></li>
              <li><MousePointer2 size={16} strokeWidth={1.6} /><span><b>It clicks and types</b> in other apps. You see a frame around the screen while it does.</span></li>
              <li><Hand size={16} strokeWidth={1.6} /><span><b>You stop it any time:</b> press Esc twice, or Stop.</span></li>
            </ul>
            <div className="steps"><b>Next, macOS asks you, once.</b> Allow agent-desktop in System Settings › Privacy &amp; Security, under Accessibility and then Screen Recording. Oby never asks macOS for these itself.</div>
            <div className="modal-actions"><button className="set-button" onClick={() => setAnswer('not-now')}>Not now</button><button className="primary-btn" onClick={() => setAnswer('continue')}>Continue</button></div>
          </div>
        </div>
      )}
      <span hidden data-lab="answer">{answer ?? ''}</span>
    </Window>
  );
}
