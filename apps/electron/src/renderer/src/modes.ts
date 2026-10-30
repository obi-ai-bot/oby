// How freely a job's agent works, each with its line (RunnerMode in his app): Settings' Default
// mode, a task's Mode menu and its page (jobPage.ts, which stays pure for its tests: no imports here)
export const MODES = [
  { id: 'ask', label: 'Ask', help: 'Asks before it edits a file or runs a command' },
  { id: 'acceptEdits', label: 'Accept edits', help: 'Edits files without asking; asks before commands' },
  { id: 'plan', label: 'Plan', help: 'Plans first, and works once you approve the plan' },
  { id: 'bypass', label: 'Bypass', help: 'Asks nothing' },
];
// obid's fifth, its default before #1570: shown only where an install or a job still has it, never
// offered (his design has four)
export const SANDBOX = { id: 'sandbox', label: 'Sandbox', help: 'Asks only before it reaches the network, secrets or files outside its worktree' };
export const modeLabel = (id: string) => [...MODES, SANDBOX].find((m) => m.id === id)?.label ?? id;
