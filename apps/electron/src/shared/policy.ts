// The Obysphere's members rule (docs/contracts/sphere-policy.md), with no Node in it, so main's
// engine (shared/sphere.ts) and the page share its one id.
import type { Rule } from './sphere';

// What lets every member ask every other a question or a review, and hand it a task
// (ask-an-oby.md, handoff.md; SPHEREAPP-1)
export const STARTER_RULE_ID = 'members-ask';
export const STARTER_ACTIONS = ['question', 'review_request', 'handoff'];
export const starterRule = (): Rule => ({ id: STARTER_RULE_ID, from_owner: '*', to_owner: '*', actions: [...STARTER_ACTIONS], repos: ['*'] });

// The rules with the members rule's actions all there, adding only what's missing (SPHEREAPP-1): the
// rule appended when no rule has its id; else that rule with the starter actions it lacks added at
// its end, everything else in it and every other rule as they were. null when nothing is missing.
export function withStarterRule(rules: Rule[]): Rule[] | null {
  const at = rules.findIndex((r) => r.id === STARTER_RULE_ID);
  if (at < 0) return [...rules, starterRule()];
  const had = rules[at].actions;
  const actions = Array.isArray(had) ? had : [];
  const missing = STARTER_ACTIONS.filter((a) => !actions.includes(a));
  if (!missing.length) return null;
  return rules.map((r, i) => (i === at ? { ...r, actions: [...actions, ...missing] } : r));
}
