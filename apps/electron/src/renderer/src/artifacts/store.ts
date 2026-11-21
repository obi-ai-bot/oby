import { useEffect, useState, useSyncExternalStore } from 'react';
import { api, type Artifact } from '../obid';

// The artifact open beside its conversation (docs/contracts/artifacts-app.md, ARTIFACTSAPP-4): one at
// a time, as LibreChat's panel is; null when none. The window's own state, never obid's.

export type Open = { artifact: Artifact; version: number } | null;
let open: Open = null;
const listeners = new Set<() => void>();
const changed = () => { for (const l of listeners) l(); };

export const openArtifact = (artifact: Artifact, version = artifact.versions.at(-1)?.version ?? 1) => { open = { artifact, version }; changed(); };
export const closeArtifact = () => { if (open) { open = null; changed(); } };
export const useOpenArtifact = () => useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => open);

// What the card and the pane say a kind is (Sangam's board 1b: "Oby artifact · Canvas · v3")
const KINDS: Record<string, string> = { html: 'Page', markdown: 'Document', image: 'Picture', pdf: 'PDF', canvas: 'Canvas', design: 'Canvas' };
export const kindName = (kind: string) => KINDS[kind] ?? 'File';

/** A conversation's artifacts as obid lists them, read again whenever `tick` changes (a task ended,
 * a turn came); none until they're read, and none for a conversation obid doesn't keep. */
export function useConversationArtifacts(conversation: string | null, tick: string): Artifact[] {
  const [list, setList] = useState<{ of: string | null; artifacts: Artifact[] }>({ of: null, artifacts: [] });
  useEffect(() => {
    if (!conversation) return;
    let gone = false;
    api.artifacts({ conversation }).then((artifacts) => { if (!gone) setList({ of: conversation, artifacts }); }, () => {});
    return () => { gone = true; };
  }, [conversation, tick]);
  return list.of === conversation ? list.artifacts : [];
}
