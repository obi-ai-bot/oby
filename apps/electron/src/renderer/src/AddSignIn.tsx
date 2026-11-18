import { useEffect, useState } from 'react';
import { Group, Row } from './SettingsView';
import type { SigninStatus } from '../../shared/sphere';

// Add sign-in, in Settings › Obysphere (docs/contracts/sphere-app.md, SPHEREAPP-13): link first, the
// owner's decision. Only an owner who added a sign-in, with that same account, gets back in by
// themselves after losing their Mac (SPHEREAPP-12), so it's said plainly and early. Shown where the
// Obysphere offers getting back in. Main signs in and links the account; no email ever shows here.

export function useSigninStatus(on: boolean) {
  const [status, setStatus] = useState<SigninStatus | null>(null);
  useEffect(() => {
    if (!on || !window.app?.sphereSigninStatus) return;
    let gone = false;
    void window.app.sphereSigninStatus().then((s) => { if (!gone) setStatus(s); }, () => {});
    return () => { gone = true; };
  }, [on]);
  return { status, linked: () => setStatus((s) => (s ? { ...s, linked: true } : s)) };
}
// signed in somewhere it could be: the owner still needs to add one
export const needsSignin = (s: SigninStatus | null) => !!s?.providers?.length && s.linked !== true;

export default function AddSignIn({ status, onLinked }: { status: SigninStatus | null; onLinked: () => void }) {
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const provider = status?.providers?.[0];
  if (!provider) return null;
  const add = async () => {
    setBusy(true);
    const why = await window.app!.sphereAddSignin!(provider.id).catch((e: Error) => e.message);
    setBusy(false);
    setProblem(why);
    if (!why) onLinked();
  };
  return (
    <Group i={1} title="Sign-in">
      {status!.linked
        ? <Row label="Signed in as you" about={`If you lose this Mac, sign in with ${provider.name} to get back in.`} />
        : (
          <Row label="Add sign-in" about="Without it, you can’t get back in by yourself if you lose this Mac.">
            <button className="set-button set-member-action" disabled={busy} onClick={() => void add()}>{busy ? 'Signing in…' : `Sign in with ${provider.name}`}</button>
          </Row>
        )}
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
    </Group>
  );
}
