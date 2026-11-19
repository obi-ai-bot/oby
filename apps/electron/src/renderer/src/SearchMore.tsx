import { FileText, Plus, Settings } from 'lucide-react';
import type { PaletteItem } from './search';

// What ⌘K finds besides conversations (docs/contracts/shortcuts.md, SHORTCUTS-11): Settings pages and
// a few actions, each group under its own heading below the conversations, as T3 Code's palette
// shows Threads, Settings and Actions. Rows as the side panel's own New conversation.
const ICONS: Record<string, typeof Plus> = { 'chat.new': Plus, 'settings.open': Settings, 'reveal-log': FileText };

export default function SearchMore({ settings, actions, onPick }: {
  settings: PaletteItem[];
  actions: PaletteItem[];
  onPick: (kind: 'setting' | 'action', id: string) => void;
}) {
  const row = (kind: 'setting' | 'action', item: PaletteItem) => {
    const Icon = kind === 'setting' ? Settings : ICONS[item.id] ?? Settings;
    return (
      <button key={item.id} className="sidebar-new chat-new search-pick" data-pick={`${kind}:${item.id}`} onClick={() => onPick(kind, item.id)}>
        <Icon size={16} strokeWidth={1.6} />
        <span className="sidebar-new-label">{item.label}</span>
      </button>
    );
  };
  return (
    <>
      {settings.length > 0 && <h3 className="sidebar-convo-head">Settings</h3>}
      {settings.map((p) => row('setting', p))}
      {actions.length > 0 && <h3 className="sidebar-convo-head">Actions</h3>}
      {actions.map((a) => row('action', a))}
    </>
  );
}
