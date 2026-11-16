import { useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import { ObyFace } from './ObyFace';
import { ago } from './view';
import { lastAgo, useChats, type Chat } from './chats';
import { askCardLine } from './peers';
import { plain } from './Said';
import { loadHelpers, useHelpers } from './helpers';
import AgentSheet from './AgentSheet';
import type { Helper } from './obid';

// a conversation's line under its name, on Projects (Sidebar) and Agents: the tasks it started, else
// Oby's last words; idle: what it says with none yet (an agent's row: nothing)
export const convoAbout = (c: Chat, idle = 'Oby is on it') => {
  if (!c.said.length && c.about !== undefined) return c.about;
  const said = [...c.said].reverse().find((m) => m.who === 'oby' && (m.text || m.askCard));
  const n = c.said.flatMap((m) => m.jobs ?? []).length;
  // its tasks' count while nothing newer was said; once one reports, what it said
  if (n && !said?.text) return `Started ${n} task${n === 1 ? '' : 's'}`;
  // a question's card: what it says, which is all its turns are now
  if (said?.askCard) return askCardLine(said.askCard, Date.now()) ?? said.askCard.question;
  return said ? plain(said.text) : idle;
};

// One agent's row: its helper, its conversation (none while obid hasn't listed it), and its marks
export type AgentRow = { helper: Helper; chat?: Chat; needs: boolean; working: boolean };

// The agents under Agents (docs/contracts/helpers.md, HELPERS-13): the open profile's helpers (every
// one when the panel has no profile list), those that need you first, then the rest; each group most
// recently moved first, one whose conversation isn't there last, in obid's order. A row needs you or
// works as a conversation's row does: what it asks by itself, or its tasks (needing, working)
export function agentRows(helpers: Helper[], chats: Chat[], needing: ReadonlySet<string>, working: ReadonlySet<string>, profile?: string) {
  const rows: AgentRow[] = helpers.filter((h) => profile === undefined || h.profile === profile).map((helper) => {
    const chat = chats.find((c) => c.id === helper.conversation);
    return { helper, chat, needs: !!chat && (!!chat.needs || needing.has(chat.id)), working: !!chat && (!!chat.typing || working.has(chat.id)) };
  });
  const moved = (r: AgentRow) => (r.chat ? lastAgo(r.chat) : Infinity);
  const order = (a: AgentRow, b: AgentRow) => (moved(a) === moved(b) ? 0 : moved(a) < moved(b) ? -1 : 1);
  return { needs: rows.filter((r) => r.needs).sort(order), rest: rows.filter((r) => !r.needs).sort(order) };
}

// Agents, beside Projects in the side panel (screen 08): New agent (its sheet, AgentSheet), then
// Needs you and Agents. A row opens the agent's conversation; it has no menu
export default function AgentsList({ needing, working, profile, activeTalk, onOpenTalk }: {
  needing: ReadonlySet<string>;
  working: ReadonlySet<string>;
  profile?: string;  // the open profile's id, when the panel has a profile list
  activeTalk: string | null;
  onOpenTalk: (id: string) => void;
}) {
  const helpers = useHelpers();
  const chats = useChats();
  // obid's list again each time Agents is shown
  useEffect(() => { void loadHelpers(); }, []);
  const [sheet, setSheet] = useState(false);
  // a new agent's conversation opens once it's among the app's chats (obid's conversation event)
  const [opening, setOpening] = useState<string | null>(null);
  useEffect(() => {
    if (!opening || !chats.some((c) => c.id === opening)) return;
    onOpenTalk(opening);
    setOpening(null);
  }, [opening, chats]);
  const { needs, rest } = agentRows(helpers, chats, needing, working, profile);

  const row = ({ helper: h, chat, needs: waits, working: busy }: AgentRow) => {
    const on = activeTalk === h.conversation;
    // what it's on while a turn runs, else its last words; never the stand-in, nor what isn't loaded
    const line = chat ? (chat.typing && chat.step ? chat.step : convoAbout(chat, '')) : '';
    return (
      <div
        key={h.id}
        role="button"
        tabIndex={0}
        className={`connector task agent-row${on ? ' is-active' : ''}`}
        aria-current={on || undefined}
        onClick={() => onOpenTalk(h.conversation)}
        onKeyDown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) { e.preventDefault(); e.currentTarget.click(); } }}
      >
        <ObyFace size={24} color={h.color} />
        <span className="connector-text">
          <span className="connector-name">{h.name}</span>
          <span className="connector-about">{line}</span>
        </span>
        {waits ? <span className="status-dot is-needs" role="img" aria-label="Needs you" />
          : busy ? <span className="status-dot is-working" role="img" aria-label="Working" />
          : chat && <span className="sidebar-item-ago">{ago(lastAgo(chat))}</span>}
      </div>
    );
  };

  return (
    <div className="sidebar-plain" role="tabpanel" id="panel-side-panel" aria-labelledby="panel-side-agents">
      <button className="sidebar-new" aria-haspopup="dialog" onClick={() => setSheet(true)}>
        <Plus size={16} strokeWidth={1.6} />
        <span className="sidebar-new-label">New agent</span>
      </button>
      {needs.length > 0 && <h3 className="sidebar-convo-head">Needs you<span className="sidebar-convo-count">{needs.length}</span></h3>}
      {needs.map(row)}
      {rest.length > 0 && <h3 className="sidebar-convo-head">Agents</h3>}
      {rest.map(row)}
      {!needs.length && !rest.length && <p className="sidebar-empty">No agents yet</p>}
      {sheet && (
        <AgentSheet
          profile={profile}
          taken={helpers.filter((h) => profile === undefined || h.profile === profile).map((h) => h.color.toLowerCase())}
          onClose={() => setSheet(false)}
          onMade={(h) => { setSheet(false); setOpening(h.conversation); }}
        />
      )}
    </div>
  );
}
