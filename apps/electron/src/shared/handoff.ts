// A task handed to a member's Oby (docs/contracts/handoff.md, sent from the app: sphere-app.md,
// SPHEREAPP-6), with no Node in it, so the page checks the form as it's filled and main builds
// the one body it sends from the same rules: the spec's handoff schema (obi-sphere actions/
// handoff.schema.json) for what the app sends: one line of goal, the repository as owner/name,
// the branch to start from, and one condition, that the tests pass.

export type HandoffAsk = { to: string; goal: string; repo: string; base: string };

// no control or direction characters (the spec's line_text)
// eslint-disable-next-line no-control-regex
const LINE = /^[^\u0000-\u001F\u007F‪-‮⁦-⁩]*$/;
const REPO = /^[A-Za-z0-9][A-Za-z0-9-]{0,38}\/[A-Za-z0-9_][A-Za-z0-9._-]{0,99}$/;
const BRANCH = /^[A-Za-z0-9_][A-Za-z0-9._/-]{0,199}$/;
const BAD_BRANCH = /\.\.|\/\/|\/$|\.$|\.lock$|\/\./;
export const GOAL_MAX = 200;

// What's wrong with it, in words for under the form, or null when it can go
export function handoffProblem(a: Omit<HandoffAsk, 'to'>): string | null {
  const goal = a.goal.trim(), repo = a.repo.trim(), base = a.base.trim();
  if (!goal) return 'Say what to do.';
  if (!LINE.test(goal) || [...goal].length > GOAL_MAX) return `What to do fits on one line of ${GOAL_MAX} characters.`;
  if (!REPO.test(repo)) return 'The repository is owner/name, as on GitHub.';
  if (!BRANCH.test(base) || BAD_BRANCH.test(base)) return 'That isn’t a branch name.';
  return null;
}

// The handoff's body as POST /v1/obid/sphere/requests takes it, or null for one that can't go
export function handoffBody(a: Omit<HandoffAsk, 'to'>) {
  if (handoffProblem(a)) return null;
  return { goal: a.goal.trim(), repo: a.repo.trim(), base_branch: a.base.trim(), acceptance: [{ kind: 'tests_pass' }] };
}

// How long it stays open for them to answer: a week
export const HANDOFF_OPEN = 7 * 86_400;
