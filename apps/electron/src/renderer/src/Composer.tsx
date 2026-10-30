import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ArrowUp, Check, ChevronLeft, File, FolderGit2, FolderOpen, Gauge, Laptop, ListChecks, Plus, Search, Settings, X } from 'lucide-react';
import { ObyIcon, OwnerAvatar } from './ObyFace';
import { AGENTS, OBYS, PLACES, SHARED_WITH_ME, type Attachments } from './mock';

import MessageEditor, { type EditorHandle, type MentionKey } from './MessageEditor';
import { mentioned } from './textTokens';
import StatusDot from './StatusDot';
import { useClicksEverywhere } from './Menu';
import { useTasks, type Task } from './tasks';
import { BrandMark } from './brands';
import { fullness, useAccounts, weekWords } from './SettingsView';
import { lookAtUsage, useRepos } from './live';
import { useScrollThumb } from './useScrollThumb';
import { usePrompt } from './prompts';
import { leave, useGlide } from './glide';
import { folderName, folderPlace, foldersOf, pickFolders, setFolders, useFolders, usedFolders, useRecentFolders } from './folders';
import { everything } from './lean';
import { keeper } from './drafts';

// What a message carries besides its words: files or photos (a photo with a
// small preview of itself), tasks attached. (Other Obys are brought in by name,
// in the words: @Wellington.) place: the Mac it runs on, when not this one
// (one at a time: picking another replaces it). repo: the repository the work is
// in (obid's), one at a time too.
type Attached = { kind: 'file'; name: string; preview?: string; file?: File } | { kind: 'task'; id: string } | { kind: 'place'; name: string } | { kind: 'repo'; name: string };
type View = 'root' | 'oby' | 'task' | 'where' | 'repo' | 'folder' | 'usage';
const same = (a: Attached, b: Attached) =>
  a.kind === 'task' ? b.kind === 'task' && a.id === b.id : a.kind === b.kind && a.name === (b as { name: string }).name;
const keyOf = (a: Attached) => (a.kind === 'task' ? `task:${a.id}` : `${a.kind}:${a.name}`);

// The message box: Home's, and the one at the foot of a conversation (or a
// task's or an Oby's page): tags for what's attached over one row of +, the
// text and send; once the text runs past one line, the text over a row of +
// and send instead (until it's sent). At the end, voice while the box is
// empty and send in its place once there's something to send; before them,
// usage (a gauge: how much of the agents' accounts' 5-hour session and week
// is used). In v1 (no voice) send is there all along, dim till there's
// something to send, and before the gauge, which keeps the end. The +
// opens a panel as wide as the box:
// add files or photos (straight to the file browser), add other Obys, attach
// tasks; the last two turn the panel into a search over the Obys or your
// tasks. A task picked goes in the box as a tag (x takes it out), as does each
// file; an Oby goes in the words, as @ and its name, as if typed, which is the
// other way to bring one in: an @ typed at the start of a word opens a list of
// the Obys over the box, narrowing as you type (arrows move, Enter or Tab
// picks, Esc closes it). The text grows with what you type; the up arrow sends (lit once
// there's something to send), as does Enter; Shift+Enter starts a new line.
// Words typed and not sent stay (drafts.ts): back in the box, the cursor at
// their end, when it shows again, after a relaunch too.
export default function Composer({ placeholder, prompts = false, note, label, onSend, home = false, hidden = false, autoFocus = !home, className = '', style, repos: withRepos = false, conversation, draft }: {
  placeholder?: string;
  prompts?: boolean;  // no placeholder of its own: one of Oby's prompts, changing now and then (prompts.ts)
  note?: React.ReactNode;  // a line just over the box (Home's Settings tip), coming and going with it
  label: string;
  onSend: (text: string, attached: Attachments) => void;  // its words, and what went with them
  home?: boolean;    // Home's: 480px wide, steps aside (hidden) while a page is open over it
  // words to Oby (Home's, a conversation's): the repository the work is in, as a tag, the
  // conversation's last one until another is picked or it's taken out (+ › Choose a repository)
  repos?: boolean;
  conversation?: string;
  draft?: string;    // the box its unsent words are kept as (drafts.ts, box); none: not kept
  hidden?: boolean;
  autoFocus?: boolean;  // focused on opening (a conversation's is; Home's and a task page's aren't)
  className?: string;  // a page's own spacing around it
  style?: React.CSSProperties;
}) {
  const prompt = usePrompt(prompts, hidden);
  const [filled, setFilled] = useState(false);
  // One row (+, text, send) until the text no longer fits on one line; from
  // then on, stacked (the text over a row with + and send) until it's sent,
  // however much of it you delete meanwhile
  const [stacked, setStacked] = useState(false);
  const [view, setView] = useState<View | null>(null);
  // the + panel just opened (its first 380ms, its last row's 175ms + 200ms): only then do its rows come in one by one; moving
  // between its menu and a list in it, they're simply there (before paint, so nothing flashes)
  const [opening, setOpening] = useState(false);
  const wasOpen = useRef(false), openingTimer = useRef(0);
  useLayoutEffect(() => {
    const open = view !== null && view !== 'usage';
    if (open && !wasOpen.current) {
      setOpening(true);
      clearTimeout(openingTimer.current);
      openingTimer.current = window.setTimeout(() => setOpening(false), 380);
    }
    wasOpen.current = open;
  }, [view]);
  const [attached, setAttached] = useState<Attached[]>([]);
  const { repos, last } = useRepos(conversation);
  // undefined: the conversation's last; null: none (taken out)
  const [repoPick, setRepoPick] = useState<string | null>();
  // the repository: the design's (?everything) only; v1 has working folders in its place
  const repo = everything && withRepos && repos.length ? (repoPick === undefined ? last : repoPick ?? undefined) : undefined;
  // the working folders (v1, where words to Oby go: Home's and a conversation's): a conversation's
  // are kept with it (folders.ts); Home's go with the words, to the conversation they start
  const canFolders = !everything && withRepos && !!window.app?.pickFolders;
  const kept = useFolders(conversation);
  const [homeFolders, setHomeFolders] = useState<string[]>([]);
  const folders = !canFolders ? [] : conversation ? kept : homeFolders;
  // from the list as it is when it's applied (a tag's × applies after its 140ms leave: two quick ×s
  // mustn't bring the first back)
  const putFolders = (next: (was: string[]) => string[]) => (conversation ? setFolders(conversation, next(foldersOf(conversation))) : setHomeFolders(next));
  const recentFolders = useRecentFolders();
  const chooseFolders = async () => {
    const picked = await pickFolders();
    if (picked.length) putFolders((was) => [...new Set([...was, ...picked])]);
  };
  // one from the recent ones: on the message, or off it again (the list keeps its order while it's
  // open: the folders sent go to its top, with the message)
  const toggleFolder = (f: string) => putFolders((was) => (was.includes(f) ? was.filter((x) => x !== f) : [...was, f]));
  // the Obys named in the words so far
  const [named, setNamed] = useState<string[]>([]);
  // an @ being typed: what's after it (null: none), the Oby lit in the list,
  // and whether Esc put the list away (until the @ is left)
  const [at, setAt] = useState<string | null>(null);
  const [lit, setLit] = useState(0);
  const [atClosed, setAtClosed] = useState(false);
  // a name typed out in full is done: the list goes, and Enter sends
  const fullName = at !== null && OBYS.some((o) => o.name.toLowerCase() === at.toLowerCase());
  const matches = at === null || atClosed || view || fullName ? [] : obysFor(at);
  useClicksEverywhere(matches.length > 0);
  const pickOby = (name: string) => { setView(null); box.current?.mention(name); box.current?.focus(); };
  const onMentionKey = (key: MentionKey) => {
    if (!matches.length) return false;
    if (key === 'close') setAtClosed(true);
    else if (key === 'pick') pickOby(matches[Math.min(lit, matches.length - 1)].name);
    else setLit((i) => (Math.min(i, matches.length - 1) + (key === 'down' ? 1 : matches.length - 1)) % matches.length);
    return true;
  };
  const files = useRef<HTMLInputElement>(null);
  // the buttons that open the panels; a panel put away by its button or Esc
  // hands the focus back to its button
  const plus = useRef<HTMLButtonElement>(null);
  const gauge = useRef<HTMLButtonElement>(null);
  const closePanel = () => {
    if (field.current?.querySelector('.add-panel')?.contains(document.activeElement)) (view === 'usage' ? gauge : plus).current?.focus();
    setView(null);
  };
  // Tab or Shift+Tab out of a panel puts it away (a click inside it, or another
  // app taking the focus, doesn't)
  const onPanelBlur = (e: React.FocusEvent) => {
    if (e.relatedTarget && !e.currentTarget.contains(e.relatedTarget as Node)) setView(null);
  };
  const field = useRef<HTMLDivElement>(null);
  const box = useRef<EditorHandle>(null);
  const form = useRef<HTMLFormElement>(null);
  const tasks = useTasks();
  // Past 3 rows the tags scroll: kept at the bottom, so what you just added
  // shows, and faded at the top while older ones are hidden above
  const tagRow = useRef<HTMLDivElement>(null);
  // tags coming and going: the rest glide to their places (glide.ts)
  const tagSig = [...folders.map((f) => `folder:${f}`), repo ? 'repo' : '', ...attached.map(keyOf)].join('|');
  useGlide(tagRow, tagSig);
  const [hiddenAbove, setHiddenAbove] = useState(false);
  // on any tag coming or going (gone, the row's fade goes with it)
  useLayoutEffect(() => {
    const t = tagRow.current;
    if (!t) { setHiddenAbove(false); return; }
    t.scrollTop = t.scrollHeight;
    setHiddenAbove(t.scrollTop > 0);
  }, [tagSig]);
  const add = (...picked: Attached[]) => {
    const r = picked.find((a) => a.kind === 'repo');
    if (r) { setRepoPick(r.name); picked = picked.filter((a) => a !== r); }
    const place = picked.some((a) => a.kind === 'place');
    setAttached((all) => [...all.filter((x) => !place || x.kind !== 'place'), ...picked.filter((a) => !(a.kind === 'place' && !a.name) && !all.some((x) => same(x, a)))]);
    setView(null);
    box.current?.focus();
  };

  // files picked or pasted: each a tag, a photo with its preview
  const addFiles = (list: File[]) => {
    const picked = list.map((f): Attached => ({
      kind: 'file', name: f.name || 'Pasted image', preview: f.type.startsWith('image/') ? URL.createObjectURL(f) : undefined, file: f,
    }));
    if (picked.length) add(...picked);
  };
  // The box's draft: put back when it shows (or when it's another box's), and
  // kept as it's typed, a moment after the last keystroke; what's pending is
  // written when the box goes, or the window closes (pagehide), so a quick
  // quit loses nothing. Sending empties the box, which lets its draft go.
  const drafts = useRef<ReturnType<typeof keeper>>(null);
  drafts.current ??= keeper();
  useLayoutEffect(() => {
    const d = drafts.current!, b = box.current;
    const text = d.show(draft ?? null);
    if (b && draft !== undefined && (text || b.text())) b.setText(text);
    addEventListener('pagehide', d.flush);
    return () => { removeEventListener('pagehide', d.flush); d.flush(); };
  }, [draft]);
  // Home's box stepping aside (a page opened over it) puts its panel away
  useEffect(() => { if (hidden) setView(null); }, [hidden]);
  // a click outside the box and its panel closes the panel (anywhere in the window)
  useClicksEverywhere(!!view);
  useEffect(() => {
    if (!view) return;
    const onDown = (e: PointerEvent) => { if (!field.current?.contains(e.target as Node)) setView(null); };
    addEventListener('pointerdown', onDown, true);
    return () => removeEventListener('pointerdown', onDown, true);
  }, [view]);

  return (
    <form ref={form} className={`${home ? `composer${hidden ? ' is-hidden' : ''}` : 'composer chat-composer rise'} ${className}`.trim()} inert={hidden} style={style} onSubmit={(e) => {
      e.preventDefault();
      const b = box.current!;
      // words, or something attached, or both
      if (!b.text().trim() && !attached.length) return;
      onSend(b.text(), {
        files: attached.flatMap((a) => (a.kind === 'file' ? [{ name: a.name, preview: a.preview, file: a.file }] : [])),
        obys: mentioned(b.text()),
        jobs: attached.flatMap((a) => (a.kind === 'task' ? [a.id] : [])),
        on: attached.find((a) => a.kind === 'place')?.name,
        repo,
        ...(folders.length ? { folders } : {}),
      });
      if (folders.length) usedFolders(folders);
      // Home's go with the conversation they start; a conversation keeps its own
      if (!conversation) setHomeFolders([]);
      b.clear();
      setFilled(false);
      setStacked(false);
      setAttached([]);
      setView(null);
      // Home's: what you sent opens as its own conversation, so the box lets go
      // (the send button too, when that's what sent it). Anywhere else the box
      // keeps the focus, even when send was clicked (it hides once it's sent)
      if (home) {
        b.blur();
        const a = document.activeElement;
        if (a instanceof HTMLElement && form.current?.contains(a)) a.blur();
      } else b.focus();
    }}>
      {note}
      <div
        ref={field}
        className={`composer-field${stacked ? ' is-stacked' : ''}`}
        // A click anywhere in the box is a click in the text (the room around it,
        // between the buttons, included), not just on the text itself; its
        // buttons, tags, panel and scrollbar keep their own clicks
        onMouseDown={(e) => {
          if ((e.target as HTMLElement).closest('button, input, .composer-tags, .composer-editor, .add-panel, .scroll-thumb')) return;
          e.preventDefault();
          box.current?.focus();
        }}
        // left with nothing typed (focus gone outside the box), it goes back to one row
        onBlur={(e) => { if (!field.current?.contains(e.relatedTarget as Node) && !box.current?.text().trim()) setStacked(false); }}
      >
        {view === 'usage' && <UsagePanel onClose={closePanel} onBlur={onPanelBlur} />}
        {view && view !== 'usage' && (
          <AddPanel
            opening={opening}
            view={view}
            tasks={tasks}
            onView={(v) => (v ? setView(v) : closePanel())}
            onBlur={onPanelBlur}
            onFiles={() => { closePanel(); files.current?.click(); }}
            // the first time, straight to the Mac's picker; after, the recent ones (and Choose another)
            onFolders={canFolders ? () => { if (recentFolders.length) setView('folder'); else { closePanel(); void chooseFolders(); } } : undefined}
            folders={{ recent: recentFolders, picked: folders, onToggle: toggleFolder, onChoose: () => { closePanel(); void chooseFolders(); } }}
            onPick={add}
            onPickOby={pickOby}
            repos={everything && withRepos ? repos : []}
            isPicked={(a) => (a.kind === 'repo' ? a.name === repo : attached.some((x) => same(x, a)))}
            isNamed={(name) => named.includes(name)}
          />
        )}
        {matches.length > 0 && (
          <div className="add-panel mention-panel" role="listbox" aria-label="Obys">
            {matches.map((o, i) => (
              <button
                type="button"
                role="option"
                key={o.name}
                aria-selected={i === Math.min(lit, matches.length - 1)}
                className={i === Math.min(lit, matches.length - 1) ? 'is-lit' : ''}
                // the text keeps the focus (and its cursor)
                onMouseDown={(e) => e.preventDefault()}
                onMouseEnter={() => setLit(i)}
                onClick={() => pickOby(o.name)}
              >
                <OwnerAvatar name={o.name} size={22} />
                <span className="add-item-text">{o.name}’s Oby</span>
              </button>
            ))}
          </div>
        )}
        {/* no talking (v1): send before the usage gauge, which keeps the end */}
        <div className={`composer-grid${everything ? '' : ' is-send-first'}`}>
          {(attached.length > 0 || repo || folders.length > 0) && (
            <div ref={tagRow} className={`composer-tags${hiddenAbove ? ' is-scrolled' : ''}`} onScroll={(e) => setHiddenAbove(e.currentTarget.scrollTop > 0)}>
              {folders.map((f) => (
                <span key={f} data-glide={`folder:${f}`} className="composer-tag" title={f}>
                  <FolderOpen size={15} strokeWidth={1.7} />
                  <span className="composer-tag-text">{folderName(f)}</span>
                  <button type="button" aria-label={`Remove ${folderName(f)}`} onClick={(e) => leave(e.currentTarget.parentElement, () => putFolders((was) => was.filter((x) => x !== f)))}><X size={13} strokeWidth={2} /></button>
                </span>
              ))}
              {repo && (
                <span data-glide="repo" className="composer-tag">
                  <FolderGit2 size={15} strokeWidth={1.7} />
                  <span className="composer-tag-text">{repo}</span>
                  <button type="button" aria-label={`Remove ${repo}`} onClick={(e) => leave(e.currentTarget.parentElement, () => setRepoPick(null))}><X size={13} strokeWidth={2} /></button>
                </span>
              )}
              {attached.map((a) => {
                const t = a.kind === 'task' ? tasks.find((x) => x.id === a.id) : undefined;
                return (
                  <span key={keyOf(a)} data-glide={keyOf(a)} className="composer-tag">
                    {a.kind === 'task' && <ListChecks size={15} strokeWidth={1.7} />}
                    {a.kind === 'place' && <Laptop size={15} strokeWidth={1.7} />}
                    {a.kind === 'file' && (a.preview ? <img className="composer-tag-thumb" src={a.preview} alt="" /> : <File size={15} strokeWidth={1.7} />)}
                    <span className="composer-tag-text">{a.kind === 'task' ? t?.title : a.kind === 'place' ? `On ${a.name}` : a.name}</span>
                    <button type="button" aria-label={`Remove ${a.kind === 'task' ? t?.title ?? 'task' : a.kind === 'place' ? `On ${a.name}` : a.name}`} onClick={(e) => leave(e.currentTarget.parentElement, () => { if (a.kind === 'file' && a.preview) URL.revokeObjectURL(a.preview); setAttached((all) => all.filter((x) => !same(x, a))); })}><X size={13} strokeWidth={2} /></button>
                  </span>
                );
              })}
            </div>
          )}
          <button
            ref={plus}
            type="button"
            className={`composer-button${view && view !== 'usage' ? ' is-on' : ''}`}
            aria-label="Add to the message"
            aria-haspopup="menu"
            aria-expanded={!!view && view !== 'usage'}
            // a click leaves the focus where it is (the panel takes it on opening)
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => (view && view !== 'usage' ? closePanel() : setView('root'))}
          >
            <Plus size={18} strokeWidth={1.6} />
          </button>
          <input
            ref={files}
            type="file"
            multiple
            hidden
            onChange={(e) => {
              addFiles([...(e.currentTarget.files ?? [])]);
              e.currentTarget.value = '';  // the same file can be picked again later
            }}
          />
          <MessageEditor
            handle={box}
            autoFocus={autoFocus}
            placeholder={prompts ? prompt : placeholder ?? ''}
            label={label}
            onText={(t) => { setFilled(!!t.trim()); setNamed(mentioned(t)); drafts.current!.typed(t); }}
            // each change narrows the list, so the first is lit again
            onMention={(q) => { setAt(q); setLit(0); if (q === null) setAtClosed(false); }}
            onMentionKey={onMentionKey}
            onEnter={() => form.current?.requestSubmit()}
            // past one line (32px: 22 of text, 10 of padding) it stacks
            onHeight={(h) => setStacked((was) => was || h > 33)}
            onPasteFiles={addFiles}
          />
          {/* how much of the agents' accounts is used, session and week (UsagePanel on a click) */}
          <UsageGauge ref={gauge} open={view === 'usage'} onClick={() => (view === 'usage' ? closePanel() : setView('usage'))} />
          {/* at the end: talk instead of typing while the box is empty; once there's
              something to send, send takes its place. Talking comes back later, with Voice in
              Settings › General (only with ?everything): till then send is there all along, dim till
              there's something to send */}
          {everything && <button type="button" className={`composer-button composer-voice${filled || attached.length ? ' is-away' : ''}`} aria-label="Talk to Oby" aria-hidden={filled || attached.length > 0 || undefined} tabIndex={filled || attached.length ? -1 : 0}>
            <svg width="18" height="16" viewBox="0 0 20 18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
              <path d="M2 7v4M6 4v10M10 1v16M14 4v10M18 7v4" />
            </svg>
          </button>}
          <button type="submit" className={`composer-button composer-send${everything ? '' : ' is-always'}${filled || attached.length ? ' is-ready' : ''}`} aria-label="Send" disabled={!filled && !attached.length} aria-hidden={(everything && !filled && !attached.length) || undefined}>
            <ArrowUp size={18} strokeWidth={1.8} />
          </button>
        </div>
      </div>
    </form>
  );
}

// The +'s panel over the box, as wide as it. First the three things you can
// add; Obys and tasks then turn it into a search with a list under it (back
// at its left returns to the three). Esc steps back, then closes.
function AddPanel({ view, opening, tasks, repos, onView, onBlur, onFiles, onFolders, folders, onPick, onPickOby, isPicked, isNamed }: {
  view: View;
  opening: boolean;  // just opened: its rows come in one by one (app.css)
  onFolders?: () => void;  // the working folders: the recent ones, or the Mac's picker (none: not offered)
  folders: { recent: string[]; picked: string[]; onToggle: (f: string) => void; onChoose: () => void };
  repos: string[];   // where the work can be (obid's), none where it can't be picked
  onBlur: (e: React.FocusEvent) => void;
  tasks: Task[];
  onView: (v: View | null) => void;
  onFiles: () => void;
  onPick: (a: Attached) => void;
  onPickOby: (name: string) => void;
  isPicked: (a: Attached) => boolean;
  isNamed: (name: string) => boolean;  // already @named in the words
}) {
  const [query, setQuery] = useState('');
  useEffect(() => setQuery(''), [view]);
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLDivElement>();
  // The three take the focus on opening: the first, or, back from a search,
  // the one that opened it. Up and Down move between them; Tab leaves
  const menu = useRef<HTMLDivElement>(null);
  const was = useRef<View>(view);
  useEffect(() => {
    if (view === 'root') (menu.current?.querySelector<HTMLButtonElement>(`[data-view="${was.current}"]`) ?? menu.current?.querySelector('button'))?.focus();
    was.current = view;
  }, [view]);
  const onMenuKey = (e: React.KeyboardEvent) => {
    const items = [...menu.current!.querySelectorAll('button')];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const to = { ArrowDown: (i + 1) % items.length, ArrowUp: (i + items.length - 1) % items.length, Home: 0, End: items.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    items[to].focus();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopImmediatePropagation();
      onView(view === 'root' ? null : 'root');
    };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, [view, onView]);

  if (view === 'root') {
    return (
      <div ref={menu} className={`add-panel${opening ? ' is-opening' : ''}`} role="menu" onKeyDown={onMenuKey} onBlur={onBlur}>
        <button type="button" role="menuitem" tabIndex={-1} onClick={onFiles}><File size={16} strokeWidth={1.6} />Add files or photos</button>
        {/* no other Obys, or no Macs to run on (on obid, for now): not offered */}
        {OBYS.length > 0 && <button type="button" role="menuitem" tabIndex={-1} data-view="oby" onClick={() => onView('oby')}><ObyIcon size={16} />Add other Obys<kbd className="add-hint">@</kbd></button>}
        <button type="button" role="menuitem" tabIndex={-1} data-view="task" onClick={() => onView('task')}><ListChecks size={16} strokeWidth={1.6} />Attach tasks</button>
        {/* Sharing (a preview): run it on a teammate's Mac that's free, with your own accounts */}
        {PLACES.length > 0 && <button type="button" role="menuitem" tabIndex={-1} data-view="where" onClick={() => onView('where')}><Laptop size={16} strokeWidth={1.6} />Run on another Mac</button>}
        {repos.length > 0 && <button type="button" role="menuitem" tabIndex={-1} data-view="repo" onClick={() => onView('repo')}><FolderGit2 size={16} strokeWidth={1.6} />Choose a repository</button>}
        {/* where the work is: folders on this Mac, several (it opens the Mac's own picker) */}
        {onFolders && <button type="button" role="menuitem" tabIndex={-1} data-view="folder" onClick={onFolders}><FolderOpen size={16} strokeWidth={1.6} />Select working folder…</button>}
      </div>
    );
  }

  const q = query.trim().toLowerCase();
  const obys = obysFor(q);
  const found = tasks.filter((t) => `${t.who ?? ''} ${t.title}`.toLowerCase().includes(q));
  const places = PLACES.filter((p) => p.name.toLowerCase().includes(q));
  const inRepos = repos.filter((r) => r.toLowerCase().includes(q));
  const inFolders = folders.recent.filter((f) => f.toLowerCase().includes(q));
  // Enter takes the first match (a Mac: the first that's free)
  const pickFirst = () => {
    if (view === 'oby') return obys[0] && onPickOby(obys[0].name);
    if (view === 'where') { const p = places.find((x) => x.free); return p && onPick({ kind: 'place', name: p.name }); }
    if (view === 'repo') return inRepos[0] && onPick({ kind: 'repo', name: inRepos[0] });
    if (view === 'folder') return inFolders[0] && folders.onToggle(inFolders[0]);
    return found[0] && onPick({ kind: 'task', id: found[0].id });
  };
  const hint = { oby: 'Search Obys', task: 'Search tasks', where: 'Search Macs', repo: 'Search repositories', folder: 'Search recent folders' }[view as 'oby' | 'task' | 'where' | 'repo' | 'folder'];
  return (
    <div className={`add-panel is-search${opening ? ' is-opening' : ''}`} onBlur={onBlur}>
      <div className="add-search">
        <button type="button" className="add-back" aria-label="Back" onClick={() => onView('root')}><ChevronLeft size={16} strokeWidth={1.6} /></button>
        <Search size={14} strokeWidth={1.6} className="add-search-icon" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          // Enter takes the first match
          onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); pickFirst(); } }}
          placeholder={hint}
          aria-label={hint}
        />
      </div>
      {/* (folders: several can be on, so a list of several) */}
      <div ref={list} className={`add-list${view === 'folder' ? ' is-folders' : ''}`} role="listbox" aria-multiselectable={view === 'folder' || undefined} onScroll={onScroll}>
        {view === 'oby' && obys.map((o) => (
          <button type="button" role="option" aria-selected={isNamed(o.name)} key={o.name} onClick={() => onPickOby(o.name)}>
            <OwnerAvatar name={o.name} size={22} />
            <span className="add-item-text">{o.name}’s Oby</span>
            {isNamed(o.name) && <Check size={14} strokeWidth={1.8} className="add-check" />}
          </button>
        ))}
        {view === 'task' && found.map((t) => {
          const a: Attached = { kind: 'task', id: t.id };
          return (
            <button type="button" role="option" aria-selected={isPicked(a)} key={t.id} onClick={() => onPick(a)}>
              <StatusDot task={t} />
              <span className="add-item-text">{t.who && <span className="add-item-who">{t.who}</span>}{t.title}</span>
              {isPicked(a) && <Check size={14} strokeWidth={1.8} className="add-check" />}
            </button>
          );
        })}
        {/* this Mac, then the Macs shared with you: who's away, or when one is free */}
        {view === 'where' && places.map((p) => {
          const a: Attached = { kind: 'place', name: p.name };
          const here = p.name === 'This Mac';
          return (
            <button type="button" role="option" aria-selected={isPicked(a)} aria-disabled={!p.free || undefined} disabled={!p.free} key={p.name} className="add-place" onClick={() => onPick(here ? { kind: 'place', name: '' } : a)}>
              {p.owner ? <OwnerAvatar name={p.owner} size={22} /> : <Laptop size={16} strokeWidth={1.6} />}
              <span className="add-item-text">{p.name}<span className="add-item-about">{p.about}</span></span>
              {isPicked(a) && <Check size={14} strokeWidth={1.8} className="add-check" />}
            </button>
          );
        })}
        {view === 'repo' && inRepos.map((name) => {
          const a: Attached = { kind: 'repo', name };
          return (
            <button type="button" role="option" aria-selected={isPicked(a)} key={name} onClick={() => onPick(a)}>
              <FolderGit2 size={16} strokeWidth={1.6} />
              <span className="add-item-text">{name}</span>
              {isPicked(a) && <Check size={14} strokeWidth={1.8} className="add-check" />}
            </button>
          );
        })}
        {/* the folders picked lately: each on the message or off it (checked: on it); then any other,
            with the Mac's own picker */}
        {view === 'folder' && inFolders.map((f) => (
          <button type="button" role="option" aria-selected={folders.picked.includes(f)} key={f} title={f} className="add-folder" onClick={() => folders.onToggle(f)}>
            <FolderOpen size={16} strokeWidth={1.6} />
            <span className="add-folder-name">{folderName(f)}</span>
            <span className="add-folder-place">{folderPlace(f)}</span>
            {/* its spot kept when it's off, so the places line up */}
            <span className="add-folder-check">{folders.picked.includes(f) && <Check size={14} strokeWidth={1.8} className="add-check" />}</span>
          </button>
        ))}
        {view === 'where' && <p className="add-note">Your job runs there with your own accounts: your logins stay on this Mac.</p>}
        {/* a search that found nothing: in the middle of the list's room (nothing typed: no words) */}
        {((view === 'oby' && !obys.length) || (view === 'task' && !found.length) || (view === 'where' && !places.length) || (view === 'repo' && !inRepos.length) || (view === 'folder' && !inFolders.length)) && q && <p className="add-empty is-middle">Nothing matches “{query.trim()}”</p>}
      </div>
      <div {...thumbProps} />
      {/* any other folder, with the Mac's own picker: under the list, which keeps its height */}
      {view === 'folder' && (
        <div className="add-foot">
          <button type="button" onClick={folders.onChoose}><Plus size={16} strokeWidth={1.6} />Choose another folder…</button>
        </div>
      )}
    </div>
  );
}

// The Obys a name typed so far could be: those it starts, then those it's in
function obysFor(typed: string) {
  const q = typed.trim().toLowerCase();
  const name = (o: { name: string }) => o.name.toLowerCase();
  return [...OBYS.filter((o) => name(o).startsWith(q)), ...OBYS.filter((o) => !name(o).startsWith(q) && name(o).includes(q))];
}

// Esc while there's something typed in a message box is the box's (it would
// otherwise go back, and the words with it)
export const typingIn = (e: KeyboardEvent) => {
  const box = (e.target as Element | null)?.closest?.('.composer-input');
  return !!box?.textContent?.trim();
};

// The way to how much the accounts have used (UsagePanel): a gauge, with a dot on it while
// any account is past 80% (yellow), or 90% (red), of its 5-hour session or its week.
function UsageGauge({ ref, open, onClick }: { ref: React.Ref<HTMLButtonElement>; open: boolean; onClick: () => void }) {
  const [accounts] = useAccounts();
  const full = fullness(Math.max(0, ...accounts.flatMap((a) => [a.used ?? 0, a.session ?? 0])));
  return (
    <button
      ref={ref}
      type="button"
      className={`composer-button composer-usage${open ? ' is-on' : ''}`}
      aria-label={`Usage${full ? ', an account is nearly full' : ''}`}
      title="Usage"
      aria-haspopup="dialog"
      aria-expanded={open}
      // a click leaves the focus where it is (the panel takes it on opening)
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      <Gauge size={18} strokeWidth={1.6} />
      {full && <span className={`usage-dot ${full}`} aria-hidden="true" />}
    </button>
  );
}

// Each account's use, over the box like the + panel, as Settings › Agents has it: its agent
// and name, when its 5-hour session starts over and how much of it is used on one line, the
// session's bar under them (yellow past 80%, red past 90%), and the week in words under that
// (an account with no session figure: its week's words alone); a gear at the top right goes to
// Settings › Agents, and takes the focus on opening. Esc, a click outside or Tab out of it
// closes it.
function UsagePanel({ onClose, onBlur }: { onClose: () => void; onBlur: (e: React.FocusEvent) => void }) {
  const [accounts] = useAccounts();
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(lookAtUsage, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  return (
    <div className="add-panel usage-panel" role="dialog" aria-label="Usage" onBlur={onBlur}>
      <div className="usage-head">
        <span>Usage</span>
        <button type="button" autoFocus className="icon-button" aria-label="Manage accounts" title="Manage accounts" onClick={() => { onClose(); dispatchEvent(new CustomEvent('oby:open-setting', { detail: everything ? 'accounts' : 'agents' })); }}>
          <Settings size={15} strokeWidth={1.6} />
        </button>
      </div>
      {!accounts.length && <p className="add-empty">No accounts yet</p>}
      {accounts.map((a) => {
        const week = weekWords(a);
        return (
          <div key={a.id} className="usage-row">
            <BrandMark brand={AGENTS[a.agent].brand} size={16} mono />
            <span className="usage-text">
              <span className="usage-line">
                <span className="usage-name">{AGENTS[a.agent].name} · {a.name}</span>
                {a.session != null && <>
                  {a.sessionResets && <span className="usage-about">Resets {a.sessionResets}</span>}
                  <span className={`usage-used ${fullness(a.session)}`.trim()}>{a.session}%</span>
                </>}
              </span>
              {a.session != null && <span className="set-meter" role="meter" aria-valuenow={a.session} aria-valuemin={0} aria-valuemax={100} aria-label={`${a.session}% of the 5-hour session used`}>
                <span className={fullness(a.session)} style={{ width: `${a.session}%` }} />
              </span>}
              <span className="usage-week">{week}</span>
            </span>
          </div>
        );
      })}
      {/* Sharing (a preview, only with ?everything): a part of a teammate's usage shared with
          you; the bar is your share, filled with what you've used of it */}
      {everything && SHARED_WITH_ME.length > 0 && <p className="usage-shared">Shared with you</p>}
      {(everything ? SHARED_WITH_ME : []).map((x) => (
        <div key={`${x.owner}-${x.account}`} className="usage-row">
          <OwnerAvatar name={x.owner} size={16} badge={false} />
          <span className="usage-text">
            <span className="usage-line">
              <span className="usage-name">{x.owner}’s {x.account}</span>
              <span className="usage-about">Your {x.share}% of its month</span>
              <span className={`usage-used ${fullness((100 * x.used) / x.share)}`.trim()}>{x.share - x.used}% left</span>
            </span>
            <span className="set-meter"><span className={fullness((100 * x.used) / x.share)} style={{ width: `${(100 * x.used) / x.share}%` }} /></span>
          </span>
        </div>
      ))}
    </div>
  );
}
