import { ApiError, ownerPinOf, plainId, rememberedOf, type OwnerPin, type RememberedDecision, type SphereMember } from './obid';
import { lookAgainAtThreads } from './peers';
import { lookAgainWhole } from './room';

// Whom the owner trusts in their Obysphere, and what their Oby does for teammates without asking
// (docs/contracts/obysphere.md, SPHERE-11 to 13): obid's pinned owner keys, each with the fingerprint
// to compare with that person, and its remembered decisions, each with Forget. obid's routes
// (docs/api/obysphere.md, "Pinned owner keys" and "remembered decisions"), through main as every
// call to obid is (obid.ts), kept here apart from obid.ts's client.
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const bridge = window.app?.obid;
  if (!bridge) throw new ApiError(0, 'Oby’s engine is reached from the Mac app only.');
  const { status, text } = await bridge.call(method, path, body);
  if (status === 0) throw new ApiError(0, text);
  let value: unknown = null;
  try { value = text ? JSON.parse(text) : null; } catch { /* not JSON: said below */ }
  if (status < 200 || status >= 300) {
    const said = (value as { error?: unknown } | null)?.error;
    throw new ApiError(status, typeof said === 'string' ? said : `Oby answered ${status}.`);
  }
  return value as T;
}
const list = <T,>(value: unknown, of: (v: unknown) => T | null) => (Array.isArray(value) ? value.flatMap((v) => of(v) ?? []) : []);
const id = encodeURIComponent;

export const pins = async () => list(await call<unknown>('GET', '/v1/obid/sphere/pins'), ownerPinOf);
// the members, for who looks like whom (ASK-35): typed fields only
export const members = async () => list(await call<unknown>('GET', '/v1/obid/sphere/members'), (v) => {
  const m = v as Partial<SphereMember> | null;
  return typeof m?.owner === 'string' ? { owner: m.owner, lookalike: Array.isArray(m.lookalike) ? m.lookalike.filter((x): x is string => typeof x === 'string') : [] } : null;
});
// The owner said that person read them this fingerprint: obid trusts that key, verified, and reads
// the directory again. Their thread and the room are read again, whole, so `Not verified` goes from
// posts already shown (SPHERE-11).
export async function acceptPin(owner: string, fingerprint: string) {
  await call<unknown>('POST', `/v1/obid/sphere/pins/${id(owner)}`, { fingerprint });
  lookAgainAtThreads();
  lookAgainWhole();
}
export const decisions = async () => list(await call<unknown>('GET', '/v1/obid/decisions'), rememberedOf);
export const forgetDecision = (decision: string) => call<unknown>('DELETE', `/v1/obid/decisions/${id(decision)}`);

// The fingerprint to compare: a changed key's, waiting for the owner's OK, else the pinned one's
export const toCompare = (p: OwnerPin) => p.changedTo ?? p.fingerprint;
// Verify is offered until the owner compared the key they'd now trust
export const canVerify = (p: OwnerPin) => !p.verified || !!p.changedTo;

// A remembered decision in words: who by exact owner id, what, and where. Proposed words, for Sangam.
export function decisionWords(d: RememberedDecision): { label: string; about: string } {
  const who = plainId(d.fromOwner), yes = d.decision === 'accept';
  const on = d.repo === '*' ? ' on any repository' : ` on ${d.repo}`;
  const label = d.type === 'review_request' ? (yes ? `Reviews ${who}’s requests${on}` : `Says no to ${who}’s review requests${on}`)
    : d.type === 'handoff' ? (yes ? `Takes ${who}’s tasks${on}` : `Says no to ${who}’s tasks${on}`)
    : d.type === 'question' ? `Says no to ${who}’s questions${d.repo === '*' ? '' : ` about ${d.repo}`}`
    : d.type === 'host_approval' ? `Approves ${who}’s pull requests on GitHub${on}`
    : `${yes ? 'Says yes' : 'Says no'} to ${who}’s requests${on}`;
  const used = d.uses ? `Used ${d.uses} time${d.uses === 1 ? '' : 's'}` : 'Not used yet';
  return { label, about: d.type === 'host_approval' ? `When your Oby’s review passes and their code owners approved · ${used}` : used };
}
