import { useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown, Folder, Pencil, Plus, X } from 'lucide-react';
import { Menu } from './Menu';
import { folderName, pickFolders } from './folders';
import { changeOf, createProject, deleteProject, editProject, loadProjects, repoCount, reposToAdd, useProjects } from './projects';
import type { Project } from './obid';

// The project chip (screens 01 and 03): in Home's box, the project the words start a conversation in;
// in a conversation's title bar, the one it's in. Its menu lists each project with its repositories'
// count, No project, then Edit project… (with one chosen: the sheet, filled in with it) and New
// project…, which opens the sheet: a project made there is chosen
export function ProjectChip({ chosen, onChoose, className = '' }: {
  chosen: string | null | undefined;  // a project's id; none (or one no longer listed): No project
  onChoose: (project: string | null) => void;
  className?: string;
}) {
  const projects = useProjects();
  const [menu, setMenu] = useState<DOMRect | null>(null);
  // the sheet: a new project, or the chosen one to change
  const [sheet, setSheet] = useState<'new' | 'edit' | null>(null);
  const shown = projects.find((p) => p.id === chosen);
  const choose = (project: string | null) => { setMenu(null); onChoose(project); };
  return (
    <>
      <button
        type="button"
        className={`project-chip${menu ? ' is-on' : ''} ${className}`.trim()}
        aria-haspopup="menu"
        aria-expanded={!!menu}
        // obid's list again as it opens
        onClick={(e) => { if (!menu) void loadProjects(); setMenu(menu ? null : e.currentTarget.getBoundingClientRect()); }}
      >
        <Folder size={13} strokeWidth={1.7} />
        <span className="project-chip-name">{shown?.name ?? 'No project'}</span>
        <ChevronDown size={12} strokeWidth={1.8} />
      </button>
      {/* its height: a 36px row each, No project, the rule, Edit project…, New project… and the menu's padding */}
      {menu && (
        <Menu at={menu} onClose={() => setMenu(null)} align="left" width={240} height={projects.length * 38 + 100 + (shown ? 38 : 0)}>
          {projects.map((p) => (
            <button key={p.id} type="button" role="menuitemradio" aria-checked={p.id === shown?.id} className="project-choice" onClick={() => choose(p.id)}>
              <Folder size={15} strokeWidth={1.6} />
              <span className="project-choice-name">{p.name}</span>
              <span className="project-choice-repos">{repoCount(p)}</span>
              {p.id === shown?.id && <Check size={14} strokeWidth={1.8} className="menu-check" />}
            </button>
          ))}
          <button type="button" role="menuitemradio" aria-checked={!shown} className="project-choice is-none" onClick={() => choose(null)}>
            <span className="project-choice-name">No project</span>
            {!shown && <Check size={14} strokeWidth={1.8} className="menu-check" />}
          </button>
          <hr />
          {shown && (
            <button type="button" role="menuitem" className="project-choice is-add" onClick={() => { setMenu(null); setSheet('edit'); }}>
              <Pencil size={14} strokeWidth={1.8} />
              Edit project…
            </button>
          )}
          <button type="button" role="menuitem" className="project-choice is-add" onClick={() => { setMenu(null); setSheet('new'); }}>
            <Plus size={14} strokeWidth={1.8} />
            New project…
          </button>
        </Menu>
      )}
      {sheet === 'new' && <ProjectSheet onClose={() => setSheet(null)} onMade={(p) => { setSheet(null); onChoose(p.id); }} />}
      {/* (a change keeps it chosen; a delete takes the conversation out, which obid says itself) */}
      {sheet === 'edit' && shown && <ProjectSheet editing={shown} onClose={() => setSheet(null)} onMade={() => setSheet(null)} />}
    </>
  );
}

// New project (screen 02): its name, then its folders on this Mac (main's own dialog) and its
// repositories (obid's list), each a row with a × to take it out. Sent as made (POST /v1/projects);
// obid's refusal shows in the sheet, which stays as it was. Esc, Cancel or a click outside closes it.
// The same sheet edits a project (PROJECTS-25 to 30): filled in with it, Save sends only what changed
// (PATCH /v1/projects/{id}; nothing changed: it just closes), and Delete project… asks first, then
// sends DELETE (its conversations stay, with its folders as their own). `deleting` opens it at that ask.
export default function ProjectSheet({ editing, deleting = false, onClose, onMade }: {
  editing?: Project;
  deleting?: boolean;
  onClose: () => void;
  onMade: (project: Project) => void;
}) {
  const [name, setName] = useState(editing?.name ?? '');
  const [folders, setFolders] = useState<string[]>(editing?.folders ?? []);
  const [repos, setRepos] = useState<string[]>(editing?.repos ?? []);
  // the ask before a delete (opened at it: Cancel closes the sheet; from the sheet: back to it)
  const [confirming, setConfirming] = useState(deleting);
  // Add from GitHub…'s menu: where it opens, and the repositories it lists
  const [github, setGithub] = useState<{ at: DOMRect; list: string[] } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [making, setMaking] = useState(false);
  const ready = name.trim().length > 0 && !making;
  const addFolders = async () => {
    const picked = await pickFolders();
    if (picked.length) setFolders((f) => [...new Set([...f, ...picked])]);
  };
  const openGithub = async (at: DOMRect) => {
    try { setGithub({ at, list: await reposToAdd() }); } catch (e) { setProblem((e as Error).message); }
  };
  const create = async () => {
    if (!ready) return;
    setMaking(true); setProblem(null);
    try {
      if (!editing) onMade(await createProject({ name: name.trim(), folders, repos }));
      else {
        const change = changeOf(editing, { name, folders, repos });
        onMade(Object.keys(change).length ? await editProject(editing, change) : editing);
      }
    } catch (e) {
      setProblem((e as Error).message);
      setMaking(false);
    }
  };
  const remove = async () => {
    if (!editing || making) return;
    setMaking(true); setProblem(null);
    try {
      await deleteProject(editing);
      onClose();
    } catch (e) {
      setProblem((e as Error).message);
      setMaking(false);
    }
  };

  // the latest onClose (the panel behind re-renders often, as Oby's words stream in); Esc with
  // GitHub's menu open is the menu's
  const close = useRef(onClose);
  close.current = onClose;
  const menuOpen = useRef(false);
  menuOpen.current = !!github;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !menuOpen.current) { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);

  const row = (key: string, shown: string, title: string, where: string, remove: () => void) => (
    <div key={key} className="project-row" title={title}>
      <Folder size={16} strokeWidth={1.6} />
      <span className="project-row-name">{shown}</span>
      <span className="project-row-where">{where}</span>
      <button type="button" className="project-row-remove" aria-label={`Remove ${shown}`} onClick={remove}><X size={14} strokeWidth={1.8} /></button>
    </div>
  );
  // (its clicks and its submit stay its own: a message box it opens from neither takes the focus
  // nor sends with it)
  const backdrop = (sheet: ReactNode) => createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }} onMouseDown={(e) => e.stopPropagation()}>{sheet}</div>,
    document.body,
  );
  if (editing && confirming) return backdrop(
    <form className="modal is-small project-sheet" role="alertdialog" aria-modal="true" aria-labelledby="project-title" aria-describedby="project-what" onSubmit={(e) => { e.preventDefault(); e.stopPropagation(); void remove(); }}>
      <header className="modal-head">
        <h2 id="project-title">Delete {editing.name}?</h2>
      </header>
      <p id="project-what" className="modal-lede">Its conversations stay, working in its folders as their own.</p>
      {problem && <p className="sphere-note is-bad" role="alert">{problem}</p>}
      <div className="modal-actions">
        <button type="button" className="set-button" autoFocus onClick={() => (deleting ? onClose() : (setConfirming(false), setProblem(null)))}>Cancel</button>
        <button type="submit" className="modal-primary is-danger" disabled={making}>Delete project</button>
      </div>
    </form>,
  );
  return backdrop(
      <form className="modal is-small project-sheet" role="dialog" aria-modal="true" aria-labelledby="project-title" onSubmit={(e) => { e.preventDefault(); e.stopPropagation(); void create(); }}>
        <header className="modal-head">
          <h2 id="project-title">{editing ? 'Edit project' : 'New project'}</h2>
        </header>
        <p className="modal-lede">A project is the folders and repositories its conversations work in.<br />It stays in this profile.</p>
        <label className="modal-label" htmlFor="project-name">Name</label>
        <input id="project-name" className="modal-field" autoFocus maxLength={100} value={name} onChange={(e) => setName(e.target.value)} autoComplete="off" spellCheck={false} />
        <span className="modal-label">Folders and repositories</span>
        {folders.map((f) => row(`folder:${f}`, folderName(f), f, 'On this Mac', () => setFolders((all) => all.filter((x) => x !== f))))}
        {repos.map((r) => row(`repo:${r}`, r.split('/').pop() ?? r, r, 'GitHub', () => setRepos((all) => all.filter((x) => x !== r))))}
        <div className="project-adds">
          <button type="button" className="set-button" onClick={() => void addFolders()}>Add a folder…</button>
          <button type="button" className="set-button" aria-haspopup="menu" aria-expanded={!!github} onClick={(e) => (github ? setGithub(null) : void openGithub(e.currentTarget.getBoundingClientRect()))}>Add from GitHub…</button>
        </div>
        {problem && <p className="sphere-note is-bad" role="alert">{problem}</p>}
        <div className="modal-actions">
          {editing && <button type="button" className="set-button is-danger project-delete" onClick={() => { setProblem(null); setConfirming(true); }}>Delete project…</button>}
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="submit" className="modal-primary" disabled={!ready}>{editing ? 'Save' : 'Create project'}</button>
        </div>
        {/* (none listed: it says so, rather than opening nothing) */}
        {github && (
          <Menu at={github.at} onClose={() => setGithub(null)} align="left" width={240} height={Math.max(github.list.length, 1) * 36 + 12}>
            {!github.list.length && <span className="menu-label" role="status">No repositories to add</span>}
            {github.list.map((r) => (
              <button key={r} type="button" role="menuitemcheckbox" aria-checked={repos.includes(r)} onClick={() => { setRepos((all) => (all.includes(r) ? all : [...all, r])); setGithub(null); }}>
                {r}
                {repos.includes(r) && <Check size={14} strokeWidth={1.8} className="menu-check" />}
              </button>
            ))}
          </Menu>
        )}
      </form>,
  );
}
