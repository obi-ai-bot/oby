// A sharing rule as obid answers it (GET /v1/obid/sharing), in the words Settings lists it with
export type SharingRule = { id: string; for: '*' | string[]; repos: '*' | string[]; tells: string[]; uses: number; starter?: boolean };
const TELLS: Record<string, { label: string; what: string }> = {
  jobStatus: { label: 'How your tasks are going', what: 'Each task’s title, repository, stage, state and pull request' },
  jobProgress: { label: 'What each task is doing', what: 'Its current step and latest report line, in an agent’s own words, leaving out one with a possible secret' },
  sharedResults: { label: 'What it already sent them', what: 'The results of reviews they asked for' },
};
// members: the others in the Obysphere, since the room goes to all of them or not at all
const toEveryone = (rule: SharingRule) => rule.for === '*' && rule.repos === '*' && rule.tells.includes('jobStatus');
// Whether Settings offers the room: status is shared, and no rule opens the room yet
export const canOpenRoom = (rules: SharingRule[]) => rules.some(toEveryone) && !rules.some((r) => toEveryone(r) && !r.starter);

// Whether Settings offers each task's step and report line: the same card a first question brings
export const canAddProgress = (rules: SharingRule[]) => rules.some(toEveryone) && !rules.some((r) => r.tells.includes('jobProgress'));

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
