// Settings › General's Browser group (docs/contracts/browser-app.md, BROWSERAPP-18; the design's board
// 13): two switches, both on by default, obid's (GET and PUT /v1/browser/settings), each turned with
// the owner's own click, signed by the switch's own label (shared/words.ts). Shown where the browser
// is: on obid once it answers GET /v1/browser, and on the demo data.
import type { CSSProperties } from 'react';
import { BROWSER_SWITCHES } from '../../../shared/words';
import { useBrowser } from './store';

export function BrowserSettings({ i }: { i: number }) {
  const { offered, s, api } = useBrowser();
  if (!offered) return null;
  const sw = (key: keyof typeof BROWSER_SWITCHES, about: string) => {
    const on = s.settings[key];
    const label = BROWSER_SWITCHES[key];
    return (
      <div className="set-row">
        <span className="set-label">{label}<span className="set-about">{about}</span></span>
        <span className="set-control"><button role="switch" aria-checked={on} aria-label={label} className="set-switch" onClick={() => void api.setSettings({ [key]: !on })} /></span>
      </div>
    );
  };
  return (
    <section className="set-group rise is-new-group" style={{ '--i': i } as CSSProperties} data-browser="settings">
      <h2 className="set-head">Browser<span className="new-tag">New</span></h2>
      <div className="set-card">
        {sw('agentsUseBrowser', 'They open pages, click and type; you watch beside the conversation and take control any time.')}
        {sw('agentsUseMac', 'Other apps too, through agent-desktop. macOS asks you once.')}
      </div>
      <p className="set-foot">A password you type in Oby goes straight to the page. No agent sees it.</p>
    </section>
  );
}
