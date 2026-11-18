import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import { Group, Row } from './SettingsView';
import { withStarterRule } from '../../shared/policy';
import { INVITE_DAYS, INVITE_USES } from '../../shared/invite';
import type { Invitation, InvitesAnswer } from '../../shared/sphere';

// Invite teammates, in Settings › Obysphere, for its admin only (docs/contracts/sphere-app.md,
// SPHEREAPP-10): Make a link (5 people, 14 days), the link with Copy, then the open links with
// what's left of each and Withdraw. Main makes, lists and withdraws them with the owner's own token
// (main/sphere.ts), making and withdrawing only right after the owner's click; the page holds only
// the link it was handed. Who's admin is the Sphere's word, as the rules' read brings it.

// How a link stands, in few words: uses left, and until when
export function standing(i: Invitation) {
  const until = new Date(i.expiresAt).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return { label: `${i.maxUses - i.uses} of ${i.maxUses} ${i.maxUses === 1 ? 'use' : 'uses'} left`, about: `Until ${until}` };
}
const HANDOFFS = 'Members can ask, request reviews and hand off tasks.';

export default function InviteTeammates() {
  const [admin, setAdmin] = useState(false);
  const [handoffs, setHandoffs] = useState(false);
  const [open, setOpen] = useState<Invitation[]>([]);
  const [made, setMade] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const take = (a: InvitesAnswer | null) => {
    if (!a) return setProblem('Oby couldn’t reach your Obysphere.');
    if (a.refused) return setProblem(a.why);
    setOpen(a.invitations);
    setProblem(null);
    if (a.link) setMade(a.link);
    if (a.handoffs !== undefined) setHandoffs(a.handoffs);
  };
  useEffect(() => {
    let gone = false;
    void window.app?.spherePolicy?.().then((p) => {
      if (gone || !p?.admin) return;
      setAdmin(true);
      setHandoffs(!!p.policy && !withStarterRule(p.policy.rules));
      void window.app?.sphereInvitations?.().then((a) => { if (!gone) take(a); }, () => {});
    }, () => {});
    return () => { gone = true; };
  }, []);
  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(t);
  }, [copied]);
  if (!admin) return null;
  const make = async () => {
    setBusy(true);
    take(await window.app!.sphereInvite!().catch(() => null));
    setBusy(false);
  };
  const copy = () => void navigator.clipboard?.writeText(made!).then(() => setCopied(true), () => setProblem('Oby couldn’t copy it. Select it and copy it yourself.'));
  const withdraw = (id: string) => void window.app!.sphereWithdraw!(id).then(take, () => take(null));
  return (
    <Group i={2} title="Invite teammates" footer={`Anyone with a link can join until it runs out.${handoffs ? ` ${HANDOFFS}` : ''}`}>
      <Row label="New link" about={`For ${INVITE_USES} people, ${INVITE_DAYS} days`}>
        <button className="set-button set-member-action" disabled={busy} onClick={() => void make()}>{busy ? 'Making…' : 'Make a link'}</button>
      </Row>
      {made && (
        <Row label={<span className="invite-made">{made}</span>}>
          <button className="set-button set-member-action" onClick={copy} aria-label="Copy the link">{copied ? <><Check size={12} strokeWidth={2} />Copied</> : 'Copy'}</button>
        </Row>
      )}
      {open.map((i) => {
        const { label, about } = standing(i);
        return (
          <Row key={i.id} label={label} about={about}>
            <button className="set-button set-member-action" onClick={() => withdraw(i.id)} aria-label={`Withdraw the link with ${label}`}>Withdraw</button>
          </Row>
        );
      })}
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
    </Group>
  );
}
