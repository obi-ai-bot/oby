import { useEffect, useState } from 'react';
import { Row } from './SettingsView';

// Who administers the live Obysphere, as the Sphere says (readPolicy's `admins`), and Make admin /
// Remove admin (docs/contracts/sphere-app.md, SPHEREAPP-11): main changes it with the owner's own
// token only right after the owner's click. The sole-admin notice says so, never blocks anything.

// unsigned: this owner hasn't added a sign-in (SPHEREAPP-13), so the notice points there too
export const soleAdminLine = (sphere: string, unsigned = false) =>
  `You’re the only admin of ${sphere}. Make someone else an admin so it never hangs on one Mac.${unsigned ? ' Add sign-in too, so you can get back in yourself.' : ''}`;
// this owner the only admin
export const isSoleAdmin = (admins: string[] | undefined | null, me: string | undefined) => !!me && admins?.length === 1 && admins[0] === me;

export function useSphereAdmins(on: boolean) {
  const [admins, setAdmins] = useState<string[] | null>(null);
  const [me, setMe] = useState<string | undefined>();
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!on || !window.app?.spherePolicy) return;
    let gone = false;
    void window.app.spherePolicy().then((p) => { if (!gone && p?.admins) { setAdmins(p.admins); setMe(p.me); } }, () => {});
    return () => { gone = true; };
  }, [on]);
  const amAdmin = !!me && !!admins?.includes(me);
  const set = (owner: string, admin: boolean) => void window.app?.sphereAdmin?.(owner, admin).then((a) => {
    if (!a) return setProblem('Oby couldn’t reach your Obysphere.');
    if (a.refused) return setProblem(a.why);
    setProblem(null);
    if (a.admins) setAdmins(a.admins);
  }, () => setProblem('Oby couldn’t reach your Obysphere.'));
  return { admins, me, amAdmin, set, problem };
}

// The notice, as a row
export const SoleAdmin = ({ sphere, unsigned = false }: { sphere: string; unsigned?: boolean }) => <Row label={<span className="set-notice" role="note">{soleAdminLine(sphere, unsigned)}</span>} />;
