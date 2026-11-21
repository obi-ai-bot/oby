import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { api, type Artifact } from '../obid';
import { onObid } from '../lean';
import type { ArtifactEvent } from '../../../shared/obid-wire';

// The artifact open beside its conversation (docs/contracts/artifacts-app.md, ARTIFACTSAPP-4 to 7):
// one at a time, as LibreChat's panel is; null when none. The window's own state, never obid's.
//
// It follows the agent as LibreChat's panel does: open on the latest version, a new version of it
// moves it there (ARTIFACTSAPP-6), and an artifact the agent makes in the conversation on view opens
// by itself. The words said while it's open carry it (ARTIFACTSAPP-7, openArtifactOf).

export type Tab = 'preview' | 'code';
/** The artifact open: the version on view, the tab, the owner's edits not yet saved (a path of
 * that version and its new text), and why the last Save was refused. */
export type Open = { artifact: Artifact; version: number; tab: Tab; edits: Record<string, string>; refused: string | null } | null;
let open: Open = null;
// the conversation on view (ChatView), whose new artifacts open by themselves
let onView: string | null = null;
// bumped by each artifact event, so the cards read their conversation's list again
let seen = 0;
// the artifacts made or given a version while the window runs and not opened since: their row's New
// dot (ARTIFACTSAPP-17). The window's own, never obid's or kept on disk
let fresh: ReadonlySet<string> = new Set();
const listeners = new Set<() => void>();
const changed = () => { for (const l of listeners) l(); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };

const latestOf = (a: Artifact) => a.versions.at(-1)?.version ?? 1;
export const openArtifact = (artifact: Artifact, version = latestOf(artifact)) => {
  const same = open?.artifact.id === artifact.id;
  if (fresh.has(artifact.id)) fresh = new Set([...fresh].filter((id) => id !== artifact.id));
  open = { artifact, version, tab: same ? open!.tab : 'preview', edits: same && open!.version === version ? open!.edits : {}, refused: null };
  changed();
};
/** An artifact from a dropped file's bytes (ARTIFACTSAPP-12): open beside the conversation once
 * obid keeps it; why not, when it doesn't. */
export async function importFrom(bytes: Uint8Array): Promise<string | null> {
  try {
    openArtifact(await api.importArtifact(bytes));
    return null;
  } catch (e) {
    return (e as Error).message;
  }
}
/** Another version on view (the version menu): the edits on the one before go. */
export const showVersion = (version: number) => { if (open && open.version !== version) { open = { ...open, version, edits: {}, refused: null }; changed(); } };
/** Whether the version on view is the latest: only the latest is edited (ARTIFACTSAPP-9). */
export const onLatest = (o: NonNullable<Open>) => o.version === latestOf(o.artifact);
/** The owner's new text for one file of the version on view; the same as obid's (`kept`) unedits it. */
export function edit(path: string, text: string, kept: string) {
  if (!open || !onLatest(open)) return;
  const edits = { ...open.edits };
  if (text === kept) delete edits[path]; else edits[path] = text;
  open = { ...open, edits, refused: null };
  changed();
}
/** Save (ARTIFACTSAPP-8): the edits as the owner's edit of the version on view, signed by main on
 * the click of `Save as v<n>`. Done, the pane is on the new version; refused, it says why and the
 * edits stay (ARTIFACTSAPP-10). */
export async function save(): Promise<void> {
  const o = open;
  if (!o || !Object.keys(o.edits).length) return;
  try {
    const made = await api.saveArtifact(o.artifact.id, o.version, o.edits);
    const artifact = await api.artifact(o.artifact.id);
    if (open?.artifact.id !== o.artifact.id) return;
    open = { ...open, artifact, version: made.version, edits: {}, refused: null };
  } catch (e) {
    if (open?.artifact.id !== o.artifact.id) return;
    open = { ...open, refused: (e as Error).message };
  }
  changed();
}
export const closeArtifact = () => { if (open) { open = null; changed(); } };
/** A conversation left: the artifact open beside it goes, unless it's the one now on view's (a row
 * opened that conversation and its artifact together, ARTIFACTSAPP-17). */
export const leftConversation = () => { if (!open?.artifact.conversation || open.artifact.conversation !== onView) closeArtifact(); };
export const showTab = (tab: Tab) => { if (open && open.tab !== tab) { open = { ...open, tab }; changed(); } };
export const useOpenArtifact = () => useSyncExternalStore(subscribe, () => open);
/** The artifact events seen so far: a conversation's cards read their list again as it grows. */
export const useArtifactsSeen = () => useSyncExternalStore(subscribe, () => seen);
/** The artifacts with a New dot (ARTIFACTSAPP-17). */
export const useNewArtifacts = () => useSyncExternalStore(subscribe, () => fresh);

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
  const mine = open?.artifact.id === e.id;
  // made, or given a version, while it isn't the one the owner is looking at: New until opened
  if (typeof e.version === 'number' && !e.renamed && !mine) fresh = new Set([...fresh, e.id]);
  changed();
  const made = !open && e.version === 1 && !e.renamed && !!e.conversation && e.conversation === onView;
  if (!mine && !made) return;
  void api.artifact(e.id).then((artifact) => {
    if (open?.artifact.id === e.id) {
      // the version on view stays where it is unless a newer one came while it was on the latest
      // with nothing of the owner's unsaved on it: their edits are never swept away (a Save then
      // says a version came after, ARTIFACTSAPP-10)
      const edited = Object.keys(open.edits).length > 0;
      const move = onLatest(open) && !edited;
      const came = edited && onLatest(open) && latestOf(artifact) > open.version;
      open = { ...open, artifact, version: move ? latestOf(artifact) : open.version, refused: came ? `Version ${latestOf(artifact)} came after the one you’re editing.` : open.refused };
      changed();
    } else if (!open && artifact.conversation === onView) {
      openArtifact(artifact);
    }
  }, () => {});
}

// What the card, the pane and the row say a kind is (Sangam's boards 1b and Main: "Oby artifact · Canvas · v3")
const KINDS: Record<string, string> = { html: 'Page', markdown: 'Document', image: 'Image', pdf: 'PDF', canvas: 'Canvas', design: 'Canvas' };
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

/** Every artifact obid keeps, by the conversation it's in, for the side panel's rows (ARTIFACTSAPP-17):
 * one read of the whole list, again at each artifact event, on obid only. A read that fails keeps the
 * last list. */
export function useArtifactsByConversation(): ReadonlyMap<string, Artifact[]> {
  const [all, setAll] = useState<Artifact[]>([]);
  const events = useArtifactsSeen();
  useEffect(() => {
    if (!onObid) return;
    let gone = false;
    api.artifacts().then((artifacts) => { if (!gone) setAll(artifacts); }, () => {});
    return () => { gone = true; };
  }, [events]);
  return useMemo(() => {
    const by = new Map<string, Artifact[]>();
    for (const a of all) if (a.conversation) by.set(a.conversation, [...(by.get(a.conversation) ?? []), a]);
    return by;
  }, [all]);
}
