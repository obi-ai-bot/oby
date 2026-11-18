import { Menu } from './Menu';
import { deleteTask, letGo, remove, stop, tellPlaceholder, tryAgain, type Task } from './tasks';
import { agentOf } from './view';

// A task's menu, from its side panel row (⋮ or right-click), its page's title
// bar or its card in a conversation (⋯ or right-click): its page (where it isn't
// already showing), its conversation and rename (where it isn't one already);
// on its card, telling its agent something from the message box (LIVETASK-30);
// failed or interrupted and not let go, Try again or Stop here (TASKS-25);
// then stop it while it's going, or archive it once it's done (out of Home,
// into Archive); last, set apart, delete it. A status this build doesn't know offers
// none of these (docs/contracts/wire.md, WIRE-3).
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
  return (
    <Menu at={at} align={align} onClose={onClose} height={again ? 244 : 180} width={168}>
      {onViewDetails && <button role="menuitem" onClick={pick(onViewDetails)}>View details</button>}
      {onOpenChat && <button role="menuitem" onClick={pick(onOpenChat)}>Open conversation</button>}
      {onRename && <button role="menuitem" onClick={pick(onRename)}>Rename</button>}
      {onTell && !task.unknown && <button role="menuitem" onClick={pick(onTell)}>{tellPlaceholder(task, agentOf(task.job.agent).name)}</button>}
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
