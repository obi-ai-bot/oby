// The Obysphere's members rule (docs/contracts/sphere-policy.md), with no Node in it, so main's
// engine (shared/sphere.ts) and the page share its one id.
import type { Rule } from './sphere';

// What lets every member ask every other a question or a review (ask-an-oby.md)
export const STARTER_RULE_ID = 'members-ask';
export const starterRule = (): Rule => ({ id: STARTER_RULE_ID, from_owner: '*', to_owner: '*', actions: ['question', 'review_request'], repos: ['*'] });
