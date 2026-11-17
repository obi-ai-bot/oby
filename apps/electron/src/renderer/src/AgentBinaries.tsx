import { useState } from 'react';
import { api, type Status } from './obid';
import { onObid } from './lean';
import { patchStatus } from './live';
import { Group } from './SettingsView';
import { isAgent } from '../../shared/obid-wire';
import { agentOf } from './view';

// Where each tool runs from (docs/contracts/accounts.md, ACCOUNTS-26 to 28): the binary obid runs
// it as, a path from / or a command's name, set here instead of in obid's config.json. obid keeps
// it beside the accounts, refuses one it can't find, and Use default takes it back to the
// configuration's. Under it, where obid found it and its version. Words proposed: Sangam's to
// rewrite (his design has no screen for this).

type Tool = NonNullable<Status['tools']>[number];
// the command each tool runs as when nothing names another (obid's `Tool::row().bin`)
const DEFAULT_BIN: Record<string, string> = { claude: 'claude', codex: 'codex', cursor: 'cursor-agent' };

// what's under a tool's row: where obid found it and its version, or that it didn't
export const foundWords = (t?: Tool) => (!t ? undefined : t.path ? `Found at ${t.path}${t.version ? ` · ${t.version}` : ''}` : 'Not found on this Mac');

export function ToolBinary({ tool, info, onSaved }: { tool: string; info?: Tool; onSaved: (t: Tool) => void }) {
  const name = agentOf(tool).name;
  const [text, setText] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const shown = text ?? info?.bin ?? '';
  const changed = text !== null && text.trim() !== (info?.bin ?? '');
  const save = async (bin: string | null) => {
    setSaving(true);
    setProblem(null);
    try {
      const now = await api.setAgentBin(tool, bin);
      onSaved({ ...info, ...now, versionStatus: info?.versionStatus });
      setText(null);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };
  const found = foundWords(info);
  return (
    <div className="set-row set-bin">
      <span className="set-label">
        {name}
        {problem ? <span className="set-about set-problem" role="alert">{problem}</span> : found && <span className="set-about" title={found}>{found}</span>}
      </span>
      <span className="set-control">
        <input className="set-field" value={shown} placeholder={DEFAULT_BIN[tool] ?? tool} aria-label={`${name} binary`} title={shown} spellCheck={false} disabled={saving}
          onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && changed && shown.trim()) void save(shown.trim()); if (e.key === 'Escape') setText(null); }} />
        {changed && shown.trim()
          ? <button className="set-button" disabled={saving} onClick={() => void save(shown.trim())}>Save</button>
          : info?.binSet && <button className="set-button" disabled={saving} onClick={() => void save(null)}>Use default</button>}
      </span>
    </div>
  );
}

// One row per tool that has an account, after the accounts' cards: on obid only (the mock has no
// binaries), and only from an obid that says what each runs as (`tools[].bin`)
export default function AgentBinaries({ agents, tools, i }: { agents: string[]; tools?: Status['tools']; i: number }) {
  const shown = agents.filter((a) => isAgent(a) && tools?.some((t) => t.agent === a && t.bin !== undefined));
  if (!onObid || !shown.length) return null;
  const saved = (t: Tool) => patchStatus({ tools: (tools ?? []).map((x) => (x.agent === t.agent ? t : x)) });
  return (
    <Group i={i} title="Binaries" footer="The program each tool runs as: a path from /, or a command’s name on your PATH.">
      {shown.map((a) => <ToolBinary key={a} tool={a} info={tools?.find((t) => t.agent === a)} onSaved={saved} />)}
    </Group>
  );
}
