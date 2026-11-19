import { useState } from 'react';
import { Group, Row, TextField } from './SettingsView';
import { changeSetup, useLive } from './live';
import { onObid } from './lean';
import { MOCK_REPOS } from './mock';
import type { Repo } from './obid';

// Settings › Repositories (docs/contracts/setup.md, SETUP-4): the repositories obid lists (GET
// /v1/repos, the configuration's: those tasks have worked in), each with its Setup command, the one
// line obid runs in each new task's worktree before its agent starts (PUBLISHING-23). Save sends it
// (POST /v1/repos/setup), and this click's signed Confirm keeps it (live.ts, changeSetup); an empty
// field takes it out. Not in Sangam's design yet: ours until his comes, the T3 Code way
// (WorktreeSetupCard). On the mock, the mock's repositories, kept here only.
export const SETUP_HINT = 'Oby runs it on this Mac, as your user, in each new task’s worktree before its agent starts. One line, like npm ci.';

export default function RepositoriesSettings() {
  const live = useLive();
  const [mock, setMock] = useState(MOCK_REPOS);
  const repos = (onObid ? live.repos : mock).filter((r) => r.listed);
  if (!repos.length) {
    return (
      <Group i={1} footer="A repository shows here once a task has worked in it.">
        <Row label={<span className="set-none">No repositories yet</span>} />
      </Group>
    );
  }
  const save = async (repo: string, setup: string | null) => {
    if (onObid) return changeSetup(repo, setup);
    setMock((list) => list.map((r) => (r.repo === repo ? { ...r, setup } : r)));
  };
  return <>{repos.map((r, k) => <RepoGroup key={r.repo} repo={r} i={1 + k} onSave={save} />)}</>;
}

function RepoGroup({ repo: r, i, onSave }: { repo: Repo; i: number; onSave: (repo: string, setup: string | null) => Promise<void> }) {
  const kept = r.setup ?? '';
  // what's typed, until it's saved (or the repository's setup changes under it)
  const [typed, setTyped] = useState<{ from: string; text: string } | null>(null);
  const text = typed && typed.from === kept ? typed.text : kept;
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const changed = text.trim() !== kept;
  const save = async () => {
    setSaving(true);
    setProblem(null);
    try {
      await onSave(r.repo, text.trim() || null);
      setTyped(null);
    } catch (e) {
      setProblem((e as Error).message);
    } finally {
      setSaving(false);
    }
  };
  return (
    <Group i={i} title={r.repo}>
      <Row label="Setup command" about={SETUP_HINT}>
        <TextField wide value={text} onChange={(v) => setTyped({ from: kept, text: v })} placeholder="None" label={`${r.repo} setup command`} />
      </Row>
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
      {changed && (
        <div className="set-row is-action">
          <button className="set-button is-primary" disabled={saving} onClick={() => void save()}>{saving ? 'Saving…' : 'Save'}</button>
          <button className="set-button" disabled={saving} onClick={() => setTyped(null)}>Cancel</button>
        </div>
      )}
    </Group>
  );
}
