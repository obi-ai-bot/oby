import { Fragment, useEffect, useRef, useState } from 'react';
import { BookOpen, Check, Mail, UserPlus, ChevronDown, ChevronRight, EllipsisVertical, Folder, FolderPlus, Frame, Plus, Search, type LucideIcon } from 'lucide-react';
// the icon strip uses Phosphor: it has outline and filled versions of each icon
import { ChatsCircleIcon, GearSixIcon, HouseIcon, ListChecksIcon, PlusIcon } from '@phosphor-icons/react';
import { BrandMark } from './brands';
import { ARCHIVE_AFTER, TYPES, archivedTasks, asksWaiting, countOf, getArchiveAfter, hasFresh, markSeen, openOf, renameTask, setArchiveAfter, tasksOf, tasksWith, toDoOf, useTasks, type Task } from './tasks';
import { OBYS } from './mock';
import { ENDED, ago } from './view';
import { ObyFace, OwnerAvatar } from './ObyFace';
import { useScrollThumb } from './useScrollThumb';
import { Menu } from './Menu';
import { SETTINGS_PAGES } from './SettingsView';
import { ConfirmModal } from './SphereSettings';
import InviteModal from './InviteModal';
import CreateSphereModal from './CreateSphereModal';
import SphereTile from './SphereTile';
import { CAN_CREATE, ME, SAMPLE, cancelInvite, invite, memberCount, revokedLine, urlOf, useSpheres, type Sphere } from './spheres';
import TaskMenu from './TaskMenu';
import ConvoMenu, { DeleteChatWindow } from './ConvoMenu';
import { latestTask, rowMark } from './chatState';
import StatusDot, { TaskMark } from './StatusDot';
import { askCardLine, memberOpens, mockConvosWith, plainId, useConvos } from './peers';
import { roomOpens, useRoom } from './room';
import { archivedChats, chatOfTask, lastAgo, latestChat, ofObid, renameChat, restartChat, useChats, type Chat } from './chats';
import { chatFirst, everything, onObid } from './lean';
import { ACTIONS, RESTART, pickGroups, tasksMatching, useObidSearch } from './search';
import { ariaKeys, composing, JUMPS, registerCommands, shortcutLabel, withShortcut } from './shortcuts';
import { adjacent, panelOrder, pinnedFirst, projectGroups as groupsOf } from './panelOrder';
import SearchMore from './SearchMore';
import { realId, say, useLive, useWaitingCount } from './live';
import { kindName, openArtifact, useArtifactsByConversation, useNewArtifacts, useOpenArtifact } from './artifacts/store';
import { has } from '../../shared/capabilities';
import { loadProfiles, openProfile, profileChats, useProfiles } from './profiles';
import type { Profile } from './obid';
import ProfileMenu, { EditProfileModal, NewProfileModal } from './ProfileMenu';
import { useProjects, useProjectsOn } from './projects';
import ProjectSheet from './ProjectSheet';
import AgentsList, { convoAbout } from './AgentsList';
import { keepKids, keptKids, kidLine, kidTitle, kidsLine, kidsOf, onRevealKids } from './nested';

import { AGENTS_SHOWN } from './panelFlags';
import { WELCOME, sampleDid, sampleOut, useSetup } from './onboarding';
import { SAMPLE_TALK } from './Welcome';
import { ComputerButton } from './computers/ComputerButton';

type Section = 'oby' | 'home' | 'conversations' | 'obysphere' | 'settings';
type StripIcon = (p: { size?: number; weight?: 'regular' | 'fill' }) => React.ReactNode;
// sections with something new get a blue dot on their icon: Home, a task not opened yet
const SECTIONS: { id: Section; label: string; icon: StripIcon }[] = chatFirst ? [
  // chat first (docs/contracts/chat-first.md): your chats; a task is a card in its conversation.
  // The Tasks list is hidden, not gone: on the demo with ?everything&chat, on obid with ?home
  { id: 'oby', label: 'Chats', icon: ChatsCircleIcon },
  ...(everything ? [{ id: 'home' as const, label: 'Tasks', icon: ListChecksIcon }] : []),
] : [
  { id: 'home', label: 'Home', icon: HouseIcon },
  // your conversations with Oby
  { id: 'conversations', label: 'Conversations', icon: ChatsCircleIcon },
];
// each section's name over it (an Obysphere's is its own)
const TITLES: Record<Exclude<Section, 'obysphere'>, string> = {
  // the chats: the open profile's name, with its menu (screen 07); Default until obid lists them, or
  // on an obid without
  oby: 'Default', home: chatFirst ? 'Tasks' : 'Home', conversations: 'Conversations', settings: 'Settings',
};

// What each tab can start, first thing under its title; and which tabs can be searched.
const NEW_ACTION: Partial<Record<Section, { label: string; icon: LucideIcon }>> = {
  home: { label: 'New task', icon: Plus },
  ...(everything ? {} : { conversations: { label: 'New conversation', icon: Plus } }),
};
const SEARCHABLE: Section[] = ['oby', 'conversations'];

// The side panel, in two columns: icons on the far left pick a section
// (chat first: Chats, then each Obysphere you're in, with Settings at the foot;
// ?home: Home and Conversations, ?everything also + and your account picture),
// the column beside them shows it. Search, on the tabs that have it, sits by the panel icon.
// It opens from the left and pushes the view over.
export default function Sidebar({ open, atHome, onClose, onNewChat, onHome, onOpenGroup, onOpenTask, onOpenAgent, onOpenChat, onOpenOby, onOpenMember, onOpenTalk, onOpenArchive, onOpenSetting, onOpenSphereSetting, onOpenKnowledge, onOpenRoom, onOpenWaiting, activeWaiting = false, activeOby = null, activeTalk = null, activeSetting = null, activeSphereSetting = null, activeKnowledge = null, active = null, width, onResize }: {
  open: boolean;
  onClose: () => void;  // the panel closes (its tab's icon again, while that tab is showing)
  onNewChat: () => void;
  onHome: () => void;  // back to Home's own page, closing whatever is open over it
  atHome: boolean;  // on Home's own page, nothing open over it
  onOpenGroup: (id: string) => void;  // chat first: a kind of work's cards (Home's chips, on Home)
  onOpenTask: (id: string) => void;  // a task's page, on its own
  onOpenAgent?: (id: string) => void;  // a nested task: its own thread, over the conversation that started it (LIVETASK-22)
  onOpenChat: (id: string) => void;  // a task's conversation, from its menu or its row's icon
  onOpenOby: (name: string) => void;  // someone else's Oby, from the mock's Obysphere: its conversations
  onOpenMember: (owner: string) => void;  // a member of the live Obysphere, by owner id: their thread
  onOpenTalk: (id: string) => void;  // a conversation with Oby, from Conversations
  onOpenArchive: () => void;  // the archived tasks, as a page
  onOpenWaiting?: () => void;  // Waiting for you, as a page
  activeWaiting?: boolean;  // Waiting for you is showing: its row is highlighted
  activeOby?: string | null;  // the Oby whose page is showing: its row is highlighted
  activeTalk?: string | null;  // the conversation with Oby that's showing: its row is highlighted
  activeSetting?: string | null;  // the Settings page that's showing: its row is highlighted
  onOpenSphereSetting: (sphere: string) => void;  // an Obysphere's settings, from its ⋮
  activeSphereSetting?: string | null;  // the Obysphere whose settings are showing
  onOpenKnowledge: (sphere: string) => void;  // an Obysphere's knowledge, from Knowledge
  activeKnowledge?: string | null;  // the Obysphere whose knowledge is showing: Knowledge is highlighted
  onOpenRoom: (sphere: string) => void;  // an Obysphere's room, from its tile
  onOpenSetting: (id: string) => void;  // a Settings page, from the account menu or the Settings list
  active?: string | null;  // the task whose card is showing on its own: its row is highlighted
  width: number;
  onResize: (width: number | null, done?: boolean) => void;  // null: back to the default
}) {
  // drag the panel's right edge to change its width
  const startResize = (e: React.PointerEvent<HTMLDivElement>) => {
    const t = e.currentTarget, x0 = e.clientX, w0 = width;
    let x = x0;
    t.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => { x = m.clientX; onResize(w0 + x - x0); };
    // a cancelled drag (the system took the pointer) ends like a release where
    // it last was, so the panel doesn't stay stuck mid-resize
    const up = () => {
      onResize(w0 + x - x0, true);
      t.removeEventListener('pointermove', move);
      t.removeEventListener('pointerup', up);
      t.removeEventListener('pointercancel', up);
    };
    t.addEventListener('pointermove', move);
    t.addEventListener('pointerup', up);
    t.addEventListener('pointercancel', up);
  };
  const tasks = useTasks();
  // every artifact, by its conversation, each a row under it (ARTIFACTSAPP-17): one read for the panel
  const artifactsBy = useArtifactsByConversation(), newArtifacts = useNewArtifacts(), openArt = useOpenArtifact();
  const convos = useConvos();
  // the open profile's (profiles.ts): on the mock its own conversations, on obid all of them
  const profiles = useProfiles();
  const withAgents = profileChats(useChats(), profiles.list, profiles.kept);
  // a helper's conversation is under Agents only (AgentsList), never among the chats; an archived one
  // is under Archived, but while something in it needs you (CHATSTATE-9)
  const waitsIn = new Set(tasksWith('needs').flatMap((t) => chatOfTask(t.id)?.id ?? []));
  const chats = withAgents.filter((c) => !c.helper && (!c.archived || c.needs || waitsIn.has(c.id)));
  const shownProfile = profiles.list?.length ? openProfile(profiles.list, profiles.kept) : undefined;
  // the title's menu (ProfileMenu), and New profile…'s form
  const [profileMenu, setProfileMenu] = useState<DOMRect | null>(null);
  const [newProfile, setNewProfile] = useState(false);
  const [editProfile, setEditProfile] = useState<Profile | null>(null);
  // Home's dot turns orange while any task needs you
  const needsYou = countOf('needs') + asksWaiting().length > 0;
  // and chat first, with Home's chips gone from sight: GitHub's work waiting on you too (a review
  // asked of you, an issue of yours), as the chips' own dots said
  const tasksNeedYou = needsYou || (chatFirst && TYPES.some((t) => toDoOf(t.id).length > 0));
  // Chat first, the dot is on what waits and always leads somewhere: a conversation's row for its
  // tasks and what it asks by itself (Chat.needs); Oby's row for what has no row to light, since the
  // empty chat it opens shows it (a task put off isn't there till its time); the Chats icon while
  // any row has one
  const needing = new Set(tasksWith('needs').flatMap((t) => chatOfTask(t.id)?.id ?? []));
  const working = new Set(tasksWith('running').flatMap((t) => chatOfTask(t.id)?.id ?? []));
  // (an agent's row is a row: what waits in its conversation lights it, and the icon)
  const noRow = tasksWith('needs').some((t) => !t.answer && !chatOfTask(t.id)) || asksWaiting().some((a) => !withAgents.some((c) => c.id === a.talk));
  const anyWaits = noRow || needing.size > 0 || withAgents.some((c) => c.needs);
  const waiting = useWaitingCount();
  const [section, setSection] = useState<Section>(SECTIONS[0].id);
  // chat first, under the title: Projects (Oby and the chats) or Agents (AgentsList); remembered.
  // Agents is hidden while AGENTS_SHOWN is false (the owner's word, 2026-11-16): Projects only
  const [side, setSide] = useState<'projects' | 'agents'>(() => {
    if (!AGENTS_SHOWN) return 'projects';
    try { return localStorage.getItem('oby.panelSide') === 'agents' ? 'agents' : 'projects'; } catch { return 'projects'; }
  });
  const pickSide = (to: 'projects' | 'agents') => {
    setSide(to);
    if (to === 'agents') { setSearching(false); setQuery(''); }
    try { localStorage.setItem('oby.panelSide', to); } catch { /* not remembered, that's all */ }
  };
  // the Obyspheres you're in (spheres.ts), under the line in the icon strip:
  // an Obysphere (a focused workspace: your Oby with other people's, sharing
  // what the org knows) by its name, under its own logo. Which one is showing
  const spheres = useSpheres();
  const room = useRoom();
  // ponytail: the one list here that knows of setting up's tour: yours, and its sample while
  // it's out (onboarding.ts), so the sample opens, and goes, as any other one does
  const tour = useSetup();
  const all = sampleOut(tour) ? [...spheres, SAMPLE] : spheres;
  // in the icon strip: in lean, not the demo's own Deskree (spheres.ts), only what you made with the +
  const strip = all.filter((x) => everything || x.id !== 'deskree');
  const [sphereId, setSphereId] = useState(() => all[0]?.id ?? '');
  // (the last one shown, while it goes: deleted or left, the panel goes back to Home)
  const lastSphere = useRef<Sphere>(all[0]);
  const sphere = all.find((x) => x.id === sphereId) ?? lastSphere.current;
  lastSphere.current = sphere;
  const inSphere = section === 'obysphere';
  const members = memberCount(sphere);
  // Knowledge lit: its page open, or in the sample, its step
  const knowing = sphere.sample ? tour.member === 'Knowledge' : activeKnowledge === sphere.id;
  // an invite's menu (⋮ or right-click on it; an admin's): send it again, or take it back.
  // resent: the one just sent again, which says so for a moment
  const [inviteMenu, setInviteMenu] = useState<{ email: string; at: DOMRect; atPointer?: boolean } | null>(null);
  const [resent, setResent] = useState<string | null>(null);
  useEffect(() => {
    if (!resent) return;
    const t = setTimeout(() => setResent(null), 2000);
    return () => clearTimeout(t);
  }, [resent]);
  // making a new one (CreateSphereModal): its modal is open
  const [creating, setCreating] = useState(false);
  const inSettings = section === 'settings';
  // a Settings page opened from elsewhere (the message box's usage gear): the panel follows
  // (chat first, back from it, the panel goes back to the chats too)
  useEffect(() => {
    if (activeSetting) setSection('settings');
    else if (chatFirst) setSection((s) => (s === 'settings' ? SECTIONS[0].id : s));
  }, [activeSetting]);
  // and an Obysphere's settings or knowledge: the panel shows that Obysphere
  useEffect(() => {
    const id = activeSphereSetting ?? activeKnowledge;
    if (!id) return;
    setSphereId(id);
    setSection('obysphere');
  }, [activeSphereSetting, activeKnowledge]);
  // and someone's Oby opened from elsewhere (a question's card): the panel shows them in their Obysphere
  useEffect(() => {
    if (!activeOby || !chatFirst) return;
    const live = all.find((x) => x.live);
    if (live) setSphereId(live.id);
    setSection('obysphere');
  }, [activeOby]);
  useEffect(() => {
    if (all.some((x) => x.id === sphereId)) return;
    if (all[0]) setSphereId(all[0].id);
    setSection((s) => (s === 'obysphere' ? SECTIONS[0].id : s));
    setLeaving(null);
  }, [all.length, sphereId]);
  const [query, setQuery] = useState('');
  const [searching, setSearching] = useState(false);
  const searchInput = useRef<HTMLInputElement>(null);
  // groups the user folded away; while searching, every group with a match opens
  const [folded, setFolded] = useState<Set<string>>(new Set());
  // groups showing all their done tasks (Show more), not just the latest few
  const [more, setMore] = useState<Set<string>>(new Set());
  // a Home task's menu: from its ⋮ (under it), or a right-click (where the pointer is)
  const [taskMenu, setTaskMenu] = useState<{ id: string; at: DOMRect; atPointer?: boolean } | null>(null);
  const menuTask = taskMenu ? tasks.find((t) => t.id === taskMenu.id) : undefined;
  // a Home task being renamed in place
  const [renamingTask, setRenamingTask] = useState<string | null>(null);
  const doneRenamingTask = (id: string, title: string) => { renameTask(id, title); setRenamingTask(null); };
  // a conversation's menu, and renaming one in place, as a task's
  const [convoMenu, setConvoMenu] = useState<{ id: string; at: DOMRect; atPointer?: boolean } | null>(null);
  const menuConvo = convoMenu ? chats.find((c) => c.id === convoMenu.id) : undefined;
  const [renamingConvo, setRenamingConvo] = useState<string | null>(null);
  // Home's ⋮: how its tasks are grouped, and when done ones get archived
  const [archiveMenu, setArchiveMenu] = useState<DOMRect | null>(null);
  // your account's menu (your picture at the icons' foot): who you are, Settings, log out
  const [accountMenu, setAccountMenu] = useState<DOMRect | null>(null);
  // the Obysphere's ⋮: which one it is, then its settings; leave it, for those
  // invited in (not its admins), through Settings' own confirmation (ConfirmModal)
  const [sphereMenu, setSphereMenu] = useState<DOMRect | null>(null);
  // the one being left from it: its confirmation is open, and goes with it
  const [leaving, setLeaving] = useState<string | null>(null);
  const leavingSphere = all.find((x) => x.id === leaving);
  // inviting people into the Obysphere (Invite member): its modal is open
  const [inviting, setInviting] = useState(false);
  const archiveAfter = getArchiveAfter();
  // Home's groups: by kind of work (the default) or by status; remembered
  // (lean: by status only, needs you, running, completed; the kinds of work came from sources
  // that aren't there yet)
  const [groupBy, setGroupBy] = useState<'type' | 'status'>(() => {
    if (!everything) return 'status';
    try { return localStorage.getItem('oby.groupBy') === 'status' ? 'status' : 'type'; } catch { return 'type'; }
  });
  const pickGroupBy = (by: 'type' | 'status') => {
    setGroupBy(by); setArchiveMenu(null);
    try { localStorage.setItem('oby.groupBy', by); } catch { /* not remembered, that's all */ }
  };
  // each group: its tasks, how many to show by its name (open ones for a kind
  // of work, all of them for a status), and how many need you
  const groups = groupBy === 'type'
    ? TYPES.map((t) => ({ id: t.id, label: t.label, tasks: tasksOf(t.id), count: openOf(t.id), countLabel: 'open', needs: countOf('needs', t.id) }))
    : ([['needs', 'Needs you'], ['running', 'Running'], ['done', 'Completed']] as const).map(([id, label]) => {
      const list = tasksWith(id);
      return { id: `status:${id}`, label, tasks: list, count: list.length, countLabel: 'tasks', needs: id === 'needs' ? list.length : 0 };
    });
  const pickArchive = (id: string) => { setArchiveAfter(id); setArchiveMenu(null); };
  // A group's done tasks: the latest few (3 in a kind of work, 5 in Done), then
  // Show more for the rest; what's still open always shows in full. Done tasks
  // leave Home for the archive after a while (Archive completed, in Home's ⋮)
  const shownOf = (g: { id: string; tasks: Task[] }) => {
    const open = g.tasks.filter((t) => t.status !== 'done'), done = g.tasks.filter((t) => t.status === 'done');
    const keep = g.id === 'status:done' ? 5 : 3;
    return { rows: more.has(g.id) ? g.tasks : [...open, ...done.slice(0, keep)], hidden: Math.max(0, done.length - keep) };
  };
  const toggleMore = (id: string) => setMore((m) => { const next = new Set(m); if (!next.delete(id)) next.add(id); return next; });
  // what Archived holds: done tasks that left Home, and chat first, the conversations put away
  const archivedCount = archivedTasks().length + (chatFirst ? archivedChats(withAgents).length : 0);
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLDivElement>();
  const pick = (id: Section) => {
    // Home again, while it's showing: back to Home's own page if something's open over it,
    // else (already there) the panel closes; any other tab again, while it's showing: the
    // panel closes, as its toggle does (chat first, the icons only ever open: its toggle closes it)
    if (id === 'home' && section === 'home' && !atHome && !chatFirst) onHome();
    else if (id === section) { if (!chatFirst) onClose(); return; }
    setSection(id); setSearching(false); setQuery('');
    if (list.current) list.current.scrollTop = 0;
  };
  // chat first, the row you're on stays in view, however you came to it (an alert, back from a Sphere)
  useEffect(() => { if (section === 'oby') list.current?.querySelector('.convo-row.is-active')?.scrollIntoView({ block: 'nearest' }); }, [activeTalk, section]);
  const [look, setLook] = useState(0);
  useEffect(() => { if (look) list.current?.querySelector('.convo-row:has(> .status-dot.is-needs)')?.scrollIntoView({ block: 'nearest' }); }, [look]);
  // In the sample: a click is the tour's (the same as its pick in the chat), with the
  // conversation brought back on top first if a page covers it; Take a look shows it here
  const tourDid = (what: string) => { onOpenTalk(WELCOME); void sampleDid(what); };
  useEffect(() => { if (tour.sphere === 'inside') { setSphereId(SAMPLE.id); pick('obysphere'); } }, [tour.sphere]);
  const q = query.trim().toLowerCase();
  const matches = (...text: string[]) => !q || text.join(' ').toLowerCase().includes(q);
  const newAction = NEW_ACTION[section];

  const toggleGroup = (name: string) => setFolded((f) => {
    const next = new Set(f);
    if (!next.delete(name)) next.add(name);
    return next;
  });
  // The Projects side's groups (docs/contracts/projects.md, PROJECTS-16 to 18): one per project, then
  // No project. Which are folded is kept (oby.projectsFolded: their ids, "" for No project), the one
  // thing the app keeps of projects
  const projectsOn = useProjectsOn();
  const projects = useProjects();
  const [projectsFolded, setProjectsFolded] = useState<Set<string>>(() => {
    try { const kept: unknown = JSON.parse(localStorage.getItem('oby.projectsFolded') ?? '[]'); return new Set(Array.isArray(kept) ? kept.filter((k) => typeof k === 'string') : []); } catch { return new Set(); }
  });
  const toggleProject = (id: string) => setProjectsFolded((f) => {
    const next = new Set(f);
    if (!next.delete(id)) next.add(id);
    try { localStorage.setItem('oby.projectsFolded', JSON.stringify([...next])); } catch { /* not remembered, that's all */ }
    return next;
  });
  const [newProject, setNewProject] = useState(false);
  // the project just made: scrolled into view once its group shows (it may be below the fold)
  const [made, setMade] = useState<string | null>(null);
  useEffect(() => {
    const group = made && list.current?.querySelector(`[data-project="${CSS.escape(made)}"]`);
    if (group) { group.scrollIntoView({ block: 'nearest' }); setMade(null); }
  });
  // scrolled down: rows going under the title fade out rather than show cut in half
  const [scrolled, setScrolled] = useState(false);
  // a project heading's menu (its ⋮, or a right-click: PROJECTS-26), and the sheet it opens on that
  // project, to change it or at the ask before deleting it
  const [projectMenu, setProjectMenu] = useState<{ id: string; at: DOMRect; atPointer?: boolean } | null>(null);
  const [projectSheet, setProjectSheet] = useState<{ id: string; deleting: boolean } | null>(null);
  const sheetProject = projects.find((p) => p.id === projectSheet?.id);
  // A conversation's tasks, nested under its row (LIVETASK-19 to 21): shown for the open conversation,
  // the one holding the open task and one whose task needs you, unless the owner folded it; what the
  // owner chose is kept (oby.kidsShown: conversation id to shown or not)
  const [kidsShown, setKidsShown] = useState<Record<string, boolean>>(keptKids);
  const showKids = (id: string, shown: boolean) => {
    const next = { ...kidsShown, [id]: shown };
    setKidsShown(next);
    keepKids(next);
  };
  // a task's own thread opened: its conversation's rows show once, even folded (revealKids, LIVETASK-22)
  useEffect(() => onRevealKids((chat) => setKidsShown((k) => { const { [chat]: _, ...rest } = k; return rest; })), []);
  const chatIdOf = (taskId: string) => chatOfTask(taskId)?.id;
  // the chats are searched, never the agents (Agents has no search)
  const searchable = SEARCHABLE.includes(section) && !(section === 'oby' && side === 'agents');
  const openSearch = () => {
    if (searching) setQuery('');
    setSearching(!searching);
  };
  // the search field takes focus as it opens
  useEffect(() => { if (searching) searchInput.current?.focus(); }, [searching]);
  // ⌘K (SHORTCUTS-2): the chats' search, from anywhere, its field taking the keys (what was typed
  // in it stays, selected)
  useEffect(() => {
    const open = () => {
      setSection(chatFirst ? 'oby' : 'conversations');
      if (chatFirst && side === 'agents') setSide('projects');
      setSearching(true);
      requestAnimationFrame(() => searchInput.current?.select());
    };
    addEventListener('oby:search', open);
    return () => removeEventListener('oby:search', open);
  }, [side]);

  // your conversations with Oby (matching the search): pinned ones first, under
  // Pinned, the rest under Recent (no headings while nothing's pinned)
  // (SEARCH-1 to 3) what's on the page as it's typed, then, under those so nothing jumps, those
  // obid's own search found the words in and those holding a task they're in
  const found = useObidSearch(searching ? query : '', onObid);
  const local = chats.filter((c) => matches(c.title, ...c.said.flatMap((m) => (m.askCard ? [m.askCard.question, m.askCard.said ?? '', askCardLine(m.askCard, Date.now()) ?? ''] : [m.text]))));
  const byTask = tasksMatching(tasks, q);
  const talks = !q ? local : [...local, ...chats.filter((c) => !local.includes(c) && (found.has(c.id) || byTask.has(c.id)))];
  const pinned = talks.filter((c) => c.pinned);
  // chat first, under your Oby: all of them, in one list
  const latest = latestChat(chats);
  const inOrder = [...pinned, ...talks.filter((c) => !c.pinned)];
  // each project's rows in that order, then those in none (panelOrder.ts)
  const projectGroups = groupsOf(inOrder, projects, projectsOn, !!q);
  // ⌘1 to ⌘9, ⌘⇧[ and ⌘⇧] (SHORTCUTS-8, 9): the conversations as the panel lists them unsearched
  // (chat first; else pinned first, as Conversations lists them), from anywhere
  const allInOrder = pinnedFirst(chats);
  const order = panelOrder(allInOrder, chatFirst ? groupsOf(allInOrder, projects, projectsOn, false) : [], projectsFolded);
  const jumping = useRef({ order, activeTalk, onOpenTalk });
  jumping.current = { order, activeTalk, onOpenTalk };
  useEffect(() => {
    const go = (id: string | null | undefined) => { if (id) jumping.current.onOpenTalk(id); };
    const step = (direction: 'previous' | 'next') => () => go(adjacent(jumping.current.order, jumping.current.activeTalk, direction));
    return registerCommands({
      ...Object.fromEntries(JUMPS.map((command, i) => [command, () => go(jumping.current.order[i])])),
      'chat.previous': step('previous'),
      'chat.next': step('next'),
    });
  }, []);
  // ⌘K finds more than conversations (SHORTCUTS-11): Settings pages and a few actions, under their
  // own headings below them; Reveal obid's Log only where main is; Restart agent only in a conversation
  // shown, once obid lists skills-v1 (COMPOSER-28)
  const { status } = useLive();
  const restartable = !!activeTalk && withAgents.some((c) => c.id === activeTalk && !c.local) && (!onObid || (has(status, 'skills-v1') && ofObid(activeTalk)));
  const finds = pickGroups(query, SETTINGS_PAGES, [...(window.app?.help ? ACTIONS : ACTIONS.filter((a) => a.id !== 'reveal-log')), ...(restartable ? [RESTART] : [])]);
  const moreFound = finds.settings.length + finds.actions.length > 0;
  const runPick = (kind: 'setting' | 'action', id: string) => {
    setSearching(false); setQuery('');
    if (kind === 'setting') { pick('settings'); onOpenSetting(id); }
    else if (id === 'chat.new') onNewChat();
    else if (id === 'settings.open') { pick('settings'); onOpenSetting('general'); }
    else if (id === 'reveal-log') void window.app?.help?.('reveal-log').then((why) => { if (why) say(why); }, say);
    else if (id === 'agent.restart' && activeTalk) void restartChat(activeTalk);
  };
  const searchMore = q ? <SearchMore settings={finds.settings} actions={finds.actions} onPick={runPick} /> : null;
  const convoRow = (c: Chat) => {
    const kids = chatFirst ? kidsOf(tasks, c.id, chatIdOf) : [];
    const arts = chatFirst ? artifactsBy.get(realId(c.id)) ?? [] : [];
    // its one mark (CHATSTATE-5): needs you, working, unread, then how its latest task ended
    const latest = chatFirst ? latestTask(tasks, c.id, chatIdOf) : undefined;
    const mark = chatFirst ? rowMark({ needs: !!c.needs || needing.has(c.id), working: working.has(c.id), unread: !!c.unread, latest }) : null;
    const nested = kids.length + arts.length > 0;
    // one of its tasks' own thread showing: its row lit, so its rows show unless the owner folded them
    // (opening that thread unfolds them once: revealKids)
    const lit = kids.some((k) => k.id === active);
    const shown = nested && (kidsShown[c.id] ?? (activeTalk === c.id || lit || kids.some((k) => k.status === 'needs')));
    return (
    <div key={c.id} className={`convo-nest${nested ? ' has-kids' : ''}`}>
    {nested && (
      <button className="convo-twist" aria-expanded={shown} aria-label={`${shown ? 'Hide' : 'Show'} ${c.title}’s agents`} onClick={() => showKids(c.id, !shown)}>
        {shown ? <ChevronDown size={10} strokeWidth={2} /> : <ChevronRight size={10} strokeWidth={2} />}
      </button>
    )}
    {/* a row with its own menu button, so the row itself isn't a <button> */}
    <div
      role="button"
      tabIndex={0}
      className={`connector task convo-row${activeTalk === c.id ? ' is-active' : ''}${convoMenu?.id === c.id ? ' is-menu-open' : ''}${c.unread ? ' is-unread' : ''}`}
      aria-current={activeTalk === c.id || undefined}
      onClick={() => onOpenTalk(c.id)}
      onKeyDown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) { e.preventDefault(); e.currentTarget.click(); } }}
      // right-click: its menu, where the pointer is
      onContextMenu={(e) => { e.preventDefault(); setConvoMenu({ id: c.id, at: new DOMRect(e.clientX, e.clientY, 0, 0), atPointer: true }); }}
    >
      <span className="connector-text">
        {renamingConvo === c.id ? (
          <input
            className="sidebar-rename"
            defaultValue={c.title}
            autoFocus
            onFocus={(e) => e.currentTarget.select()}
            onClick={(e) => e.stopPropagation()}
            onBlur={(e) => { renameChat(c.id, e.currentTarget.value); setRenamingConvo(null); }}
            onKeyDown={(e) => {
              if (composing(e)) return;
              if (e.key === 'Enter') e.currentTarget.blur();
              if (e.key === 'Escape') { e.stopPropagation(); setRenamingConvo(null); }
            }}
            aria-label="Conversation name"
          />
        ) : <span className="connector-name">{c.title}</span>}
        {/* searching: the words obid found it by, in its latest turn holding them */}
        <span className="connector-about">{(q && found.get(c.id)) || (kidsLine(kids) ?? convoAbout(c))}</span>
      </span>
      {/* when it last moved; on hover, its ⋮ in the same spot */}
      <span className="sidebar-item-ago">{c.typing ? 'now' : ago(lastAgo(c))}</span>
      {/* under it, its state as a mark: something in it needs you, a task of its is working, something
          new in it, or how its latest task ended */}
      {mark === 'needs' && <span className="status-dot is-needs" role="img" aria-label="Needs you" />}
      {mark === 'working' && <span className="status-dot is-working" role="img" aria-label="Running" />}
      {mark === 'unread' && <span className="status-dot is-new" role="img" aria-label="Unread" />}
      {mark === 'failed' && <span className="status-dot is-failed" role="img" aria-label={`Latest task: ${ENDED[latest!.job.status]}`} />}
      {mark === 'done' && <span className="convo-done" role="img" aria-label="Latest task: Done"><Check size={11} strokeWidth={2.2} /></span>}
      <button
        className="sidebar-item-more"
        aria-label={`More for ${c.title}`}
        aria-haspopup="menu"
        aria-expanded={convoMenu?.id === c.id}
        onClick={(e) => { e.stopPropagation(); setConvoMenu(convoMenu?.id === c.id ? null : { id: c.id, at: e.currentTarget.getBoundingClientRect() }); }}
      >
        <EllipsisVertical size={14} strokeWidth={1.6} />
      </button>
    </div>
    {/* its tasks, each a session of its own: its agent, its title, what it's on, its state (screen 03); a click opens its own thread, its row lit */}
    {shown && (
      <div className="convo-kids" role="group" aria-label={`${c.title}’s tasks`}>
        {kids.map((k) => (
          <button
            key={k.id}
            className={`convo-kid${active === k.id ? ' is-active' : ''}`}
            aria-current={active === k.id || undefined}
            title={kidTitle(k)}
            onClick={() => { markSeen(k.id); onOpenAgent?.(k.id); }}
            onContextMenu={(e) => { e.preventDefault(); setTaskMenu({ id: k.id, at: new DOMRect(e.clientX, e.clientY, 0, 0), atPointer: true }); }}
          >
            <BrandMark brand={k.brand} size={14} />
            <span className="convo-kid-text"><b>{k.title}</b> · {kidLine(k)}</span>
            <TaskMark task={k} />
          </button>
        ))}
        {/* its artifacts, after its tasks: a click opens it beside this conversation; New until opened (board 1a), lit while open (board 1b) */}
        {arts.map((a) => (
          <button
            key={a.id}
            className={`convo-kid art-kid${openArt?.artifact.id === a.id ? ' is-on' : ''}`}
            title={`${a.title}, made in this conversation`}
            onClick={() => { onOpenTalk(c.id); openArtifact(a); }}
          >
            <span className="art-kid-icon"><Frame size={14} strokeWidth={1.6} aria-hidden="true" /></span>
            <span className="convo-kid-text"><b>{a.title}</b> · {kindName(a.kind)}</span>
            {newArtifacts.has(a.id) && <span className="status" role="img" aria-label="New"><span className="status-dot is-new" /></span>}
          </button>
        ))}
      </div>
    )}
    </div>
    );
  };

  // An Obysphere's people, by their names
  const memberRows = (s: Sphere) => (s.sample || s.live ? s.members.map((name) => ({ name })) : OBYS.filter((o) => s.members.includes(o.name))).filter((o) => matches(o.name)).map((o) => {
    // in the sample: the tour's conversations, the one showing lit, the one it calls dotted
    const mock = mockConvosWith(s, convos, o.name);
    const n = s.sample ? (SAMPLE_TALK[o.name] ? 1 : 0) : mock.length;
    const unread = s.sample ? tour.call === o.name : mock.some((c) => c.unread);
    const lit = s.sample ? tour.member === o.name : activeOby === o.name;
    // a live member's row goes by their owner id (peers.ts), which is all obid's directory gives
    const opens = memberOpens(s), who = opens === 'thread' ? plainId(o.name) : o.name;
    return (
      <button key={o.name} className={`connector oby-row${lit ? ' is-active' : ''}`} aria-current={lit || undefined} onClick={() => (opens === 'tour' ? tourDid(o.name) : opens === 'thread' ? onOpenMember(o.name) : onOpenOby(o.name))} aria-label={`Chat with ${who}’s Oby${s.sample ? ', a sample' : ''}${unread ? ', new messages' : ''}`}>
        <OwnerAvatar name={o.name} size={22} />
        <span className="connector-text">
          <span className="connector-name">{who}’s Oby</span>
        </span>
        {/* something new from it: a blue dot, just before the count */}
        {unread && <span className={`status-dot is-new${s.sample ? ' is-calling' : ''}`} role="img" aria-label="New messages" />}
        {/* how many conversations with it, if any */}
        {n > 0 && <span className="connector-count" aria-label={`${n} conversations`}>{n}</span>}
      </button>
    );
  });

  return (
    <>
    <div className="sidebar-slot" inert={!open}>
      <nav className="sidebar" aria-label="Side panel">
        <div className="rail">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <Fragment key={id}>
              <button className={`rail-button${section === id ? ' is-on' : ''}${id === 'home' && tasks.some((t) => t.fresh) ? ' has-dot' : ''}${(id === 'home' ? tasksNeedYou : id === 'oby' && anyWaits) ? ' has-dot is-needs' : ''}`} data-tip={label} aria-label={label} aria-pressed={section === id}
                // Chats from a page (a thread, Settings): the column, and your conversation back
                // (and with the chats showing already, to the first row that waits, however far down: below)
                onClick={() => { if (id === 'oby' && section === 'oby') setLook((n) => n + 1); pick(id); if (id === 'oby' && !activeTalk && !atHome) onHome(); }}>
                <Icon size={20} weight={section === id ? 'fill' : 'regular'} />
              </button>
              {/* your own places above the line; the Obyspheres (there can be several) below */}
              {id === SECTIONS.at(-1)!.id && (everything || CAN_CREATE || strip.length > 0) && <span className="rail-sep" role="separator" />}
            </Fragment>
          ))}
          {strip.map((x) => (
            <button
              key={x.id}
              // the sample: a dashed edge, calling you (a ripple, then a pulsing dot) until you open it
              className={`rail-button${inSphere && sphere.id === x.id ? ' is-on' : ''}${x.sample ? ` is-sample${tour.sphere === 'sample' ? ' has-dot is-calling' : ''}${tour.sphere === 'leaving' ? ' is-leaving' : ''}` : ''}`}
              data-tip={x.sample ? `${x.name} · Sample` : x.name}
              aria-label={x.sample ? `${x.name}, a sample Obysphere` : x.name}
              aria-description={x.sample ? 'Sample. It goes away when setup is done.' : undefined}
              aria-pressed={inSphere && sphere.id === x.id}
              // its column, and its room when it has one (room.ts)
              onClick={() => { setSphereId(x.id); pick('obysphere'); if (x.sample) tourDid('tile'); else if (chatFirst && roomOpens(room, x)) onOpenRoom(x.id); }}
            >
              <SphereTile name={x.name} brand={x.brand} image={x.image} />
            </button>
          ))}
          {/* not a section: makes another Obysphere (CreateSphereModal) */}
          {(everything || CAN_CREATE) && <button className="rail-button" data-tip="Create an Obysphere" aria-label="Create an Obysphere" aria-haspopup="dialog" onClick={() => setCreating(true)}>
            <PlusIcon size={20} />
          </button>}
          {/* the computer the window is on, above Settings (docs/contracts/computers-app.md) */}
          <ComputerButton />
          {/* you, at the foot: your picture opens your account's menu (Settings is in it) */}
          {/* lean (Sangam's call of 2026-10-28): no account, no sign-in, nothing but this Mac, so
              Settings itself, the gear filled while it's open; the design's account picture and its
              menu (name, email, Log out) with ?everything */}
          {!everything ? (
            <button className={`rail-button rail-settings${inSettings ? ' is-on' : ''}`} data-tip={withShortcut('Settings', 'settings.open')} aria-label="Settings" aria-keyshortcuts={ariaKeys('settings.open')} aria-pressed={inSettings}
              // a Settings page open: the gear again closes the panel (as a tab's icon does); else it opens
              // Settings (General), staying on the Settings list if that's showing
              onClick={() => { if (inSettings && activeSetting) return pick('settings'); if (!inSettings) pick('settings'); if (!activeSetting) onOpenSetting('general'); }}>
              <GearSixIcon size={20} weight={inSettings ? 'fill' : 'regular'} />
            </button>
          ) : (
          <button
              className={`rail-button rail-account${inSettings || accountMenu ? ' is-on' : ''}`}
              data-tip={accountMenu ? undefined : 'Account'}
              aria-label={`${ME.name}’s account`}
              aria-haspopup="menu"
              aria-expanded={!!accountMenu}
              onClick={(e) => setAccountMenu(accountMenu ? null : e.currentTarget.getBoundingClientRect())}
            >
              <OwnerAvatar name={ME.name} size={24} badge={false} />
            </button>
          )}
        </div>

        <div className="sidebar-content">
        <div className="sidebar-title-row">
          <h2 className="sidebar-title">{inSphere ? sphere.name : section === 'oby' && shownProfile ? (
            <button
              className={`sidebar-title-button${profileMenu ? ' is-on' : ''}`}
              aria-haspopup="menu"
              aria-expanded={!!profileMenu}
              // obid's list again as it opens
              onClick={(e) => { if (!profileMenu) void loadProfiles(); setProfileMenu(profileMenu ? null : e.currentTarget.getBoundingClientRect()); }}
            >
              <span className="sidebar-title-name">{shownProfile.name}</span>
              <ChevronDown size={16} strokeWidth={1.8} />
            </button>
          ) : TITLES[section]}{inSphere && sphere.sample && <span className="sidebar-sample">Sample</span>}</h2>
          {section === 'home' && (
            <button
              className={`icon-button${archiveMenu ? ' is-on' : ''}`}
              aria-label="Home options"
              aria-haspopup="menu"
              aria-expanded={!!archiveMenu}
              onClick={(e) => setArchiveMenu(archiveMenu ? null : e.currentTarget.getBoundingClientRect())}
            >
              <EllipsisVertical size={16} strokeWidth={1.6} />
            </button>
          )}
          {section === 'obysphere' && !sphere.sample && (
            <button
              className={`icon-button${sphereMenu ? ' is-on' : ''}`}
              aria-label={`${sphere.name} options`}
              aria-haspopup="menu"
              aria-expanded={!!sphereMenu}
              onClick={(e) => setSphereMenu(sphereMenu ? null : e.currentTarget.getBoundingClientRect())}
            >
              <EllipsisVertical size={16} strokeWidth={1.6} />
            </button>
          )}
        </div>
        {/* chat first: Projects or Agents, under the profile's name (screen 08); the arrow keys move between them */}
        {section === 'oby' && AGENTS_SHOWN && (
          <div
            className="panel-side"
            role="tablist"
            aria-label="Projects or agents"
            onKeyDown={(e) => {
              if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
              e.preventDefault();
              e.currentTarget.querySelector<HTMLButtonElement>('[aria-selected="false"]')?.focus();
              pickSide(side === 'projects' ? 'agents' : 'projects');
            }}
          >
            {(['projects', 'agents'] as const).map((to) => (
              <button key={to} id={`panel-side-${to}`} role="tab" aria-selected={side === to} aria-controls="panel-side-panel" tabIndex={side === to ? 0 : -1} className={side === to ? 'is-on' : undefined} onClick={() => pickSide(to)}>
                {to === 'projects' ? 'Projects' : 'Agents'}
              </button>
            ))}
          </div>
        )}
        {/* how many members it has, you included */}
        {section === 'obysphere' && <p className="sidebar-subtitle">{members} member{members === 1 ? '' : 's'}</p>}
        {/* on the panel icon's row */}
        <div className="sidebar-top">
          {searchable && (
            <button className={`icon-button${searching ? ' is-on' : ''}`} aria-label="Search" title={withShortcut('Search', 'search.open')} aria-keyshortcuts={ariaKeys('search.open')} aria-expanded={searching} onClick={openSearch}>
              <Search size={17} strokeWidth={1.6} />
            </button>
          )}
        </div>
        {/* everything under the title scrolls together; the title stays */}
        <div className="sidebar-scroll">
        <div ref={list} className={`sidebar-list${scrolled ? ' is-scrolled' : ''}`} onScroll={(e) => { onScroll(); setScrolled(e.currentTarget.scrollTop > 0); }}>
        {/* slides open under the title (a one-row grid going 0fr → 1fr) */}
        <div className={`sidebar-search-wrap${searching ? ' is-open' : ''}`}>
          <div>
            <label className="sidebar-search">
              <Search size={14} strokeWidth={1.6} />
              <input
                ref={searchInput}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); setSearching(false); setQuery(''); } }}
                placeholder="Search"
                aria-label="Search"
                tabIndex={searching ? 0 : -1}
              />
            </label>
          </div>
        </div>

        {/* the Obysphere: what its Obys share (Knowledge: the knowledge sources
            connected for everyone), and inviting someone in (InviteModal), first;
            Invite member is the mock's and the sample's, not on a joined one */}
        {section === 'obysphere' && (
          <>
            <button className={`sidebar-new is-stacked${knowing ? ' is-active' : ''}${sphere.sample && tour.call === 'Knowledge' ? ' is-calling' : ''}`} aria-current={knowing || undefined} onClick={() => (sphere.sample ? tourDid('Knowledge') : onOpenKnowledge(sphere.id))}>
              <BookOpen size={16} strokeWidth={1.6} />
              <span className="sidebar-new-label">Knowledge</span>
            </button>
            {!sphere.live && (
              <button className="sidebar-new is-stacked" aria-haspopup={sphere.sample ? undefined : 'dialog'} onClick={() => (sphere.sample ? tourDid('invite') : setInviting(true))}>
                <UserPlus size={16} strokeWidth={1.6} />
                <span className="sidebar-new-label">Invite member</span>
              </button>
            )}
          </>
        )}
        {newAction && (
          <button className="sidebar-new" onClick={onNewChat}>
            <newAction.icon size={16} strokeWidth={1.6} />
            <span className="sidebar-new-label">{newAction.label}</span>
          </button>
        )}

        {/* chat first: Oby first (the empty chat, and all that waits with no row of its own), then
            every conversation with it */}
        {section === 'oby' && side === 'agents' && (
          <AgentsList needing={needing} working={working} profile={shownProfile?.id} activeTalk={activeTalk} onOpenTalk={onOpenTalk} />
        )}
        {section === 'oby' && side === 'projects' && (
          <div className="sidebar-plain" role="tabpanel" id="panel-side-panel" aria-labelledby="panel-side-projects">
            <button className="connector chat-own" onClick={onNewChat}>
              <ObyFace size={32} />
              <span className="connector-text">
                <span className="connector-name">Oby</span>
                <span className="connector-about">{latest ? convoAbout(latest) : noRow ? 'Needs you' : 'All quiet'}</span>
              </span>
              {noRow && <span className="status-dot is-needs" role="img" aria-label="Needs you" />}
            </button>
            <button className={`sidebar-new chat-new has-kbd${atHome ? ' is-active' : ''}`} aria-current={atHome || undefined} aria-keyshortcuts={ariaKeys('chat.new')} data-kbd={shortcutLabel('chat.new')} onClick={onNewChat}>
              <Plus size={16} strokeWidth={1.6} />
              <span className="sidebar-new-label">New conversation</span>
            </button>
            {/* everything across the profile's projects that needs your word (WaitingView), only while there is some */}
            {waiting > 0 && (
              <button className={`sidebar-new chat-new${activeWaiting ? ' is-active' : ''}`} aria-current={activeWaiting || undefined} onClick={onOpenWaiting}>
                <ListChecksIcon size={16} />
                <span className="sidebar-new-label">Waiting for you</span>
                <span className="waiting-count">{waiting}</span>
              </button>
            )}
            {projectGroups.length ? projectGroups.map((g) => {
              // while searching, a group with a match opens
              const shut = !q && projectsFolded.has(g.id);
              return (
                <section key={g.id || 'none'} className={`sidebar-group project-group${shut ? ' is-folded' : ''}`} data-project={g.id || undefined}>
                  {/* a project's heading has its own ⋮ beside it, so the heading stays a <button> */}
                  <div
                    className={`project-group-row${projectMenu?.id === g.id ? ' is-menu-open' : ''}`}
                    onContextMenu={g.id ? (e) => { e.preventDefault(); setProjectMenu({ id: g.id, at: new DOMRect(e.clientX, e.clientY, 0, 0), atPointer: true }); } : undefined}
                  >
                    <button className="sidebar-group-head" aria-expanded={!shut} onClick={() => toggleProject(g.id)}>
                      {g.id && <Folder size={14} strokeWidth={1.6} className="project-mark" />}
                      <h3>{g.name}</h3>
                      <ChevronDown size={12} strokeWidth={1.8} className="sidebar-chevron" />
                    </button>
                    {g.id && (
                      <button
                        className="sidebar-item-more"
                        aria-label={`More for ${g.name}`}
                        aria-haspopup="menu"
                        aria-expanded={projectMenu?.id === g.id}
                        onClick={(e) => setProjectMenu(projectMenu?.id === g.id ? null : { id: g.id, at: e.currentTarget.getBoundingClientRect() })}
                      >
                        <EllipsisVertical size={14} strokeWidth={1.6} />
                      </button>
                    )}
                  </div>
                  <div className="sidebar-group-body"><div>{g.rows.map(convoRow)}</div></div>
                </section>
              );
            }) : inOrder.map(convoRow)}
            {!talks.length && !(q && moreFound) && <p className="sidebar-empty">{q ? `No conversations match “${query.trim()}”` : 'No conversations yet'}</p>}
            {searchMore}
            {projectsOn && (
              <button className="sidebar-new chat-new project-new" aria-haspopup="dialog" onClick={() => setNewProject(true)}>
                <FolderPlus size={16} strokeWidth={1.6} />
                <span className="sidebar-new-label">New project</span>
              </button>
            )}
          </div>
        )}
        {/* chat first: the kinds of work, Home's chips, first thing under Tasks */}
        {chatFirst && section === 'home' && TYPES.map((c) => (
          <button key={c.id} className="sidebar-item chat-kind" onClick={() => onOpenGroup(c.id)}>
            <span className="sidebar-item-title">{c.label}</span>
            {hasFresh(c.id) && <span className="status-dot is-new" role="img" aria-label="New" />}
            {(countOf('needs', c.id) > 0 || toDoOf(c.id).length > 0) && <span className="status-dot is-needs" role="img" aria-label="Needs you" />}
          </button>
        ))}
        {/* your conversations with Oby, newest first, ones still going included */}
        {section === 'conversations' && (
          <div className="sidebar-plain">
            {pinned.length > 0 && <h3 className="sidebar-convo-head">Pinned</h3>}
            {pinned.map(convoRow)}
            {pinned.length > 0 && pinned.length < talks.length && <h3 className="sidebar-convo-head">Recent</h3>}
            {talks.filter((c) => !c.pinned).map(convoRow)}
            {/* none at all: in the middle, as Home's; none matching a search: at the top, under it */}
            {!talks.length && (query.trim()
              ? !moreFound && <p className="sidebar-empty">{`No conversations match “${query.trim()}”`}</p>
              : <p className="sidebar-empty is-middle">No conversations yet</p>)}
            {searchMore}
          </div>
        )}
        {/* Home: each kind of work folds open to its tasks (running / needs you
            counted on the right) */}
        {section === 'home' && (
          <div className="sidebar-plain">
            {/* nothing open, instead of three empty groups: never any tasks yet (just set up), or
                every one done and archived (the Archived button below has them) */}
            {groups.every((t) => !t.tasks.length) && <p className="sidebar-empty is-middle">
              {archivedCount > 0 ? 'All caught up.\nFinished tasks are in Archived.' : 'No tasks yet.\nThey’ll show up here\nonce you give me one.'}
            </p>}
            {groups.some((t) => t.tasks.length) && groups.map((t) => (
              <section key={t.id} className={`sidebar-group${folded.has(t.id) ? ' is-folded' : ''}`}>
                <button className="sidebar-group-head" aria-expanded={!folded.has(t.id)} onClick={() => toggleGroup(t.id)}>
                  <h3>{t.label}</h3>
                  <ChevronDown size={12} strokeWidth={1.8} className="sidebar-chevron" />
                  {/* folded, the orange dot still says something in it needs you */}
                  {t.needs > 0 && <span className="sidebar-group-needs" aria-label={`${t.needs} need${t.needs === 1 ? 's' : ''} you`} />}
                  {/* its count: not in v1 (Sangam: the rows say it); nothing open in it: none (a 0 says nothing) */}
                  {everything && t.count > 0 && <span className="sidebar-group-count" aria-label={`${t.count} ${t.countLabel}`}>{t.count}</span>}
                </button>
                <div className="sidebar-group-body">
                  <div>
                    {shownOf(t).rows.map((k) => (
                      // a row with its own menu button, so the row itself isn't a <button>; dimmed once
                      // done, but not for a status this build doesn't know (docs/contracts/wire.md, WIRE-3)
                      <div
                        key={k.id}
                        role="button"
                        tabIndex={0}
                        className={`task-row${k.status === 'done' && !k.unknown ? ' is-done' : ''}${active === k.id ? ' is-active' : ''}${taskMenu?.id === k.id ? ' is-menu-open' : ''}`}
                        aria-current={active === k.id || undefined}
                        onClick={() => { markSeen(k.id); onOpenTask(k.id); }}
                        onKeyDown={(e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) { e.preventDefault(); e.currentTarget.click(); } }}
                        // right-click: its menu, where the pointer is
                        onContextMenu={(e) => { e.preventDefault(); setTaskMenu({ id: k.id, at: new DOMRect(e.clientX, e.clientY, 0, 0), atPointer: true }); }}
                      >
                        <BrandMark brand={k.brand} size={14} mono />
                        {renamingTask === k.id ? (
                          <input
                            className="sidebar-rename"
                            defaultValue={k.title}
                            autoFocus
                            onFocus={(e) => e.currentTarget.select()}
                            onClick={(e) => e.stopPropagation()}
                            onBlur={(e) => doneRenamingTask(k.id, e.currentTarget.value)}
                            onKeyDown={(e) => {
                              if (composing(e)) return;
                              if (e.key === 'Enter') e.currentTarget.blur();
                              if (e.key === 'Escape') { e.stopPropagation(); setRenamingTask(null); }
                            }}
                            aria-label="Task name"
                          />
                        ) : (
                          <span className="sidebar-item-title">
                            {k.who && <span className="sidebar-item-who">{k.who}</span>}
                            {k.title}
                          </span>
                        )}
                        {/* at the row's end, its state: new, needs you, working, queued or done;
                            on hover, its ⋮ in the same spot, and its conversation before it */}
                        <StatusDot task={k} />
                        <button
                          className="sidebar-item-more task-row-chat"
                          aria-label={`Go to the conversation of ${k.title}`}
                          data-tip="Go to conversation"
                          onClick={(e) => { e.stopPropagation(); markSeen(k.id); onOpenChat(k.id); }}
                        >
                          <ChatsCircleIcon size={14} />
                        </button>
                        <button
                          className="sidebar-item-more"
                          aria-label={`More for ${k.title}`}
                          aria-haspopup="menu"
                          aria-expanded={taskMenu?.id === k.id}
                          onClick={(e) => { e.stopPropagation(); setTaskMenu(taskMenu?.id === k.id ? null : { id: k.id, at: e.currentTarget.getBoundingClientRect() }); }}
                        >
                          <EllipsisVertical size={14} strokeWidth={1.6} />
                        </button>
                      </div>
                    ))}
                    {shownOf(t).hidden > 0 && (
                      <button className="sidebar-more" onClick={() => toggleMore(t.id)}>
                        {more.has(t.id) ? 'Show less' : `Show ${shownOf(t).hidden} more`}
                      </button>
                    )}
                  </div>
                </div>
              </section>
            ))}
          </div>
        )}
        {/* the Obysphere: the people in it (their Obys); a conversation with one
            starts from its page. Then who's been invited and hasn't joined yet */}
        {section === 'obysphere' && (
          <div className="sidebar-plain">
            {/* Members, then Invited (who hasn't joined yet: once they do, they're
                in Members), each folding as Home's groups do */}
            <SphereGroup id={`members:${sphere.id}`} label="Members" folded={folded} onToggle={toggleGroup}>
            {/* taken out by its admin: said where its members were, not "Just you so far" (SPHEREAPP-14) */}
            {sphere.revoked
              ? <p className="sidebar-empty" role="status">{revokedLine(sphere)}</p>
              : !sphere.members.length && <p className="sidebar-empty">Just you so far</p>}
            {memberRows(sphere)}
            </SphereGroup>
            {sphere.invited.length > 0 && (
              <SphereGroup id={`invited:${sphere.id}`} label="Invited" count={sphere.invited.length} folded={folded} onToggle={toggleGroup}>
                {sphere.invited.map((e) => (
                  <div
                    key={e}
                    className={`connector oby-row invited-row${inviteMenu?.email === e ? ' is-menu-open' : ''}`}
                    onContextMenu={sphere.admin ? (ev) => { ev.preventDefault(); setInviteMenu({ email: e, at: new DOMRect(ev.clientX, ev.clientY, 0, 0), atPointer: true }); } : undefined}
                  >
                    <span className="invited-mark"><Mail size={12} strokeWidth={1.8} /></span>
                    <span className="connector-text"><span className="connector-name">{e}</span></span>
                    {resent === e && <span className="invited-sent">Sent</span>}
                    {sphere.admin && (
                      <button
                        className="sidebar-item-more"
                        aria-label={`More for ${e}`}
                        aria-haspopup="menu"
                        aria-expanded={inviteMenu?.email === e}
                        onClick={(ev) => setInviteMenu(inviteMenu?.email === e ? null : { email: e, at: ev.currentTarget.getBoundingClientRect() })}
                      >
                        <EllipsisVertical size={14} strokeWidth={1.6} />
                      </button>
                    )}
                  </div>
                ))}
              </SphereGroup>
            )}
          </div>
        )}
        {section === 'settings' && (
          <div className="sidebar-plain sidebar-settings">
            {SETTINGS_PAGES.map((p) => (
              <button
                key={p.id}
                className={`sidebar-item${activeSetting === p.id ? ' is-active' : ''}`}
                aria-current={activeSetting === p.id || undefined}
                onClick={() => onOpenSetting(p.id)}
              >
                <span className="sidebar-item-title">{p.label}</span>
              </button>
            ))}
          </div>
        )}
        </div>
        <div {...thumbProps} />
        </div>
        {/* Home's foot, held at the bottom under a line, however it's grouped:
            the archive, done tasks that have left Home; it opens as a page */}
        {/* there from the start, its count 0 till something's archived (Sangam, 2026-10-28);
            chat first, under the chats, once there's something in it */}
        {(section === 'home' || (section === 'oby' && archivedCount > 0)) && (
          <button className={`sidebar-archived${archivedCount ? '' : ' is-empty'}`} onClick={onOpenArchive}>
            Archived
            <span className="sidebar-group-count">{archivedCount}</span>
          </button>
        )}
        </div>
      </nav>

      {/* A Home task's menu (TaskMenu), its page first */}
      {taskMenu && menuTask && (
        <TaskMenu
          task={menuTask}
          at={taskMenu.at}
          align={taskMenu.atPointer ? 'left' : 'right'}
          onClose={() => setTaskMenu(null)}
          onViewDetails={() => { markSeen(menuTask.id); onOpenTask(menuTask.id); }}
          onOpenChat={() => onOpenChat(menuTask.id)}
          onRename={() => setRenamingTask(menuTask.id)}
        />
      )}
      {/* an invite's menu: send it again; take it back */}
      {inviteMenu && (
        <Menu at={inviteMenu.at} align={inviteMenu.atPointer ? 'left' : 'right'} onClose={() => setInviteMenu(null)} height={100} width={168}>
          <button role="menuitem" onClick={() => { setResent(inviteMenu.email); setInviteMenu(null); }}>Resend invite</button>
          <hr />
          <button role="menuitem" className="is-danger" onClick={() => { cancelInvite(sphere.id, inviteMenu.email); setInviteMenu(null); }}>Cancel invite</button>
        </Menu>
      )}
      {/* deleting a conversation asks first, wherever its menu was (CHATSTATE-8) */}
      <DeleteChatWindow />
      {/* a conversation's menu (ConvoMenu) */}
      {convoMenu && menuConvo && (
        <ConvoMenu
          chat={menuConvo}
          at={convoMenu.at}
          align={convoMenu.atPointer ? 'left' : 'right'}
          onClose={() => setConvoMenu(null)}
          onRename={() => setRenamingConvo(menuConvo.id)}
        />
      )}
      {accountMenu && (
        <Menu at={accountMenu} onClose={() => setAccountMenu(null)} align="left" width={240} height={150}>
          <div className="menu-head is-account">
            <span className="menu-head-text">
              <span className="menu-head-name">{ME.name}</span>
              <span className="menu-head-url">{ME.email}</span>
            </span>
          </div>
          <hr />
          {/* Settings opens on General, unless one of its pages is open already */}
          <button role="menuitem" className="has-kbd" aria-keyshortcuts={ariaKeys('settings.open')} data-kbd={shortcutLabel('settings.open')} onClick={() => { setAccountMenu(null); pick('settings'); if (!activeSetting) onOpenSetting('general'); }}>Settings</button>
          <hr />
          <button role="menuitem" onClick={() => setAccountMenu(null)}>Log out</button>
        </Menu>
      )}
      {profileMenu && !!profiles.list?.length && (
        <ProfileMenu list={profiles.list} kept={profiles.kept} at={profileMenu} onClose={() => setProfileMenu(null)} onNew={() => setNewProfile(true)} onEdit={setEditProfile} />
      )}
      {newProject && <ProjectSheet onClose={() => setNewProject(false)} onMade={(p) => { setNewProject(false); setMade(p.id); }} />}
      {projectMenu && (
        <Menu at={projectMenu.at} align={projectMenu.atPointer ? 'left' : 'right'} onClose={() => setProjectMenu(null)} height={94} width={180}>
          <button role="menuitem" onClick={() => { setProjectSheet({ id: projectMenu.id, deleting: false }); setProjectMenu(null); }}>Edit project…</button>
          <hr />
          <button role="menuitem" className="is-danger" onClick={() => { setProjectSheet({ id: projectMenu.id, deleting: true }); setProjectMenu(null); }}>Delete project…</button>
        </Menu>
      )}
      {sheetProject && projectSheet && (
        <ProjectSheet key={`${sheetProject.id}:${projectSheet.deleting}`} editing={sheetProject} deleting={projectSheet.deleting} onClose={() => setProjectSheet(null)} onMade={() => setProjectSheet(null)} />
      )}
      {editProfile && <EditProfileModal key={editProfile.id} profile={editProfile} defaultName={(profiles.list ?? []).find((p) => p.default)?.name ?? 'Default'} onClose={() => setEditProfile(null)} />}
      {newProfile && <NewProfileModal taken={(profiles.list ?? []).map((p) => p.color.toLowerCase())} onClose={() => setNewProfile(false)} />}
      {inviting && <InviteModal sphere={{ name: sphere.name, url: urlOf(sphere) }} domain={sphere.domain} onSent={(emails) => invite(sphere.id, emails)} onClose={() => setInviting(false)} />}
      {creating && <CreateSphereModal email={ME.email} onCreated={(id) => { setCreating(false); setSphereId(id); pick('obysphere'); }} onClose={() => setCreating(false)} />}
      {sphereMenu && (
        <Menu at={sphereMenu} onClose={() => setSphereMenu(null)} height={sphere.admin ? 120 : 170} width={201}>
          {/* which Obysphere: its picture, then its name and URL under it */}
          <div className="menu-head is-stacked">
            <SphereTile name={sphere.name} brand={sphere.brand} image={sphere.image} />
            <span className="menu-head-text">
              <span className="menu-head-name">{sphere.name}</span>
              <span className="menu-head-url">{urlOf(sphere)}</span>
            </span>
          </div>
          <hr />
          <button role="menuitem" onClick={() => { setSphereMenu(null); onOpenSphereSetting(sphere.id); }}>Settings</button>
          {!sphere.admin && <>
            <hr />
            <button role="menuitem" className="is-danger" onClick={() => { setSphereMenu(null); setLeaving(sphere.id); }}>Leave {sphere.name}</button>
          </>}
        </Menu>
      )}
      {leavingSphere && <ConfirmModal sphere={leavingSphere} onClose={() => setLeaving(null)} />}
      {archiveMenu && (
        <Menu at={archiveMenu} onClose={() => setArchiveMenu(null)} height={everything ? 290 : 170} width={180}>
          {everything && <>
            <span className="menu-label">Group by</span>
            {(['type', 'status'] as const).map((by) => (
              <button key={by} role="menuitemradio" aria-checked={groupBy === by} onClick={() => pickGroupBy(by)}>
                {by === 'type' ? 'Type' : 'Status'}
                {groupBy === by && <Check size={14} strokeWidth={1.8} className="menu-check" />}
              </button>
            ))}
            <hr />
          </>}
          <span className="menu-label">Archive completed</span>
          {ARCHIVE_AFTER.map((a) => (
            <button key={a.id} role="menuitemradio" aria-checked={archiveAfter === a.id} onClick={() => pickArchive(a.id)}>
              {a.label}
              {archiveAfter === a.id && <Check size={14} strokeWidth={1.8} className="menu-check" />}
            </button>
          ))}
        </Menu>
      )}
    </div>
    {/* the panel's right edge, out here so it can reach past the edge (the slot
        clips): drag to resize, double-click for the default width */}
    {open && (
      <div
        className="sidebar-resize"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize side panel"
        onPointerDown={startResize}
        onDoubleClick={() => onResize(null, true)}
      />
    )}
    </>
  );
}

// One of an Obysphere's lists in the side panel (Members, Invited), folding
// as Home's groups do: its name, a chevron after it, a count at the right
function SphereGroup({ id, label, count, folded, onToggle, children }: {
  id: string;
  label: string;
  count?: number;
  folded: Set<string>;
  onToggle: (id: string) => void;
  children: React.ReactNode;
}) {
  const shut = folded.has(id);
  return (
    <section className={`sidebar-group sphere-group${shut ? ' is-folded' : ''}`}>
      <button className="sidebar-group-head" aria-expanded={!shut} onClick={() => onToggle(id)}>
        <h3>{label}</h3>
        <ChevronDown size={12} strokeWidth={1.8} className="sidebar-chevron" />
        {count !== undefined && <span className="sidebar-group-count">{count}</span>}
      </button>
      <div className="sidebar-group-body"><div>{children}</div></div>
    </section>
  );
}
