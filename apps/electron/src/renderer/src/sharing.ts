// A sharing rule as obid answers it (GET /v1/obid/sharing), in the words Settings lists it with
export type SharingRule = { id: string; for: '*' | string[]; repos: '*' | string[]; tells: string[]; uses: number; starter?: boolean };
const TELLS: Record<string, { label: string; what: string }> = {
  jobStatus: { label: 'How your tasks are going', what: 'Each task’s title, repository, stage, state and pull request' },
  jobProgress: { label: 'What each task is doing', what: 'Its current step and latest report line, in an agent’s own words, leaving out one with a possible secret' },
  sharedResults: { label: 'What it already sent them', what: 'The results of reviews they asked for' },
};
// members: the others in the Obysphere, since the room goes to all of them or not at all
const toEveryone = (rule: SharingRule) => rule.for === '*' && rule.tells.includes('jobStatus');
// Whether Settings offers the room: status is shared, and no rule opens the room yet
export const canOpenRoom = (rules: SharingRule[]) => rules.some(toEveryone) && !rules.some((r) => toEveryone(r) && !r.starter);

// Whether Settings offers each task's step and report line, to a rule of the owner's for every
// repository: a yes to the card with the repositories on it has them already
export const canAddProgress = (rules: SharingRule[]) => rules.some((r) => toEveryone(r) && r.repos === '*') && !rules.some((r) => r.tells.includes('jobProgress'));

// Whether Settings offers to change a rule's repositories: only the one the owner's yes made, through
// the same card and Confirm. A rule they wrote themselves stays as they wrote it (PEER-42).
export const canChangeRepos = (rule: SharingRule) => !!rule.starter && rule.repos !== '*';

// The card's list and what's ticked on it, when it's the card that asks (PEER-38)
export const repoTicks = (item: { confirm?: { action: string; payload: { offered?: string[]; repos?: string[] } | null } | null }) => {
  const payload = item.confirm?.action === 'share_status' ? item.confirm.payload : null;
  return payload?.offered ? { offered: payload.offered, chosen: payload.repos ?? [] } : undefined;
};
// One tick more or less, in the list's order, so the title reads as the list does
export const tickRepo = (offered: string[], ticked: string[], repo: string) => {
  const next = ticked.includes(repo) ? ticked.filter((r) => r !== repo) : [...ticked, repo];
  return offered.filter((r) => next.includes(r));
};

export function whatItTells(rule: SharingRule, members: string[]) {
  const known = rule.tells.map((t) => TELLS[t]).filter(Boolean);
  const who = rule.for === '*' ? 'everyone in this Obysphere who asks' : rule.for.join(', ');
  const where = rule.repos === '*' ? 'every repository' : rule.repos.join(', ');
  // only a rule the owner made themselves, covering everyone, opens the room (docs/contracts/rooms.md)
  const everyone = rule.for === '*' || (members.length > 0 && members.every((m) => rule.for.includes(m)));
  const room = !rule.starter && everyone && rule.tells.includes('jobStatus') ? ' It can post them in the room too.' : '';
  const told = rule.uses > 0 ? ` Answered ${rule.uses === 1 ? 'once' : `${rule.uses} times`}.` : '';
  return {
    label: (known.map((k) => k.label).join(', and ') || 'Something this version doesn’t know') + (room ? ', in the room' : ''),
    about: `${known.map((k) => k.what).join('. ')}. To ${who}, in ${where}.${room}${told}`,
  };
}
