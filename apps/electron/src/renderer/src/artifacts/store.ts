import { useEffect, useState, useSyncExternalStore } from 'react';
import { api, type Artifact } from '../obid';
import type { ArtifactEvent } from '../../../shared/obid-wire';

// The artifact open beside its conversation (docs/contracts/artifacts-app.md, ARTIFACTSAPP-4 to 7):
// one at a time, as LibreChat's panel is; null when none. The window's own state, never obid's.
//
// It follows the agent as LibreChat's panel does: open on the latest version, a new version of it
// moves it there (ARTIFACTSAPP-6), and an artifact the agent makes in the conversation on view opens
// by itself. The words said while it's open carry it (ARTIFACTSAPP-7, openArtifactOf).

export type Tab = 'preview' | 'code';
export type Open = { artifact: Artifact; version: number; tab: Tab } | null;
let open: Open = null;
// the conversation on view (ChatView), whose new artifacts open by themselves
let onView: string | null = null;
// bumped by each artifact event, so the cards read their conversation's list again
let seen = 0;
const listeners = new Set<() => void>();
const changed = () => { for (const l of listeners) l(); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };

const latestOf = (a: Artifact) => a.versions.at(-1)?.version ?? 1;
export const openArtifact = (artifact: Artifact, version = latestOf(artifact)) => { open = { artifact, version, tab: open?.artifact.id === artifact.id ? open.tab : 'preview' }; changed(); };
export const closeArtifact = () => { if (open) { open = null; changed(); } };
export const showTab = (tab: Tab) => { if (open && open.tab !== tab) { open = { ...open, tab }; changed(); } };
export const useOpenArtifact = () => useSyncExternalStore(subscribe, () => open);
/** The artifact events seen so far: a conversation's cards read their list again as it grows. */
export const useArtifactsSeen = () => useSyncExternalStore(subscribe, () => seen);

/** The conversation on view, or none: an artifact its agent makes opens by itself. */
export const viewing = (conversation: string | null) => { onView = conversation; };

/** The artifact open for words said in `conversation`, with the version on view; none when the
 * artifact open is another conversation's. */
export function openArtifactOf(conversation: string): { id: string; version: number } | null {
  if (!open || (open.artifact.conversation && open.artifact.conversation !== conversation)) return null;
  return { id: open.artifact.id, version: open.version };
}

/** An `artifact` event (shared/obid-wire.ts): the one open moves to its new version, or retitles; a
 * new one made in the conversation on view opens. Read again from obid, never taken from the event. */
export function artifactChanged(e: ArtifactEvent) {
  seen++;
  changed();
  const mine = open?.artifact.id === e.id;
  const made = !open && e.version === 1 && !e.renamed && !!e.conversation && e.conversation === onView;
  if (!mine && !made) return;
  void api.artifact(e.id).then((artifact) => {
    if (open?.artifact.id === e.id) {
      // the version on view stays where it is unless a newer one came (it was on the latest)
      const wasLatest = open.version === latestOf(open.artifact);
      open = { ...open, artifact, version: wasLatest ? latestOf(artifact) : open.version };
      changed();
    } else if (!open && artifact.conversation === onView) {
      openArtifact(artifact);
    }
  }, () => {});
}

// What the card and the pane say a kind is (Sangam's board 1b: "Oby artifact · Canvas · v3")
const KINDS: Record<string, string> = { html: 'Page', markdown: 'Document', image: 'Picture', pdf: 'PDF', canvas: 'Canvas', design: 'Canvas' };
export const kindName = (kind: string) => KINDS[kind] ?? 'File';

/** A conversation's artifacts as obid lists them, read again whenever `tick` changes (a task ended,
 * a turn came, an artifact event); none until they're read, and none for a conversation obid
 * doesn't keep. */
export function useConversationArtifacts(conversation: string | null, tick: string): Artifact[] {
  const [list, setList] = useState<{ of: string | null; artifacts: Artifact[] }>({ of: null, artifacts: [] });
  const events = useArtifactsSeen();
  useEffect(() => {
    if (!conversation) return;
    let gone = false;
    api.artifacts({ conversation }).then((artifacts) => { if (!gone) setList({ of: conversation, artifacts }); }, () => {});
    return () => { gone = true; };
  }, [conversation, tick, events]);
  return list.of === conversation ? list.artifacts : [];
}
