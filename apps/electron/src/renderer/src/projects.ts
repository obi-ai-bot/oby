import { useEffect, useSyncExternalStore } from 'react';
import { api, type Project } from './obid';
import { JOBS, MOCK_PROJECTS } from './mock';
import { onObid } from './lean';
import { takesProjects, useLive } from './live';

// The projects (docs/contracts/projects.md, PROJECTS-16 to 24): the side panel's groups, and the chip
// that picks one in Home's box and moves a conversation in its title bar. obid's list (GET
// /v1/projects), only once its status lists projects-v1, or the mock's. A read that fails leaves the
// list as it was, with nothing shown as an error. The app keeps none of it.
let list: Project[] = onObid ? [] : MOCK_PROJECTS;
const listeners = new Set<() => void>();
const set = (next: Project[]) => { list = next; listeners.forEach((l) => l()); };
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const useProjects = () => useSyncExternalStore(subscribe, () => list);

// obid's list again: once its status says it has projects, each time a chip's menu opens, and after
// a create
export async function loadProjects() {
  if (!onObid || !takesProjects()) return;
  try { set(await api.projects()); } catch { /* as it was */ }
}
// Whether projects show: on the mock, or on an obid with projects-v1 (the first time it says so,
// its list is read)
let read = false;
export function useProjectsOn() {
  const on = !onObid || takesProjects(useLive());
  useEffect(() => { if (on && onObid && !read) { read = true; void loadProjects(); } }, [on]);
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

// The repositories a project can take from GitHub: obid's (GET /v1/repos), or the mock's tasks'
export async function reposToAdd(): Promise<string[]> {
  if (!onObid) return [...new Set(JOBS.map((j) => j.repo).filter((r) => r.includes('/')))];
  return (await api.repos()).repos.map((r) => r.repo);
}

// "1 repo", "3 repos"; none: nothing
export const repoCount = (p: Project) => (p.repos.length ? `${p.repos.length} repo${p.repos.length === 1 ? '' : 's'}` : '');
