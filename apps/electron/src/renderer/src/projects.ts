import { useEffect, useSyncExternalStore } from 'react';
import { api, type Project } from './obid';
import { MOCK_PROJECTS } from './mock';
import { onObid } from './lean';
import { onSynced, takesProjects, useLive } from './live';

// The projects (docs/contracts/projects.md, PROJECTS-16 to 32): the side panel's groups, and the chip
// that picks one in Home's box and moves a conversation in its title bar. obid's list (GET
// /v1/projects), only once its status lists projects-v1, or the mock's. A read that fails leaves the
// list as it was, with nothing shown as an error. The app keeps none of it.
let list: Project[] = onObid ? [] : MOCK_PROJECTS;
const listeners = new Set<() => void>();
const set = (next: Project[]) => { list = next; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useProjects = () => useSyncExternalStore(subscribe, () => list);

// obid's list again: once its status says it has projects, each time main synced with obid (the
// window opened, or came back to an obid that was away or restarted), each time a chip's menu opens,
// and after a create, a change or a delete (PROJECTS-32). One read at a time: one asked meanwhile
// runs right after it. A read that fails leaves the list as it was and is tried again, 1 s later,
// then 2, 4, 8 and every 10 s, until one is read or projects no longer show
let read = false;
let reading = false;
let again = false;
let tries = 0;
let retrying: ReturnType<typeof setTimeout> | undefined;
export async function loadProjects(): Promise<void> {
  if (!onObid || !takesProjects()) return;
  read = true;
  if (reading) { again = true; return; }
  clearTimeout(retrying);
  reading = true;
  let ok = false;
  try { set(await api.projects()); ok = true; } catch { /* as it was */ }
  reading = false;
  if (ok) tries = 0;
  if (again) { again = false; return loadProjects(); }
  if (!ok) retrying = setTimeout(() => void loadProjects(), Math.min(10_000, 1000 * 2 ** tries++));
}
if (onObid) onSynced(() => void loadProjects());
// Whether projects show: on the mock, or on an obid with projects-v1 (the first time it says so,
// its list is read, if nothing read it yet)
export function useProjectsOn() {
  const on = !onObid || takesProjects(useLive());
  useEffect(() => { if (on && onObid && !read) void loadProjects(); }, [on]);
  return on;
}

// A new project: obid makes it (its refusal, an ApiError, goes to the sheet as it said it), or on the
// mock, the mock; then it's listed
export type NewProject = Omit<Project, 'id'>;
export async function createProject(body: NewProject) {
  const made = onObid ? await api.createProject(body) : { id: `pr${Date.now().toString(36)}`, ...body, name: body.name.trim() };
  set([...list.filter((p) => p.id !== made.id), made]);
  void loadProjects();
  return made;
}

// What changed in a project: only the keys whose value differs (the name trimmed; folders and
// repositories in their order), none when nothing did, since obid refuses an empty change
export type ProjectChange = Partial<NewProject>;
export function changeOf(was: Project, now: NewProject): ProjectChange {
  const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);
  return {
    ...(now.name.trim() !== was.name ? { name: now.name.trim() } : {}),
    ...(!same(now.folders, was.folders) ? { folders: now.folders } : {}),
    ...(!same(now.repos, was.repos) ? { repos: now.repos } : {}),
  };
}
// A change to a project (PATCH /v1/projects/{id}, PROJECTS-3): obid answers it as kept (its refusal
// goes to the sheet), or on the mock, the mock; it takes the old one's place in the list. Its
// conversations get its new folders from obid's own conversation events
export async function editProject(was: Project, change: ProjectChange) {
  const kept = onObid ? await api.editProject(was.id, change) : { ...was, ...change };
  set(list.map((p) => (p.id === kept.id ? kept : p)));
  void loadProjects();
  return kept;
}
// A project gone (DELETE /v1/projects/{id}, PROJECTS-8): its conversations stay, out of it, with its
// folders as their own (obid's conversation events say so); it leaves the list
export async function deleteProject(gone: Project) {
  if (onObid) await api.deleteProject(gone.id);
  set(list.filter((p) => p.id !== gone.id));
  void loadProjects();
}

// "1 repo", "3 repos"; none: nothing
export const repoCount = (p: Project) => (p.repos.length ? `${p.repos.length} repo${p.repos.length === 1 ? '' : 's'}` : '');
