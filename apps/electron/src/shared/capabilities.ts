import type { Status } from './obid-wire';

// What obid serves, by name (docs/contracts/capabilities.md): its status's `capabilities` list,
// and nothing else when it sends one. An obid before R06-01 sends none: then a feature's own older
// key answers (FALLBACK), and a name with none is absent.
const FALLBACK: Record<string, (s: Pick<Status, 'folders'>) => boolean> = {
  'folders-v1': (s) => s.folders === true,
};

export function has(status: Pick<Status, 'capabilities' | 'folders'> | null | undefined, name: string): boolean {
  if (!status) return false;
  if (Array.isArray(status.capabilities)) return status.capabilities.includes(name);
  return FALLBACK[name]?.(status) ?? false;
}
