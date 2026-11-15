import { Menu } from './Menu';
import { deleteTask, letGo, remove, stop, tryAgain, type Task } from './tasks';

// A task's menu, from its side panel row (⋮ or right-click) or its page's title
// bar: its page (where it isn't already showing), its conversation, rename;
// failed or interrupted and not let go, Try again or Stop here (TASKS-25);
// then stop it while it's going, or archive it once it's done (out of Home,
// into Archive); last, set apart, delete it. A status this build doesn't know offers
// none of these (docs/contracts/wire.md, WIRE-3).
export default function TaskMenu({ task, at, align = 'right', onClose, onViewDetails, onOpenChat, onRename }: {
  task: Task;
  at: DOMRect;
  align?: 'left' | 'right';
  onClose: () => void;
  onViewDetails?: () => void;  // left out on the task's own page
  onOpenChat: () => void;
  onRename: () => void;
}) {
  const pick = (fn: () => void) => () => { fn(); onClose(); };
  const again = !task.unknown && !!task.job.canTryAgain;
  return (
    <Menu at={at} align={align} onClose={onClose} height={again ? 244 : 180} width={168}>
      {onViewDetails && <button role="menuitem" onClick={pick(onViewDetails)}>View details</button>}
      <button role="menuitem" onClick={pick(onOpenChat)}>Open conversation</button>
      <button role="menuitem" onClick={pick(onRename)}>Rename</button>
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
