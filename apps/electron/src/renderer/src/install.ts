import { useEffect, useState } from 'react';
import { api, type Status } from './obid';
import type { Agent } from './view';

// Each coding agent's own install command, the vendor's, shown on first run's Tools step when obid
// doesn't find its CLI (docs/contracts/first-run.md, FIRSTRUN-2): the owner copies it and runs it in
// their own Terminal; Oby runs nothing. None needs Node or npm.
export const INSTALL: Record<Agent, string> = {
  claude: 'curl -fsSL https://claude.ai/install.sh | bash',
  codex: 'curl -fsSL https://chatgpt.com/codex/install.sh | sh',
  cursor: 'curl https://cursor.com/install -fsS | bash',
};

// How often first run asks obid again while an agent's CLI isn't found: obid looks for it at every
// status, so one installed meanwhile shows up (FIRSTRUN-3)
export const LOOK_AGAIN_MS = 3000;

// The agents whose CLI obid doesn't find (`tools[]`, its `path` null), asked again every few seconds
// while there are any. An obid that sends no `tools` (an older one) finds every one, as before
export function useMissing(status: Status | null | undefined): Set<string> {
  const [fresh, setFresh] = useState<Status['tools']>();
  const tools = fresh ?? status?.tools;
  const missing = new Set((tools ?? []).filter((t) => !t.path).map((t) => t.agent));
  const any = missing.size > 0;
  useEffect(() => {
    if (!any) return;
    const t = setInterval(() => { api.status().then((s) => setFresh(s.tools), () => { /* asked again next time */ }); }, LOOK_AGAIN_MS);
    return () => clearInterval(t);
  }, [any]);
  return missing;
}
