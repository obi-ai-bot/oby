// The browser's small pieces, on the app's own classes and logos (brands.tsx). Moved from the lab
// (tools/obid-spec/lab-ui/browser/src/pieces.tsx): only the pieces the app had none of; the lab's
// copies of the app's own (the message box, a task's card, its log) stayed behind.
import type { CSSProperties, ReactNode } from 'react';
import { Check, Globe } from 'lucide-react';
import { BRANDS, BrandMark } from '../brands';
import { AGENT_NAME, type AgentId } from './view';

const BRAND: Record<AgentId, keyof typeof BRANDS> = { codex: 'openai', claude: 'claude', cursor: 'cursor' };

/** An agent's logo, as the app draws it (BrandMark); `alt` names it where the board does. */
export function Logo({ agent, size = 16, alt }: { agent: AgentId; size?: number; alt?: string }) {
  return (
    <span className="logo" role={alt ? 'img' : undefined} aria-label={alt} style={{ width: size, height: size }}>
      <BrandMark brand={BRANDS[BRAND[agent]]} size={size} />
    </span>
  );
}
export const agentName = (a: AgentId) => AGENT_NAME[a];

/** The globe at the bar's end: lit while the browser shows; a blue dot while an agent uses a tab. */
export function Globe17({ on, badge, label, onClick, style }: { on?: boolean; badge?: boolean; label: string; onClick?: () => void; style?: CSSProperties }) {
  return (
    <button className={`icon-button${on ? ' is-on' : ''}`} aria-label={label} aria-pressed={on ? true : undefined} onClick={onClick} style={style} data-browser="globe">
      <Globe size={17} strokeWidth={1.6} />
      {badge && <span className="badge" />}
    </button>
  );
}

export const Sheen = ({ children }: { children: ReactNode }) => <span className="sheen">{children}</span>;

export type Dot = 'working' | 'needs' | 'held' | 'done';
export function Status({ dot, size }: { dot: Dot; size?: number }) {
  if (dot === 'done') return <span className="status"><Check size={12} strokeWidth={2} /></span>;
  const s = size ? { width: size, height: size } : undefined;
  return <span className="status" role="img" aria-label={dot === 'working' ? 'Working' : dot === 'needs' ? 'Needs you' : 'Held'}><span className={`status-dot is-${dot}`} style={s} /></span>;
}
