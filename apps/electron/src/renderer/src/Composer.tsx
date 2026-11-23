import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ArrowUp, Check, ChevronLeft, File, FolderGit2, FolderOpen, Gauge, Laptop, ListChecks, Plus, Search, Settings, Square, SquareCheck, X } from 'lucide-react';
import { ObyIcon, OwnerAvatar } from './ObyFace';
import { PLACES, SHARED_WITH_ME } from './mock';
import { aboutOf, askableFor, useAskable, type Askable } from './askable';
import { agentOf, type Attachments } from './view';

import MessageEditor, { type EditorHandle, type MentionKey } from './MessageEditor';
import { mentioned } from './textTokens';
import StatusDot from './StatusDot';
import { MENU_GAP, useClicksEverywhere } from './Menu';
import { MENU_WIDTH } from './talk';
import { useTasks, type Task } from './tasks';
import { BrandMark } from './brands';
import { barOf, fullness, useAccounts, weekWords } from './SettingsView';
import { USING_PLAN } from './chatgpt';
import { ManageUsage } from './ChatGptPlan';
import { lookAtUsage, queuedFiles, uploadFiles, useRepos } from './live';
import { useScrollThumb } from './useScrollThumb';
import { usePrompt } from './prompts';
import { leave, useGlide } from './glide';
import { folderName, folderPlace, foldersOf, pickFolders, setFolders, useFolders, usedFolders, useRecentFolders } from './folders';
import { everything } from './lean';
import { dataOf, draft as keptDraft, fileOf, hold, keep, keeper, leftInBox, type KeptTag } from './drafts';
import TalkPill from './TalkPill';
import { useChats, type Chat } from './chats';
import { useProjects, useProjectsOn } from './projects';
import { ProjectChip } from './ProjectSheet';
import TaskChips from './TaskChips';
import QueuedWords from './QueuedWords';
import { chipsOf, defaultsOf } from './chips';
import { FOLDER_DROPPED, dropped, fileWords, isFileDrag, noCommands, skillsMatching, slashOf, useMentionFiles, useSkills, useStopping } from './composerInput';
import { takesWorkspace, useLive, type SkillsWhere } from './live';
import { WORKTREE_WORDS } from '../../shared/finish';
import { onObid } from './lean';
import { has } from '../../shared/capabilities';
import type { ConversationFile } from './obid';
import type { Queued } from './chats';
import { composing } from './shortcuts';

// What a message carries besides its words: files or photos (a photo with a
// small preview of itself), tasks attached. (Other Obys are brought in by name,
// in the words: @Wellington.) place: the Mac it runs on, when not this one
// (one at a time: picking another replaces it). repo: the repository the work is
// in (obid's), one at a time too.
type Attached = { kind: 'file'; name: string; preview?: string; file?: File; path?: string } | { kind: 'task'; id: string } | { kind: 'place'; name: string } | { kind: 'repo'; name: string };
type View = 'root' | 'oby' | 'task' | 'where' | 'repo' | 'folder' | 'usage';
const same = (a: Attached, b: Attached) =>
  a.kind === 'task' ? b.kind === 'task' && a.id === b.id : a.kind === b.kind && a.name === (b as { name: string }).name;
const keyOf = (a: Attached) => (a.kind === 'task' ? `task:${a.id}` : `${a.kind}:${a.name}`);

// image.png, image 2.png, image 3.png: the first of them not taken
export function numbered(name: string, taken: Set<string>) {
  let next = name;
  for (let n = 2; taken.has(next); n++) next = name.replace(/(\.[^.]*)?$/, ` ${n}$1`);
  return next;
}

export { leftInBox };

// Whether the box stacks at this height: once its text runs past one line, until it's sent. An empty
// box measured while still stacked is past one line by its padding alone, which is no reason to stay
export const stackedAt = (was: boolean, height: number, text: string) => was || (height > 33 && text !== '');

// The message box: Home's, and the one at the foot of a conversation (or a
// task's or an Oby's page): tags for what's attached over one row of +, the
// text and send; once the text runs past one line, the text over a row of +
// and send instead (until it's sent). Send is there all along, dim till
// there's something to send (the window has no voice button), next to usage
// (a gauge: how much of the agents' accounts' 5-hour session and week is
// used): after it in the design (?everything), before it in v1, where the
// gauge keeps the end. The +
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
// Send is Stop while Oby answers and there's nothing to send (CHATFIRST-33)
export const stopShows = (answering: boolean, filled: boolean, attached: number) => answering && !filled && !attached;

export default function Composer({ placeholder, prompts = false, note, label, onSend, home = false, hidden = false, autoFocus = !home, className = '', style, repos: withRepos = false, conversation, talk, skills, draft, say, onStop, queued = [], onCancelQueued, history, quote }: {
  placeholder?: string;
  prompts?: boolean;  // no placeholder of its own: one of Oby's prompts, changing now and then (prompts.ts)
  note?: React.ReactNode;  // a line just over the box (Home's Settings tip), coming and going with it
  label: string;
  // its words, and what went with them; a promise (words to a task) keeps them in the box until it
  // settles, and all of them when it's false: obid refused them, and said why (ATTACH-20)
  onSend?: (text: string, attached: Attachments) => unknown;
  // Words alone, which can be refused (the Sphere's room): no +, no @ list, no usage, no files pasted.
  // The box keeps them until they're answered, and for good on false
  say?: (text: string) => Promise<boolean>;
  home?: boolean;    // Home's: 480px wide, steps aside (hidden) while a page is open over it
  // words to Oby (Home's, a conversation's): the repository the work is in, as a tag, the
  // conversation's last one until another is picked or it's taken out (+ › Choose a repository)
  repos?: boolean;
  conversation?: string;
  talk?: Chat | true;  // who Oby talks through, by send: this conversation's, or (true, Home's) a new one's
  // whose skills `/` lists, where the words go (COMPOSER-25): a task's box names its task; by default a
  // conversation's box its conversation, Home's a new one's, and any other box none
  skills?: SkillsWhere;
  draft?: string;    // the box its unsent words are kept as (drafts.ts, box); none: not kept
  onStop?: () => void;  // Oby is answering here: with nothing to send, send is Stop (CHATFIRST-33)
  // words sent while Oby answers, waiting their turn: over the box, with Edit and Cancel (COMPOSER-8 to 10).
  // onCancelQueued answers whether obid dropped them
  queued?: Queued[];
  onCancelQueued?: (q: Queued) => Promise<boolean>;
  // the owner's earlier words here, oldest first: ↑ in an empty box brings them back (COMPOSER-20 to 22)
  history?: string[];
  // where a reply's picked words are put in the box, quoted above the caret (ReplySelection, COMPOSER-23)
  quote?: React.MutableRefObject<((text: string) => void) | null>;
  hidden?: boolean;
  autoFocus?: boolean;  // focused on opening (a conversation's is; Home's and a task page's aren't)
  className?: string;  // a page's own spacing around it
  style?: React.CSSProperties;
}) {
  const prompt = usePrompt(prompts, hidden);
  const wordsOnly = !!say;
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
  // the skill picked for the words (COMPOSER-26, 27), set below: something to send on its own too
  const [skill, setSkill] = useState<string | null>(null);
  // Stop pressed: "Stopping…" until the turn really ends (COMPOSER-12)
  const [stopPending, askStop] = useStopping(!!onStop);
  // The chips (COMPOSER-1 to 3): where words go to Oby (Home's, a conversation's), once obid takes them
  const status = useLive().status;
  const chipsShown = !!talk && !wordsOnly && (!onObid || has(status, 'composer-v1'));
  // the skills of the agent the words go to, once obid lists them (always on the mock): `/` lists them,
  // and the one picked goes with the words as a chip (COMPOSER-25 to 27)
  const skillsWhere: SkillsWhere | undefined = wordsOnly ? undefined : skills ?? (talk === true ? {} : talk ? { conversation: talk.id } : undefined);
  const skillsOn = !!skillsWhere && (!onObid || has(status, 'skills-v1'));
  // words, or a skill picked with none (COMPOSER-27): send is lit, and it's no Stop
  const ready = filled || (skillsOn && !!skill);
  const stopping = stopShows(!!onStop, ready, attached.length);
  // the whole text, for a / at its start (COMPOSER-16); Esc puts the commands' list away until the / goes
  const [typed, setTyped] = useState('');
  const slash = chipsShown || skillsOn ? slashOf(typed) : null;
  const [slashClosed, setSlashClosed] = useState(false);
  useEffect(() => { if (slash === null) setSlashClosed(false); }, [slash]);
  // files dragged over the box, and what it says of folders dropped (COMPOSER-13)
  const [dragging, setDragging] = useState(false);
  const [dropNote, setDropNote] = useState('');
  useEffect(() => {
    if (!dropNote) return;
    const t = setTimeout(() => setDropNote(''), 5000);
    return () => clearTimeout(t);
  }, [dropNote]);
  // its files going up: a second send waits
  const sending = useRef(false);
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
  // The project (docs/contracts/projects.md, PROJECTS-19, 22 and 25): Home's chip picks the one its
  // words start a conversation in, for this run only (one deleted since: none); a conversation in one
  // works in its folders, which its title bar's chip stands for: the box shows no folder of it. Either
  // way the box picks and sends no folders of its own
  const projectsOn = useProjectsOn();
  const projects = useProjects();
  const [homeProject, setHomeProject] = useState<string | null>(null);
  const project = home && projectsOn && projects.some((p) => p.id === homeProject) ? homeProject : null;
  const chats = useChats();
  const inProject = !!conversation && projectsOn && !!chats.find((c) => c.id === conversation)?.project;
  const ownFolders = canFolders && !project && !inProject;
  const folders = !canFolders || project || inProject ? [] : conversation ? kept : homeFolders;
  // how many folders the conversation works in: a file of one of several is named after its folder
  const chat = chats.find((c) => c.id === conversation);
  // where the conversation Home's words start works (WORKSPACE-3): this folder by default, or a new
  // worktree with Worktree checked, as Claude Code offers one (the owner's choice, 2026-11-23),
  // chosen with its first message and never after: only in the box that starts a new conversation
  // (Home's), never a reply's, a follow-up's or a task's, with folders or a project to work in, once
  // obid takes it
  const workspaceOn = takesWorkspace(useLive());
  const [workspace, setWorkspace] = useState<'folder' | 'worktree'>('folder');
  const choosesWorkspace = home && talk === true && !conversation && workspaceOn && (homeFolders.length > 0 || !!project);
  const workFolders = inProject ? projects.find((p) => p.id === chat?.project)?.folders.length ?? 1 : kept.length;
  // the tool the conversation talks through, for the commands' list when no chip names one
  const [accounts] = useAccounts();
  const accountAgent = accounts.find((a) => a.id === (chat?.talkedOn ?? chat?.talkAccount))?.agent;
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
  // whom @ asks: on obid, the Obysphere's members that take questions (ASK-37)
  const askable = useAskable();
  // a name typed out in full is done: the list goes, and Enter sends
  const fullName = at !== null && askable.some((o) => o.name.toLowerCase() === at.toLowerCase());
  const listing = !(wordsOnly || hidden || at === null || atClosed || view || fullName);
  // after the Obys, the files of the conversation's folders (COMPOSER-14)
  const atFiles = useMentionFiles(listing && conversation ? conversation : undefined, listing ? at : null);
  const matches: ({ kind: 'oby' } & Askable | { kind: 'file'; file: ConversationFile })[] = listing
    ? [...askableFor(at!, askable).map((o) => ({ kind: 'oby' as const, ...o })), ...atFiles.map((file) => ({ kind: 'file' as const, file }))] : [];
  const slashShown = slash !== null && !slashClosed && !view && !hidden;
  // the skills `/` lists: asked as it opens (null till obid answers), those matching what's typed after it
  const skillsAnswer = useSkills(skillsOn ? skillsWhere : undefined, slashShown && !matches.length);
  const skillRows = skillsAnswer ? skillsMatching(skillsAnswer.skills, slash ?? '') : [];
  const [skillLit, setSkillLit] = useState(0);
  useEffect(() => setSkillLit(0), [slash]);
  // the list as Claude's own slash menu draws it: a popover over the box's left edge, as tall as fits
  // above the box (at most about 13 rows) and scrolling inside, the lit row kept in view as the
  // arrows move, its description beside it
  const skillList = useRef<HTMLDivElement>(null);
  const [skillTip, setSkillTip] = useState<{ top: number; left: number } | null>(null);
  const placeSkillTip = () => {
    const panel = skillList.current, row = panel?.querySelector<HTMLElement>('.is-lit');
    const form = (panel?.offsetParent ?? panel?.parentElement)?.getBoundingClientRect();
    if (!panel || !row || !form) return setSkillTip(null);
    const r = row.getBoundingClientRect(), p = panel.getBoundingClientRect();
    // beside the row while it shows in the list; a row scrolled out of it has none
    const next = p.height > 0 && (r.bottom <= p.top || r.top >= p.bottom) ? null : { top: Math.round(r.top - form.top + r.height / 2), left: Math.round(p.right - form.left + 6) };
    setSkillTip((was) => (was && next && was.top === next.top && was.left === next.left ? was : next));
  };
  const skillsListed = slashShown && !matches.length && skillRows.length > 0;
  useLayoutEffect(() => {
    const panel = skillList.current;
    if (!panel) return setSkillTip(null);
    panel.style.maxHeight = `${Math.max(120, Math.min(376, panel.getBoundingClientRect().bottom - 12))}px`;
    panel.querySelector('.is-lit')?.scrollIntoView({ block: 'nearest' });
    placeSkillTip();
  }, [skillsListed, skillLit, skillRows.length]);
  // one picked: the /words go, and it's the box's chip (another picked replaces it)
  const pickSkill = (name: string) => { setSkill(name); box.current?.clear(); box.current?.focus(); };
  useClicksEverywhere(matches.length > 0 || slashShown);
  // a click anywhere outside the list (the conversation, the side panel, elsewhere in the box) closes
  // it, picking nothing, as Claude's own menus do
  const listOpen = matches.length > 0 || slashShown, atListOpen = matches.length > 0;
  useEffect(() => {
    if (!listOpen) return;
    const onDown = (e: PointerEvent) => {
      if (e.target instanceof Element && e.target.closest('.mention-panel, .slash-panel')) return;
      if (atListOpen) setAtClosed(true);
      else setSlashClosed(true);
    };
    // Esc wherever the focus is: the box's own keys hear it only while the box has the focus, and a
    // click elsewhere (the side panel, the window's edge) takes it away while the list stays. It's
    // the list's alone: nothing under it (a conversation going back) hears it too
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.isComposing) return;
      e.preventDefault();
      e.stopPropagation();
      if (atListOpen) setAtClosed(true);
      else setSlashClosed(true);
      box.current?.focus();
    };
    addEventListener('pointerdown', onDown, true);
    addEventListener('keydown', onKey, true);
    return () => { removeEventListener('pointerdown', onDown, true); removeEventListener('keydown', onKey, true); };
  }, [listOpen, atListOpen]);
  const pickOby = (name: string) => { setView(null); box.current?.mention(name); box.current?.focus(); };
  const pickMatch = (m: (typeof matches)[number]) => pickOby(m.kind === 'oby' ? m.name : fileWords(m.file, workFolders));
  const onMentionKey = (key: MentionKey) => {
    if (slashShown && !matches.length && skillRows.length) {
      const i = Math.min(skillLit, skillRows.length - 1);
      if (key === 'close') setSlashClosed(true);
      else if (key === 'pick') pickSkill(skillRows[i].name);
      else setSkillLit((i + (key === 'down' ? 1 : skillRows.length - 1)) % skillRows.length);
      return true;
    }
    if (slashShown && !matches.length) { if (key !== 'close') return false; setSlashClosed(true); return true; }
    if (!matches.length) return false;
    if (key === 'close') setAtClosed(true);
    else if (key === 'pick') pickMatch(matches[Math.min(lit, matches.length - 1)]);
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
  useEffect(() => {
    if (!quote) return;
    quote.current = (text) => { box.current?.quote(text); box.current?.focus(); };
    return () => { quote.current = null; };
  }, [quote]);
  const form = useRef<HTMLFormElement>(null);
  const tasks = useTasks();
  // Past 3 rows the tags scroll: kept at the bottom, so what you just added
  // shows, and faded at the top while older ones are hidden above
  const tagRow = useRef<HTMLDivElement>(null);
  // tags coming and going: the rest glide to their places (glide.ts)
  const tagSig = [skill && skillsOn ? 'skill' : '', ...folders.map((f) => `folder:${f}`), repo ? 'repo' : '', ...attached.map(keyOf)].join('|');
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

  // files picked or pasted: each a tag, a photo with its preview. A pasted one named as one
  // there already (every screenshot is image.png) is numbered, so neither is dropped
  const addFiles = (list: File[], pasted = false) => {
    const taken = new Set(attached.flatMap((a) => (a.kind === 'file' ? [a.name] : [])));
    const picked = list.map((f): Attached => {
      const base = f.name || 'Pasted image', name = pasted ? numbered(base, taken) : base;
      taken.add(name);
      return { kind: 'file', name, preview: f.type.startsWith('image/') ? URL.createObjectURL(f) : undefined, file: f };
    });
    if (picked.length) add(...picked);
  };
  // The box's draft: put back when it shows (or when it's another box's), and
  // kept as it's typed, a moment after the last keystroke; what's pending is
  // written when the box goes, or the window closes (pagehide), so a quick
  // quit loses nothing. Sending empties the box, which lets its draft go.
  const drafts = useRef<ReturnType<typeof keeper>>(null);
  drafts.current ??= keeper();
  // which box the tags shown are of: none kept for a box until its own are put back
  const [tagsOf, setTagsOf] = useState<string>();
  // the box shown now: a send that settles after another shows leaves that one alone
  const showing = useRef(draft);
  useLayoutEffect(() => {
    showing.current = draft;
    const d = drafts.current!, b = box.current;
    const text = d.show(draft ?? null);
    if (b && draft !== undefined && (text || b.text())) b.setText(text);
    // its tags and choices come back with its words (DRAFTS-4 to 7): a file whose bytes weren't
    // kept, after a relaunch, doesn't
    if (draft !== undefined && !wordsOnly) {
      const e = d.more();
      setAttached((e.tags ?? []).flatMap((t): Attached[] => {
        if (t.kind !== 'file') return t.kind === 'repo' ? [] : [t];
        const file = fileOf(draft, t);
        return file ? [{ kind: 'file', name: t.name, file, preview: file.type.startsWith('image/') ? URL.createObjectURL(file) : undefined }] : [];
      }));
      const r = e.tags?.find((t) => t.kind === 'repo');
      setRepoPick(r && r.kind === 'repo' ? r.name : undefined);
      if (home) { setHomeProject(e.project ?? null); setHomeFolders(e.folders ?? []); }
      setTagsOf(draft);
    }
    // a skill picked is for this box's words alone
    setSkill(null);
    addEventListener('pagehide', d.flush);
    return () => { removeEventListener('pagehide', d.flush); d.flush(); };
  }, [draft]);
  // and are kept as they change, the files' bytes read first (DRAFTS-4 to 8)
  useEffect(() => {
    if (draft === undefined || wordsOnly || tagsOf !== draft) return;
    const files = attached.flatMap((a) => (a.kind === 'file' ? [a] : []));
    hold(draft, files);
    let gone = false;
    void Promise.all(files.map((f) => (f.file ? dataOf(f.file) : undefined))).then((datas) => {
      if (gone) return;
      let n = 0;
      const tags = attached.map((a): KeptTag => (a.kind === 'file' ? { kind: 'file', name: a.name, ...(a.file?.type ? { type: a.file.type } : {}), ...(datas[n++] ? { data: datas[n - 1] } : {}) } : a));
      if (typeof repoPick === 'string') tags.push({ kind: 'repo', name: repoPick });
      drafts.current!.carry({ tags, ...(home ? { project: homeProject, folders: homeFolders } : {}) });
    });
    return () => { gone = true; };
  }, [attached, repoPick, homeProject, homeFolders, tagsOf, draft]);
  // Home's box stepping aside (a page opened over it) puts its panel away
  // and its lists, `/`'s and @'s: a box no one sees holds none open, nor their keys and clicks on the
  // window (Esc in the page over it is that page's)
  useEffect(() => {
    if (!hidden) return;
    setView(null);
    // only a list that is open: one closed with nothing typed would stay closed until a / is typed and taken out
    if (slash !== null) setSlashClosed(true);
    if (at !== null) setAtClosed(true);
  }, [hidden]);
  // a click outside the box and its panel closes the panel (anywhere in the window)
  useClicksEverywhere(!!view);
  useEffect(() => {
    if (!view) return;
    const onDown = (e: PointerEvent) => { if (!field.current?.contains(e.target as Node)) setView(null); };
    addEventListener('pointerdown', onDown, true);
    return () => removeEventListener('pointerdown', onDown, true);
  }, [view]);

  return (
    <form ref={form} className={`${home ? `composer${hidden ? ' is-hidden' : ''}` : 'composer chat-composer rise'} ${className}`.trim()} inert={hidden} style={style} onSubmit={async (e) => {
      e.preventDefault();
      const b = box.current!;
      // words, or something attached, or both
      if (sending.current || (!b.text().trim() && !attached.length && !(skillsOn && skill))) return;
      // the files go up first (on obid): one refused keeps the box as it was, words and tags
      sending.current = true;
      const files = await uploadFiles(attached.flatMap((a) => (a.kind === 'file' ? [{ name: a.name, preview: a.preview, file: a.file, path: a.path }] : [])));
      sending.current = false;
      if (!files) return;
      if (say) {
        const sent = b.text();
        sending.current = true;
        const went = await say(sent);
        sending.current = false;
        const left = leftInBox(sent, b.text(), went);
        if (left !== b.text()) b.setText(left);
        if (!left) { setFilled(false); setStacked(false); }
        return;
      }
      const sent = b.text(), from = draft;
      const went = onSend?.(sent, {
        files,
        obys: mentioned(b.text()),
        jobs: attached.flatMap((a) => (a.kind === 'task' ? [a.id] : [])),
        on: attached.find((a) => a.kind === 'place')?.name,
        repo,
        ...(folders.length && !inProject ? { folders } : {}),
        ...(project ? { project } : {}),
        // the chips go with the words, and stay for the next (COMPOSER-3)
        ...(chipsShown && defaultsOf(chipsOf(draft)) ? { defaults: defaultsOf(chipsOf(draft)) } : {}),
        // the skill picked, beside the words; never put in them (COMPOSER-27)
        ...(skillsOn && skill ? { skill } : {}),
        // where the conversation these words start works (WORKSPACE-3)
        ...(choosesWorkspace ? { workspace } : {}),
      });
      if (went instanceof Promise) {
        sending.current = true;
        const taken = await went.catch(() => false);
        sending.current = false;
        if (taken === false) return;
      }
      if (folders.length && !inProject) usedFolders(folders);
      // another box shows now (DRAFTS-1, DRAFTS-4): its words and tags are its own, and the box the
      // words went from keeps only what was typed after them
      if (showing.current !== from) {
        if (from !== undefined) { keep(from, leftInBox(sent, keptDraft(from), true), localStorage, Date.now(), {}); hold(from, []); }
        return;
      }
      // Home's go with the conversation they start; a conversation keeps its own
      if (!conversation) { setHomeFolders([]); setWorkspace('folder'); }
      // what was typed while the words went stays (DRAFTS-1)
      const left = leftInBox(sent, b.text(), true);
      if (left) b.setText(left);
      else b.clear();
      setFilled(!!left.trim());
      setStacked(false);
      setAttached([]);
      // the skill was for these words only (the mode chip stays)
      setSkill(null);
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
      {/* words sent while Oby answers, waiting their turn (COMPOSER-8 to 10) */}
      {onCancelQueued && <QueuedWords queued={queued} onCancel={onCancelQueued} onEdit={async (q) => {
        if (!(await onCancelQueued(q))) return;
        const b = box.current, now = b?.text().trim() ?? '';
        b?.setText([q.text, now].filter(Boolean).join('\n\n'));
        // its files come back as tags, where obid keeps them (COMPOSER-10)
        const back = queuedFiles(q.id, q.files).map((f): Attached => ({ kind: 'file', ...f }));
        if (back.length) setAttached((all) => [...all, ...back.filter((a) => !all.some((x) => same(x, a)))]);
        b?.focus();
      }} />}
      {dropNote && <p className="composer-drop-note" role="status">{dropNote}</p>}
      <div
        ref={field}
        className={`composer-field${stacked ? ' is-stacked' : ''}${dragging ? ' is-dropping' : ''}`}
        // files dropped on the box are attached, as + › Add files does (COMPOSER-13); a box of
        // words alone (the room) takes none
        onDragEnter={wordsOnly ? undefined : (e) => { if (isFileDrag(e.dataTransfer)) { e.preventDefault(); setDragging(true); } }}
        onDragOver={wordsOnly ? undefined : (e) => { if (isFileDrag(e.dataTransfer)) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; } }}
        onDragLeave={wordsOnly ? undefined : (e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setDragging(false); }}
        onDrop={wordsOnly ? undefined : (e) => {
          if (!isFileDrag(e.dataTransfer)) return;
          e.preventDefault();
          setDragging(false);
          const got = dropped(e.dataTransfer);
          addFiles(got.files);
          if (got.folders && !got.files.length) setDropNote(FOLDER_DROPPED);
          box.current?.focus();
        }}
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
        {view === 'usage' && <UsagePanel anchor={gauge} onClose={closePanel} onBlur={onPanelBlur} />}
        {view && view !== 'usage' && (
          <AddPanel
            opening={opening}
            view={view}
            tasks={tasks}
            onView={(v) => (v ? setView(v) : closePanel())}
            onBlur={onPanelBlur}
            onFiles={() => { closePanel(); files.current?.click(); }}
            // the first time, straight to the Mac's picker; after, the recent ones (and Choose another)
            onFolders={ownFolders ? () => { if (recentFolders.length) setView('folder'); else { closePanel(); void chooseFolders(); } } : undefined}
            folders={{ recent: recentFolders, picked: folders, onToggle: toggleFolder, onChoose: () => { closePanel(); void chooseFolders(); } }}
            onPick={add}
            onPickOby={pickOby}
            repos={everything && withRepos ? repos : []}
            isPicked={(a) => (a.kind === 'repo' ? a.name === repo : attached.some((x) => same(x, a)))}
            isNamed={(name) => named.includes(name)}
          />
        )}
        {matches.length > 0 && (
          <div className="add-panel mention-panel" role="listbox" aria-label={atFiles.length ? 'Obys and files' : 'Obys'}>
            {matches.map((m, i) => (
              <button
                type="button"
                role="option"
                key={m.kind === 'oby' ? m.name : `${m.file.folder}/${m.file.path}`}
                aria-selected={i === Math.min(lit, matches.length - 1)}
                className={`${i === Math.min(lit, matches.length - 1) ? 'is-lit' : ''}${m.kind === 'file' ? ' mention-file' : ''}`.trim()}
                // the text keeps the focus (and its cursor)
                onMouseDown={(e) => e.preventDefault()}
                onMouseEnter={() => setLit(i)}
                onClick={() => pickMatch(m)}
                title={m.kind === 'file' ? `${m.file.folder}/${m.file.path}` : undefined}
              >
                {m.kind === 'oby' ? <OwnerAvatar name={m.card ?? m.name} size={22} /> : <File size={16} strokeWidth={1.6} />}
                {m.kind === 'oby'
                  ? <ObyRow oby={m} />
                  : <span className="add-item-text">{m.file.path.split('/').pop()}<span className="add-item-about">{fileWords(m.file, workFolders)}</span></span>}
              </button>
            ))}
          </div>
        )}
        {/* a / at the start: the skills of the agent the words go to, where obid gives them
            (COMPOSER-25); with none to list, obid's why, else what's missing (COMPOSER-16) */}
        {skillsListed && (
          <div ref={skillList} className="add-panel mention-panel skill-panel" role="listbox" aria-label="Skills" onScroll={placeSkillTip}>
            {skillRows.map((k, i) => (
              <button
                type="button"
                role="option"
                key={k.name}
                aria-selected={i === Math.min(skillLit, skillRows.length - 1)}
                className={`mention-skill${i === Math.min(skillLit, skillRows.length - 1) ? ' is-lit' : ''}`}
                // the text keeps the focus (and its cursor)
                onMouseDown={(e) => e.preventDefault()}
                onMouseEnter={() => setSkillLit(i)}
                onClick={() => pickSkill(k.name)}
                aria-description={k.description || undefined}
              >
                <span className="add-item-text">{k.name}</span>
              </button>
            ))}
          </div>
        )}
        {/* the lit row's description, in a tip beside it */}
        {slashShown && !matches.length && skillTip && skillRows[Math.min(skillLit, skillRows.length - 1)]?.description && (
          <div className="skill-tip" role="tooltip" style={{ top: skillTip.top, left: skillTip.left }}>{skillRows[Math.min(skillLit, skillRows.length - 1)].description}</div>
        )}
        {slashShown && !matches.length && !skillRows.length && (!skillsOn || skillsAnswer) && (
          <div className="add-panel slash-panel" role="dialog" aria-label="Commands">
            <p className="add-empty">{skillsAnswer?.skills.some((k) => k.userInvocable) ? `Nothing matches “/${slash}”` : skillsAnswer?.why ?? noCommands(agentOf(skillsAnswer?.tool ?? accountAgent ?? 'claude').name)}</p>
          </div>
        )}
        {/* no talking (v1): send before the usage gauge, which keeps the end */}
        <div className={`composer-grid${everything && !wordsOnly ? '' : ' is-send-first'}`}>
          {(attached.length > 0 || repo || folders.length > 0 || (home && projectsOn) || (skillsOn && skill)) && (
            <div ref={tagRow} className={`composer-tags${hiddenAbove ? ' is-scrolled' : ''}`} onScroll={(e) => setHiddenAbove(e.currentTarget.scrollTop > 0)}>
              {/* the skill picked, first: it goes with these words (COMPOSER-26) */}
              {skillsOn && skill && (
                <span data-glide="skill" className="composer-tag composer-skill">
                  <span className="composer-tag-text">{`/${skill}`}</span>
                  <button type="button" aria-label={`Remove /${skill}`} onClick={(e) => leave(e.currentTarget.parentElement, () => setSkill(null))}><X size={13} strokeWidth={2} /></button>
                </span>
              )}
              {/* Home's project, first (choosing one lets go of the folders picked here) */}
              {home && projectsOn && <ProjectChip chosen={homeProject} onChoose={(p) => { setHomeProject(p); if (p) setHomeFolders([]); }} className="composer-project" />}
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
          {!wordsOnly && <button
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
          </button>}
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
            // after a lone /, what to do next, as Claude's own slash menu says it
            hint={skillsListed && slash === '' ? { after: '/', text: 'Type to filter' } : undefined}
            label={label}
            onText={(t) => { setFilled(!!t.trim()); setNamed(mentioned(t)); setTyped(t); drafts.current!.typed(t); }}
            // each change narrows the list, so the first is lit again
            onMention={(q) => { setAt(q); setLit(0); if (q === null) setAtClosed(false); }}
            onMentionKey={onMentionKey}
            // Backspace at the very start (or in an empty box) takes the skill's chip out (COMPOSER-26)
            onBackAtStart={skillsOn && skill ? () => { setSkill(null); return true; } : undefined}
            onEnter={() => form.current?.requestSubmit()}
            // past one line (32px: 22 of text, 10 of padding) it stacks
            onHeight={(h) => setStacked((was) => stackedAt(was, h, box.current?.text() ?? ''))}
            onPasteFiles={wordsOnly ? undefined : (list) => addFiles(list, true)}
            history={history}
          />
          {talk && (
            <div className="composer-talk">
              {chipsShown && <TaskChips box={draft} />}
              {/* where the conversation works (WORKSPACE-3): unchecked this folder, checked a new worktree */}
              {choosesWorkspace && (
                <button type="button" role="checkbox" aria-checked={workspace === 'worktree'} className={`talk-pick task-chip composer-worktree${workspace === 'worktree' ? '' : ' is-default'}`}
                  onMouseDown={(e) => e.preventDefault()} onClick={() => setWorkspace((w) => (w === 'worktree' ? 'folder' : 'worktree'))}>
                  {workspace === 'worktree' ? <SquareCheck size={15} strokeWidth={1.6} /> : <Square size={15} strokeWidth={1.6} />}
                  <span>{WORKTREE_WORDS}</span>
                </button>
              )}
              <TalkPill chat={talk === true ? undefined : talk} />
            </div>
          )}
          {/* how much of the agents' accounts is used, session and week (UsagePanel on a click) */}
          {!wordsOnly && <UsageGauge ref={gauge} open={view === 'usage'} onClick={() => (view === 'usage' ? closePanel() : setView('usage'))} />}
          {/* at the end, send, there all along and dim till there's something to send: the window has
              no voice button (AGENTS.md, "Sangam's design leads"), as his boards show (board 2) */}
          {/* Oby answering, and nothing to send: Stop where send is (the owner's ask of 2026-11-10);
              words typed meanwhile are sent as ever, and wait their turn */}
          {stopping ? (
            // pressed, it says "Stopping…" until the turn really ends (COMPOSER-12)
            <button type="button" className={`composer-button composer-send is-always is-ready${stopPending ? ' is-stopping' : ''}`} aria-label={stopPending ? 'Stopping…' : 'Stop'} title={stopPending ? 'Stopping…' : undefined}
              disabled={stopPending} onMouseDown={(e) => e.preventDefault()} onClick={() => { askStop(); onStop?.(); }}>
              <Square size={13} strokeWidth={1.8} fill="currentColor" />
            </button>
          ) : (
            <button type="submit" className={`composer-button composer-send is-always${ready || attached.length ? ' is-ready' : ''}`} aria-label="Send" disabled={!ready && !attached.length}>
              <ArrowUp size={18} strokeWidth={1.8} />
            </button>
          )}
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
  const askable = useAskable();
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
        {askable.length > 0 && <button type="button" role="menuitem" tabIndex={-1} data-view="oby" onClick={() => onView('oby')}><ObyIcon size={16} />Add other Obys<kbd className="add-hint">@</kbd></button>}
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
  const obys = askableFor(q, askable);
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
          onKeyDown={(e) => { if (!composing(e) && e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); pickFirst(); } }}
          placeholder={hint}
          aria-label={hint}
        />
      </div>
      {/* (folders: several can be on, so a list of several) */}
      <div ref={list} className={`add-list${view === 'folder' ? ' is-folders' : ''}`} role="listbox" aria-multiselectable={view === 'folder' || undefined} onScroll={onScroll}>
        {view === 'oby' && obys.map((o) => (
          <button type="button" role="option" aria-selected={isNamed(o.name)} key={o.name} onClick={() => onPickOby(o.name)}>
            <OwnerAvatar name={o.card ?? o.name} size={22} />
            <ObyRow oby={o} />
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

// An Oby in the @ list or + › Add other Obys: whose it is, by their card's name, their owner id beside
// it when that differs, and, for a member whose owner id looks like another's, who it isn't (ASK-38),
// as the ASK-34 Confirm says it. A mention picked is always the owner id
function ObyRow({ oby }: { oby: Askable }) {
  const about = aboutOf(oby);
  return <span className="add-item-text">{oby.card ?? oby.name}’s Oby{about ? <span className="add-item-about">{about}</span> : null}</span>;
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

// Each account's use, over the box like the + panel, as Settings › Accounts has it: its agent
// and name, when its 5-hour session starts over and how much of it is used on one line, the
// session's bar under them (yellow past 80%, red past 90%), and the week in words under that
// (an account with no session figure: the bar is what it has, as in Settings › Accounts); a gear at the top right goes to
// Settings › Accounts, and takes the focus on opening. Esc, a click outside or Tab out of it
// closes it.
export function UsagePanel({ anchor, onClose, onBlur }: { anchor?: React.RefObject<HTMLButtonElement | null>; onClose: () => void; onBlur: (e: React.FocusEvent) => void }) {
  const [accounts] = useAccounts();
  // as wide as the account menu, its right edge on the gauge's and MENU_GAP over it, as that menu
  // sits on its pill (COMPOSER-18); placed in the box (.composer-field), which holds it
  const panel = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<React.CSSProperties>({ width: MENU_WIDTH });
  useLayoutEffect(() => {
    const field = panel.current?.closest('.composer-field'), button = anchor?.current?.getBoundingClientRect(), box = field?.getBoundingClientRect();
    // inside the Mac window, its 420px least too, as the account menu keeps (TalkPill)
    const width = Math.min(MENU_WIDTH, innerWidth - 24);
    if (!field || !button || !box) return setPlace({ width });
    // right and bottom are from inside the box's border
    const edge = getComputedStyle(field), [right, bottom] = [parseFloat(edge.borderRightWidth) || 0, parseFloat(edge.borderBottomWidth) || 0];
    setPlace({ width, right: box.right - right - button.right, bottom: box.bottom - bottom - button.top + MENU_GAP });
  }, [anchor]);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(lookAtUsage, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  return (
    <div ref={panel} className="add-panel usage-panel" role="dialog" aria-label="Usage" style={place} onBlur={onBlur}>
      <div className="usage-head">
        <span>Usage</span>
        <button type="button" autoFocus className="icon-button" aria-label="Manage accounts" title="Manage accounts" onClick={() => { onClose(); dispatchEvent(new CustomEvent('oby:open-setting', { detail: everything ? 'accounts' : 'agents' })); }}>
          <Settings size={15} strokeWidth={1.6} />
        </button>
      </div>
      {!accounts.length && <p className="add-empty">No accounts yet</p>}
      {accounts.map((a) => {
        const week = weekWords(a), bar = a.signedIn === false ? null : barOf(a);
        return (
          <div key={a.id} className="usage-row">
            <BrandMark brand={agentOf(a.agent).brand} size={16} mono />
            <span className="usage-text">
              <span className="usage-line">
                <span className="usage-name">{agentOf(a.agent).name} · {a.name}</span>
                {bar != null && <>
                  {a.sessionResets && <span className="usage-about">Resets {a.sessionResets}</span>}
                  <span className={`usage-used ${fullness(bar)}`.trim()}>{bar}%</span>
                </>}
              </span>
              {bar != null && <span className="set-meter" role="meter" aria-valuenow={bar} aria-valuemin={0} aria-valuemax={100} aria-label={a.session != null ? `${bar}% of the 5-hour session used` : `${bar}% used`}>
                <span className={fullness(bar)} style={{ width: `${bar}%` }} />
              </span>}
              <span className="usage-week">{week}</span>
              {a.chatgpt && a.signedIn && <span className="usage-week">{USING_PLAN} · <ManageUsage /></span>}
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
