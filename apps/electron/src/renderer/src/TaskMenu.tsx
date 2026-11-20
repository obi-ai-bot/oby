import { useEffect, useState } from 'react';
import { Menu } from './Menu';
import { deleteTask, letGo, remove, stop, tellPlaceholder, tryAgain, type Task } from './tasks';
import { agentOf } from './view';
import { openChanges } from './ChangesPanel';
import { api } from './obid';
import { onObid } from './lean';
import { undoJob } from './live';
import { bridge as terminalBridge, openTerminal } from './terminal/store';

// A task's menu, from its side panel row (⋮ or right-click), its page's title
// bar or its card in a conversation (⋯ or right-click): its page (where it isn't
// already showing), its conversation and rename (where it isn't one already);
// on its card, telling its agent something from the message box (LIVETASK-30);
// with a worktree on disk, its Changes and the worktree in the owner's Terminal or
// editor, and Undo this round… while obid has a sentence for it (docs/contracts/
// task-changes.md, CHANGES-4, 6 and 14); failed or interrupted and not let go, Try
// again or Stop here (TASKS-25); then stop it while it's going, or archive it once
// it's done (out of Home, into Archive); last, set apart, delete it. A status this
// build doesn't know offers none of these (docs/contracts/wire.md, WIRE-3).
export default function TaskMenu({ task, at, align = 'right', onClose, onViewDetails, onOpenChat, onRename, onTell }: {
  task: Task;
  at: DOMRect;
  align?: 'left' | 'right';
  onClose: () => void;
  onViewDetails?: () => void;  // left out on the task's own page
  onOpenChat?: () => void;  // left out on a card in a conversation, which is that conversation
  onRename?: () => void;    // left out on a card in a conversation: no rename in place there
  onTell?: () => void;      // on a card in a conversation: the message box talks to its agent
}) {
  const pick = (fn: () => void) => () => { fn(); onClose(); };
  const again = !task.unknown && !!task.job.canTryAgain;
  const tree = !task.unknown && !!task.job.worktree;
  const [editor, setEditor] = useState<string | null>(null);
  const [undo, setUndo] = useState<string | null>(null);
  useEffect(() => {
    if (!tree || !onObid) return;
    let live = true;
    void window.app?.editorName?.().then((name) => { if (live) setEditor(name); });
    api.undoAction(task.id).then((u) => { if (live) setUndo(u.action); }, () => {});
    return () => { live = false; };
  }, [task.id, tree]);
  const open = (where: 'terminal' | 'editor') => () => { void window.app?.openWorktree?.(task.id, where); onClose(); };
  // Terminal: the owner's shell in its worktree, in Oby's own panel (docs/contracts/terminal.md)
  const shell = tree && onObid && !!terminalBridge();
  const extra = (tree ? 32 * ((editor ? 3 : 2) + (shell ? 1 : 0)) + 9 : 0) + (undo ? 32 : 0) + (onTell ? 32 : 0);
  return (
    <Menu at={at} align={align} onClose={onClose} height={(again ? 244 : 180) + extra} width={tree ? 188 : 168}>
      {onViewDetails && <button role="menuitem" onClick={pick(onViewDetails)}>View details</button>}
      {onOpenChat && <button role="menuitem" onClick={pick(onOpenChat)}>Open conversation</button>}
      {onRename && <button role="menuitem" onClick={pick(onRename)}>Rename</button>}
      {onTell && !task.unknown && <button role="menuitem" onClick={pick(onTell)}>{tellPlaceholder(task, agentOf(task.job.agent).name)}</button>}
      {tree && <>
        <button role="menuitem" onClick={pick(() => openChanges(task.id))}>Changes</button>
        {shell && <button role="menuitem" onClick={pick(() => openTerminal('task', task.id, task.title))}>Terminal</button>}
        <button role="menuitem" onClick={open('terminal')}>Open in Terminal</button>
        {editor && <button role="menuitem" onClick={open('editor')}>Open in {editor}</button>}
        {undo && <button role="menuitem" onClick={pick(() => void undoJob(task.id, undo))}>Undo this round…</button>}
        <hr />
      </>}
      {again && <>
        <button role="menuitem" onClick={pick(() => tryAgain(task.id))}>Try again</button>
        <button role="menuitem" onClick={pick(() => letGo(task.id))}>Stop here</button>
      </>}
      {task.unknown ? null : task.status === 'done'
        ? <button role="menuitem" onClick={pick(() => remove(task.id))}>Archive task</button>
        : <button role="menuitem" onClick={pick(() => stop(task.id))}>Stop task</button>}
      <hr />
      <button role="menuitem" className="is-danger" onClick={pick(() => deleteTask(task.id))}>Delete task</button>
    </Menu>
  );
}
