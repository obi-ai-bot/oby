// The pieces every board is made of, on the app's own classes (app.css, liveTask.css) and its own
// logos and face (brands.tsx, ObyFace.tsx, read from apps/electron), so each can move into the app
// as a file. Words come from the boards (design/boards/*.dc.html) exactly.
import { Children, isValidElement, type CSSProperties, type ReactElement, type ReactNode } from 'react';
import {
  ArrowUp, Check, ChevronDown, EllipsisVertical, Folder, Gauge, Globe, PanelLeft, Plus, ShieldCheck, X,
} from 'lucide-react';
import { BRANDS, BrandMark } from '@app/brands';
import { ObyFace } from '@app/ObyFace';
import { AGENT_NAME, type AgentId } from './api/types';

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

/** The window a board shows: its size, the traffic lights, and the panel icon. */
export function Window({ width, height, sidebar, children, className = '' }: { width: number; height: number; sidebar?: number; children: ReactNode; className?: string }) {
  // --column as tokens.css computes it from the window's width (it reads 100vw there)
  const column = Math.min(768, Math.max(540, 540 + (width - 994) * 0.6));
  // sidebar: the side panel open at the width the board shows (the owner can drag it)
  const style = { width, height, '--column': `${column}px`, ...(sidebar ? { '--sidebar': `${sidebar}px` } : {}) } as CSSProperties;
  return (
    <div className={`app lab-win${sidebar ? ' has-sidebar' : ''} ${className}`.trim()} style={style} data-lab-window>
      <span className="lab-lights" aria-hidden="true"><i /><i /><i /></span>
      <button className="icon-button panel-toggle" aria-label={sidebar ? 'Hide sidebar' : 'Show sidebar'}><PanelLeft size={18} strokeWidth={1.6} /></button>
      {children}
    </div>
  );
}

/** The title bar: the conversation's name, its project, ⋮ and what sits at its end. */
export function Bar({ crumb, project, machine, bare = true, end, crumbStyle }: { crumb: string; project?: string; machine?: ReactNode; bare?: boolean; end?: ReactNode; crumbStyle?: CSSProperties }) {
  return (
    <div className={`bar${bare ? ' is-bare' : ''}`}>
      <span className="crumb is-here" style={crumbStyle}>{crumb}</span>
      {project && <button className="project-chip"><Folder size={14} strokeWidth={1.6} />{project}<ChevronDown size={12} strokeWidth={1.8} /></button>}
      {machine}
      {project && <button className="icon-button bar-more" aria-label="Conversation options"><EllipsisVertical size={16} strokeWidth={1.6} /></button>}
      {end && <div className="bar-end">{end}</div>}
    </div>
  );
}

/** The globe at the bar's end: lit while the browser shows; a blue dot while an agent uses a tab. */
export function Globe17({ on, badge, label, onClick, style }: { on?: boolean; badge?: boolean; label: string; onClick?: () => void; style?: CSSProperties }) {
  return (
    <button className={`icon-button${on ? ' is-on' : ''}`} aria-label={label} aria-pressed={on ? true : undefined} onClick={onClick} style={style} data-lab="globe">
      <Globe size={17} strokeWidth={1.6} />
      {badge && <span className="badge" />}
    </button>
  );
}

export function Thread({ children }: { children: ReactNode }) {
  return (
    <div className="chat-scroll"><section className="chat"><div className="chat-column">
      <div role="list" className="chat-thread">{children}</div>
    </div></section></div>
  );
}

export function You({ children, time }: { children: ReactNode; time: string }) {
  return <div role="listitem" className="msg is-you"><div className="msg-end"><p className="msg-bubble">{children}</p><span className="msg-time">{time}</span></div></div>;
}

export function Oby({ children, time, style }: { children: ReactNode; time: string; style?: CSSProperties }) {
  return (
    <div role="listitem" className="msg is-oby" style={style}>
      <span className="msg-face"><ObyFace size={22} /></span>
      <div className="msg-end"><div className="msg-bubble"><p>{children}</p></div><span className="msg-time">{time}</span></div>
    </div>
  );
}

export const Code = ({ children }: { children: ReactNode }) => <span className="tok-code">{children}</span>;

export type Dot = 'working' | 'needs' | 'held' | 'done';
export function Status({ dot, size }: { dot: Dot; size?: number }) {
  if (dot === 'done') return <span className="status"><Check size={12} strokeWidth={2} /></span>;
  const s = size ? { width: size, height: size } : undefined;
  return <span className="status" role="img" aria-label={dot === 'working' ? 'Working' : dot === 'needs' ? 'Needs you' : 'Held'}><span className={`status-dot is-${dot}`} style={s} /></span>;
}

/** A task's card in the conversation (TaskCard's .msg-task), with its progress bars. */
export function TaskCard({ agent, title, meta, progress, dot }: { agent: AgentId; title: string; meta: string; progress?: ('done' | 'current' | 'needs' | '')[]; dot: Dot }) {
  return (
    <div className="msg-task" data-lab="task">
      <Logo agent={agent} size={16} />
      <span className="msg-task-text">
        <span className="msg-task-title">{title}</span>
        <span className="msg-task-meta">{meta}</span>
        {progress && <span className="tc-progress msg-task-stages" role="img">{progress.map((p, i) => <span key={i} className={p ? `is-${p}` : undefined} />)}</span>}
      </span>
      <Status dot={dot} />
    </div>
  );
}

/** One row of what it did (TaskLog): its mark, its words, its time. */
export function LogRow({ mark, children, meta, refused, after }: { mark: 'check' | 'working' | 'needs' | 'x' | ReactNode; children: ReactNode; meta?: string; refused?: boolean; after?: ReactNode }) {
  const m = mark === 'check' ? <Check size={12} strokeWidth={2} />
    : mark === 'x' ? <X size={12} strokeWidth={2} />
    : mark === 'working' || mark === 'needs' ? <span className={`status-dot is-${mark}`} style={{ width: 6, height: 6 }} />
    : mark;
  return (
    <li className={`ss-tool${refused ? ' is-refused' : ''}`}>
      <div className="ss-tool-row">
        <span className="ss-tool-mark">{m}</span>
        <span className="ss-tool-text" style={after ? { flex: '0 1 auto' } : undefined}>{children}</span>
        {meta && <span className="ss-tool-meta">{meta}</span>}
        {after}
      </div>
    </li>
  );
}

/** What it did, under its card (TaskLog): its rows in the app's list (.ss-entries), the lines
 * between them (answered, waiting) outside it, as TaskLog puts them. */
export function Log({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  const out: ReactNode[] = [];
  let rows: ReactElement[] = [];
  const flush = () => { if (rows.length) out.push(<ol key={`ol${out.length}`} className="ss-entries">{rows}</ol>); rows = []; };
  Children.toArray(children).forEach((c) => {
    if (isValidElement(c) && (c.type === LogRow || (c.props as { 'data-row'?: boolean })['data-row'])) rows.push(c);
    else { flush(); out.push(c); }
  });
  flush();
  return <div className="lt-log" role="group" style={style}>{out}</div>;
}

export const Sheen = ({ children }: { children: ReactNode }) => <span className="sheen">{children}</span>;

/** The message box (Composer), as a real install shows it. */
export function Composer({ chips = false, usage = true, placeholder = 'Reply to Oby…' }: { chips?: boolean; usage?: boolean; placeholder?: string }) {
  return (
    <form className="composer chat-composer" onSubmit={(e) => e.preventDefault()}><div className="composer-field"><div className="composer-grid">
      <button type="button" className="composer-button" aria-label="Add to the message"><Plus size={18} strokeWidth={1.6} /></button>
      <div className="composer-editor"><div className="composer-input" contentEditable suppressContentEditableWarning aria-label={placeholder.replace('…', '')} role="textbox" /><div className="composer-placeholder">{placeholder}</div></div>
      {chips && (
        <div className="composer-talk">
          <span className="talk-chip"><ShieldCheck size={15} strokeWidth={1.6} />Default<ChevronDown size={13} strokeWidth={1.8} /></span>
          <span className="talk-chip is-ink">Auto<ChevronDown size={13} strokeWidth={1.8} /></span>
        </div>
      )}
      {usage && <button type="button" className="composer-button composer-usage" aria-label="Usage"><Gauge size={18} strokeWidth={1.6} /></button>}
      <button type="submit" className="composer-button composer-send is-always" aria-label="Send"><ArrowUp size={18} strokeWidth={1.8} /></button>
    </div></div></form>
  );
}
