// Where words typed in a conversation go (docs/contracts/live-task.md, LIVETASK-17; the model is
// docs/models/reply_target.qnt). A box is an opened card's (it tells that card's job) or the one at the
// foot (a conversation with Oby: Oby; a task's own thread: its agent; another Oby's: that conversation).
// Only the box decides it: which cards are open or focused never does.
export type Box = { card: string } | { foot: { kind: 'oby' | 'task' | 'peer'; id: string } };
export type Target = { oby: string } | { job: string } | { peer: string };

export function replyTarget(box: { card: string }): { job: string };
export function replyTarget(box: Box): Target;
export function replyTarget(box: Box): Target {
  if ('card' in box) return { job: box.card };
  const { kind, id } = box.foot;
  return kind === 'oby' ? { oby: id } : kind === 'task' ? { job: id } : { peer: id };
}
