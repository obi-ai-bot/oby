import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown, Folder, Plus, X } from 'lucide-react';
import { Menu } from './Menu';
import { folderName, pickFolders } from './folders';
import { createProject, loadProjects, repoCount, reposToAdd, useProjects } from './projects';
import type { Project } from './obid';

// The project chip (screens 01 and 03): in Home's box, the project the words start a conversation in;
// in a conversation's title bar, the one it's in. Its menu lists each project with its repositories'
// count, No project, then New project…, which opens the sheet: a project made there is chosen
export function ProjectChip({ chosen, onChoose, className = '' }: {
  chosen: string | null | undefined;  // a project's id; none (or one no longer listed): No project
  onChoose: (project: string | null) => void;
  className?: string;
}) {
  const projects = useProjects();
  const [menu, setMenu] = useState<DOMRect | null>(null);
  const [sheet, setSheet] = useState(false);
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
      {/* its height: a 36px row each, No project, the rule, New project… and the menu's padding */}
      {menu && (
        <Menu at={menu} onClose={() => setMenu(null)} align="left" width={240} height={projects.length * 38 + 100}>
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
          <button type="button" role="menuitem" className="project-choice is-add" onClick={() => { setMenu(null); setSheet(true); }}>
            <Plus size={14} strokeWidth={1.8} />
            New project…
          </button>
        </Menu>
      )}
      {sheet && <ProjectSheet onClose={() => setSheet(false)} onMade={(p) => { setSheet(false); onChoose(p.id); }} />}
    </>
  );
}

// New project (screen 02): its name, then its folders on this Mac (main's own dialog) and its
// repositories (obid's list), each a row with a × to take it out. Sent as made (POST /v1/projects);
// obid's refusal shows in the sheet, which stays as it was. Esc, Cancel or a click outside closes it.
export default function ProjectSheet({ onClose, onMade }: { onClose: () => void; onMade: (project: Project) => void }) {
  const [name, setName] = useState('');
  const [folders, setFolders] = useState<string[]>([]);
  const [repos, setRepos] = useState<string[]>([]);
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
      onMade(await createProject({ name: name.trim(), folders, repos }));
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
  return createPortal(
    // (its clicks and its submit stay its own: a message box it opens from neither takes the focus
    // nor sends with it)
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }} onMouseDown={(e) => e.stopPropagation()}>
      <form className="modal is-small project-sheet" role="dialog" aria-modal="true" aria-labelledby="project-title" onSubmit={(e) => { e.preventDefault(); e.stopPropagation(); void create(); }}>
        <header className="modal-head">
          <h2 id="project-title">New project</h2>
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
          <button type="button" className="set-button" onClick={onClose}>Cancel</button>
          <button type="submit" className="modal-primary" disabled={!ready}>Create project</button>
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
      </form>
    </div>,
    document.body,
  );
}
