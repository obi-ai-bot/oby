import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Mail, X } from 'lucide-react';
import { Group, Row, SettingsPage, Value } from './SettingsView';
import SphereTile from './SphereTile';
import { OwnerAvatar } from './ObyFace';
import { api } from './obid';
import { keyed } from './live';
import type { InboxItem, OwnerPin, RememberedDecision } from '../../shared/obid-wire';
import { notThem } from './askable';
import { plainId } from './peers';
import { acceptPin, canVerify, decisionWords, decisions as readDecisions, forgetDecision, members as readMembers, pins as readPins, toCompare } from './sphereTrust';
import { withStarterRule } from '../../shared/policy';
import { PIN_VERIFY } from '../../shared/words';
import type { PictureAnswer, Policy, PolicyAnswer } from '../../shared/sphere';
import RepoTicks from './RepoTicks';
import { canAddProgress, canChangeRepos, canOpenRoom, confirmIsOff, repoTicks, whatItTells, type SharingRule } from './sharing';
import { roomOpens, useRoom } from './room';
import { ME, SUFFIX, cancelInvite, deleteSphere, memberCount, removeMember, revokedLine, ruleWords, showPicture, trimSlug, slugOf, slugProblem, updateSphere, urlOf, useSpheres, type Sphere } from './spheres';
import { JoinLink, useJoinLink } from './JoinLink';
import InviteTeammates from './InviteTeammates';
import { SoleAdmin, isSoleAdmin, useSphereAdmins } from './SphereAdmins';
import AddSignIn, { needsSignin, useSigninStatus } from './AddSignIn';
import { composing } from './shortcuts';

export const sphereSettingsLabel = (id: string, spheres: Sphere[]) => `${spheres.find((s) => s.id === id)?.name ?? ''} settings`;


// An Obysphere's settings, opened from its ⋮ (Settings), in the view as the
// app's are, under its name ("Deskree settings"); the side panel stays on the
// Obysphere. id: which Obysphere.
export default function SphereSettings({ id, ...frame }: {
  id: string;
  leaving: boolean;
  covered?: boolean;
  onClose: () => void;
  onGone: () => void;
}) {
  const spheres = useSpheres();
  const sphere = spheres.find((s) => s.id === id);
  // its name kept while its page goes, deleted or left (Home takes the page away)
  const title = useRef('');
  if (sphere) title.current = sphereSettingsLabel(id, spheres);
  return (
    <SettingsPage title={title.current} {...frame}>
      {sphere && <Settings key={sphere.id} sphere={sphere} />}
    </SettingsPage>
  );
}

// Its avatar, name and URL (its admins change them; anyone else sees them),
// then deleting it (an admin) or leaving it (anyone else). Each change is kept
// as it's made, as macOS settings are, and says so for a moment ("Saved").
function Settings({ sphere }: { sphere: Sphere }) {
  const { admin } = sphere;
  const file = useRef<HTMLInputElement>(null);
  const [saved, setSaved] = useState<'avatar' | 'name' | 'url' | null>(null);
  useEffect(() => {
    if (!saved) return;
    const t = setTimeout(() => setSaved(null), 2000);
    return () => clearTimeout(t);
  }, [saved]);
  const savedNote = (what: typeof saved) => (saved === what ? <span className="set-saved"><Check size={12} strokeWidth={2} />Saved</span> : undefined);
  const setImage = (image: string | undefined) => {
    if (sphere.image) URL.revokeObjectURL(sphere.image);
    updateSphere(sphere.id, { image });
    setSaved('avatar');
  };
  // the name and URL as they're typed: kept once you leave the field, if
  // they'll do (and changed); else back to what they were
  const [name, setName] = useState(sphere.name);
  const [slug, setSlug] = useState(sphere.slug);
  useEffect(() => setName(sphere.name), [sphere.name]);
  useEffect(() => setSlug(sphere.slug), [sphere.slug]);
  const problem = slug !== sphere.slug ? slugProblem(slug, sphere.id) : null;
  const saveName = () => {
    const n = name.trim();
    if (n.length < 2 || n === sphere.name) setName(sphere.name);
    else { updateSphere(sphere.id, { name: n }); setSaved('name'); }
  };
  const saveSlug = () => {
    const s = trimSlug(slug);
    if (problem || !s || s === sphere.slug) setSlug(sphere.slug);
    else { updateSphere(sphere.id, { slug: s }); setSaved('url'); }
  };
  // Enter keeps it (by leaving the field); Esc puts it back
  const keys = (undo: () => void) => (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (composing(e)) return;
    if (e.key === 'Enter') e.currentTarget.blur();
    if (e.key === 'Escape') undo();
  };
  const [confirming, setConfirming] = useState(false);
  // a live Obysphere's admins, as the Sphere says, with Make admin and Remove admin for its admins (SPHEREAPP-11)
  const live = useSphereAdmins(!!sphere.live && !sphere.revoked);
  const signin = useSigninStatus(!!sphere.live && !sphere.revoked && isSoleAdmin(live.admins, live.me));
  const roleOf = (owner: string) => (live.admins?.includes(owner) ? 'Admin' : 'Member');
  // A live Obysphere's picture is its admins' to change, as the Sphere says (SPHEREAPP-16): main picks
  // the file in its own dialog and sends it; the badge's × asks main to take it away
  const mayPicture = sphere.live ? live.amAdmin && !sphere.revoked : admin;
  const [pictureProblem, setPictureProblem] = useState<string | null>(null);
  const livePicture = (change?: () => Promise<PictureAnswer | null>) => void change?.().then((a) => {
    if (!a) return;
    if (a.refused) return setPictureProblem(a.why);
    if (!showPicture(a)) return;
    setPictureProblem(null);
    setSaved('avatar');
  }, () => setPictureProblem('Oby couldn’t reach your Obysphere.'));
  const pick = () => (sphere.live ? livePicture(window.app?.sphereSetPicture) : file.current?.click());
  const unpick = () => (sphere.live ? livePicture(window.app?.sphereRemovePicture) : setImage(undefined));

  return <>
    {sphere.revoked && <Revoked sphere={sphere} />}
    <Group
      i={1}
      footer={admin ? 'Its URL is in every invite link: a new one stops the old links working.' : 'Only its admins can change these.'}
    >
      {/* the avatar, at the right: a click picks a new image; its badge takes the image out */}
      <Row label="Avatar" about={pictureProblem ? <span className="set-problem" role="alert">{pictureProblem}</span> : savedNote('avatar') ?? (mayPicture ? 'A square image, at least 128px' : undefined)}>
        {mayPicture ? (
          <span className="sphere-logo set-avatar">
            <button className="sphere-logo-pick" aria-label="Change avatar" title="Change avatar" onClick={pick}>
              <SphereTile name={sphere.name} brand={sphere.brand} image={sphere.image} />
            </button>
            {sphere.image && <button className="sphere-logo-badge" aria-label="Remove avatar" title="Remove avatar" onClick={unpick}><X size={12} strokeWidth={2} /></button>}
            <input
              ref={file}
              type="file"
              accept="image/*"
              hidden
              onChange={(e) => {
                const f = e.currentTarget.files?.[0];
                if (f) setImage(URL.createObjectURL(f));
                e.currentTarget.value = '';
              }}
            />
          </span>
        ) : <span className="set-avatar"><SphereTile name={sphere.name} brand={sphere.brand} image={sphere.image} /></span>}
      </Row>
      <Row label="Name" about={savedNote('name')}>
        {admin
          ? <input className="set-field set-wide" value={name} maxLength={40} onChange={(e) => setName(e.target.value)} onBlur={saveName} onKeyDown={keys(() => setName(sphere.name))} aria-label="Name" spellCheck={false} />
          : <Value>{sphere.name}</Value>}
      </Row>
      <Row label="Obysphere URL" about={problem ? <span id="sphere-url-problem" className="set-problem">{problem === 'Taken' ? `${slug}${SUFFIX} is taken` : problem}</span> : savedNote('url')}>
        {admin ? (
          <label className={`set-field set-address${problem ? ' is-bad' : ''}`}>
            <input value={slug} maxLength={30} onChange={(e) => setSlug(slugOf(e.target.value))} onBlur={saveSlug} onKeyDown={keys(() => setSlug(sphere.slug))} aria-label="Obysphere URL" aria-invalid={!!problem || undefined} aria-describedby={problem ? 'sphere-url-problem' : undefined} spellCheck={false} />
            <span className="sphere-suffix">{SUFFIX}</span>
          </label>
        ) : <Value>{urlOf(sphere)}</Value>}
      </Row>
    </Group>
    {/* who's in it: you first, then the others, then who's been invited; an
        admin can take someone out (Remove) or an invite back (Cancel invite) */}
    <Group i={2} title={`Members · ${memberCount(sphere)}`}>
      {isSoleAdmin(live.admins, live.me) && <SoleAdmin sphere={sphere.name} unsigned={needsSignin(signin.status)} />}
      <div className="set-row set-member">
        {/* a live Obysphere's: only what obid knows, your owner id (no name or address yet) */}
        <OwnerAvatar name={sphere.live ? sphere.me ?? 'You' : ME.name} size={28} badge={false} />
        {sphere.live
          ? <span className="set-label">{sphere.me ? `${sphere.me} (you)` : 'You'}</span>
          : <span className="set-label">{ME.name} (you)<span className="set-about">{ME.email}</span></span>}
        <span className="set-role">{sphere.live && live.me ? roleOf(live.me) : admin ? 'Admin' : 'Member'}</span>
      </div>
      {sphere.members.map((m) => (
        <div key={m} className="set-row set-member">
          <OwnerAvatar name={m} size={28} badge={false} />
          <span className="set-label">{m}{!sphere.live && <span className="set-about">{m.toLowerCase()}@{sphere.domain ?? 'deskree.com'}</span>}</span>
          <span className="set-role">{sphere.live ? roleOf(m) : 'Member'}</span>
          {admin && <button className="set-button is-danger set-member-action" onClick={() => removeMember(sphere.id, m)} aria-label={`Remove ${m}`}>Remove</button>}
          {live.amAdmin && (live.admins!.includes(m)
            ? <button className="set-button set-member-action" onClick={() => live.set(m, false)} aria-label={`Remove ${m} as admin`}>Remove admin</button>
            : <button className="set-button set-member-action" onClick={() => live.set(m, true)} aria-label={`Make ${m} an admin`}>Make admin</button>)}
        </div>
      ))}
      {live.problem && <Row label={<span className="set-problem" role="alert">{live.problem}</span>} />}
      {sphere.invited.map((e) => (
        <div key={e} className="set-row set-member">
          <span className="invited-mark set-invited"><Mail size={14} strokeWidth={1.8} /></span>
          <span className="set-label is-muted">{e}</span>
          <span className="set-role">Invited</span>
          {admin && <button className="set-button set-member-action" onClick={() => cancelInvite(sphere.id, e)} aria-label={`Cancel invite to ${e}`}>Cancel invite</button>}
        </div>
      ))}
    </Group>
    {sphere.live && !sphere.revoked && window.app?.spherePolicy && <SphereRules />}
    {sphere.live && !sphere.revoked && <WhatItTells sphere={sphere} />}
    {sphere.live && !sphere.revoked && <MembersKeys />}
    {sphere.live && !sphere.revoked && <WithoutAsking />}
    {/* last, set apart: what it does, then its button under it at the left */}
    <Group i={7}>
      <div className="set-row set-danger">
        <span className="set-label">
          {admin ? 'Delete Obysphere' : `Leave ${sphere.name}`}
          <span className="set-about">{admin ? `Deletes ${sphere.name} for all ${memberCount(sphere)} members. You can’t undo this.` : 'You’ll lose access until someone invites you back.'}</span>
        </span>
        <button className="set-button is-danger" onClick={() => setConfirming(true)}>{admin ? 'Delete' : 'Leave'}</button>
      </div>
    </Group>
    {confirming && <ConfirmModal sphere={sphere} onClose={() => setConfirming(false)} />}
  </>;
}

// A live Obysphere whose admin took this Oby out (SPHEREAPP-5): said where it is, with a new link's
// field to come back; Leave, at the foot, lets it go
function Revoked({ sphere }: { sphere: Sphere }) {
  const link = useJoinLink();
  return (
    <Group i={0} title="You’re not in it any more" footer="To come back, ask its admin for a new invite link.">
      <Row label={<span className="set-problem" role="status">{revokedLine(sphere)}</span>} />
      <div className="set-row"><JoinLink link={link} /></div>
    </Group>
  );
}

// Settings › Obysphere (SPHEREAPP-4): the one you're in, by name and who you are in it, or the
// field for an invite link. A revoked one says so (SPHEREAPP-5), with the field to come back.
export function SphereJoinPage() {
  const live = useSpheres().find((s) => s.live);
  const link = useJoinLink();
  // Add sign-in, so this owner can get back in by themselves (SPHEREAPP-13)
  const signin = useSigninStatus(!!live && !live.revoked);
  if (live && !live.revoked) {
    return <>
      <Group i={1} footer="Its settings are under its ⋮ in the side panel.">
        <Row label="Your Obysphere"><Value>{live.name}</Value></Row>
        {live.me && <Row label="You’re in it as"><Value>{live.me}</Value></Row>}
      </Group>
      <AddSignIn status={signin.status} onLinked={signin.linked} />
      {/* its admin's links for teammates, as obid says who's admin (SPHEREAPP-10) */}
      {window.app?.sphereInvitations && <InviteTeammates unsigned={needsSignin(signin.status)} />}
    </>;
  }
  return (
    <Group i={1} title="Join an Obysphere" footer="Your Oby can talk to your team’s Obys there. Ask its admin for the invite link.">
      {live?.revoked && <Row label={<span className="set-problem" role="status">{revokedLine(live)}</span>} />}
      <div className="set-row"><JoinLink link={link} /></div>
    </Group>
  );
}

// What your Oby tells the others by itself, from obid's rules, each with Remove. With nothing shared,
// Share opens the card a first question brings, in full, and only its Confirm grants it. With status
// shared and a room there, the room's own card by the same path: its Confirm makes the rule the room
// goes by, and Remove takes it back.
function WhatItTells({ sphere }: { sphere: Sphere }) {
  const room = useRoom();
  const [rules, setRules] = useState<SharingRule[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [card, setCard] = useState<InboxItem | null>(null);
  const change = (what?: () => Promise<unknown>) => void (what?.() ?? Promise.resolve())
    .then(() => api.sharing())
    .then((s) => { setRules(s.rules); setProblem(null); })
    .catch((e: Error) => setProblem(e.message));
  useEffect(() => change(), []);
  const open = (forRoom = false) => void api.offerSharing(forRoom).then(setCard, (e: Error) => setProblem(e.message));
  return (
    <Group i={4} title="What your Oby tells" footer={rules?.length ? 'It answers by itself only from this. Never logs, diffs, prompts or files.' : undefined}>
      {rules?.map((rule) => {
        const { label, about } = whatItTells(rule, sphere.members);
        return (
          <Row key={rule.id} label={label} about={about}>
            {canChangeRepos(rule) && <button className="set-button set-member-action" onClick={() => open()}>Change repositories</button>}
            <button className="set-button set-member-action" onClick={() => change(() => api.removeSharingRule(rule.id))} aria-label={`Remove ${label}`}>Remove</button>
          </Row>
        );
      })}
      {rules?.length === 0 && (
        <Row label="Nothing yet" about="Every question waits for you.">
          <button className="set-button set-member-action" onClick={() => open()}>Share how my tasks are going</button>
        </Row>
      )}
      {rules && canAddProgress(rules) && (
        <Row label="Also tell what each task is doing" about="Its current step and latest report line, when someone asks.">
          <button className="set-button set-member-action" onClick={() => open()}>Tell them too</button>
        </Row>
      )}
      {rules && canOpenRoom(rules) && roomOpens(room, sphere) && (
        <Row label="Also post in the room" about="Everyone there sees when your tasks start and end, without asking.">
          <button className="set-button set-member-action" onClick={() => open(true)}>Post in the room</button>
        </Row>
      )}
      {problem && <Row label={<span className="set-problem">{problem}</span>} />}
      {card && <ShareModal card={card} onClose={() => setCard(null)} onAnswered={() => { setCard(null); change(); }} />}
    </Group>
  );
}

// Each member's owner key as obid pinned it (docs/contracts/obysphere.md, SPHERE-11, 12): its
// fingerprint to compare with that person, out of band (obid never fetches it from them), whether the
// owner did, and who else's id theirs looks like. Verify asks first whether that person read them
// this, and only then tells obid. Proposed words, for Sangam.
export function MembersKeys() {
  const [shown, setShown] = useState<{ pins: OwnerPin[]; alike: Map<string, string[]> } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [asking, setAsking] = useState<OwnerPin | null>(null);
  const read = () => void Promise.all([readPins(), readMembers().catch(() => [])])
    .then(([pins, members]) => { setShown({ pins, alike: new Map(members.map((m) => [m.owner, m.lookalike])) }); setProblem(null); })
    .catch((e: Error) => setProblem(e.message));
  useEffect(read, []);
  return (
    <Group i={5} title="Members’ keys" footer={shown?.pins.length ? 'Compare a fingerprint with its person, in person or on a call, never by a message in the Obysphere.' : undefined}>
      {shown?.pins.map((p) => {
        const alike = shown.alike.get(p.owner) ?? [];
        return (
          <Row key={p.owner} label={plainId(p.owner)} about={<>
            <code className="set-fingerprint">{toCompare(p)}</code>
            <span>{p.changedTo ? ' · Their key changed' : p.verified ? ' · Verified' : ' · Not verified'}</span>
            {alike.length > 0 && <span className="set-lookalike" role="note"> · {notThem(alike.map(plainId))}</span>}
          </>}>
            {canVerify(p) && <button className="set-button set-member-action" onClick={() => setAsking(p)} aria-label={`Verify ${plainId(p.owner)}`}>Verify</button>}
          </Row>
        );
      })}
      {shown?.pins.length === 0 && <Row label="No keys yet" about="A member’s key shows here once your Oby has seen it." />}
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
      {asking && <VerifyModal pin={asking} onClose={() => setAsking(null)} onVerified={() => { setAsking(null); read(); }} />}
    </Group>
  );
}

// "Did <owner id> read you this?": the fingerprint, whole, then Verify, which tells obid that person
// read the owner exactly this one, signed by its label as the owner's own (PIN_VERIFY, signed-words.md). Closed with Esc, Cancel, the x or a click outside, it tells obid nothing.
export function VerifyModal({ pin, onClose, onVerified }: { pin: OwnerPin; onClose: () => void; onVerified: () => void }) {
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  const who = plainId(pin.owner), fingerprint = toCompare(pin);
  const verify = () => {
    if (busy) return;
    setBusy(true);
    void acceptPin(pin.owner, fingerprint).then(onVerified, (e: Error) => { setBusy(false); setProblem(e.message); });
  };
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal is-small" role="alertdialog" aria-modal="true" aria-labelledby="verify-title" aria-describedby="verify-what">
        <header className="modal-head">
          <h2 id="verify-title">{`Did ${who} read you this?`}</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <p id="verify-what" className="modal-lede">{`Ask ${who} to read you their key’s fingerprint, in person or on a call. Verify only if it’s this one, character for character.`}</p>
        <p className="modal-lede"><code className="set-fingerprint">{fingerprint}</code></p>
        {problem && <p className="set-problem" role="alert">{problem}</p>}
        <div className="modal-actions">
          <button type="button" className="set-button" autoFocus onClick={onClose}>Cancel</button>
          <button type="button" className="modal-primary" disabled={busy} onClick={verify}>{PIN_VERIFY}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

// What your Oby does for teammates without asking (docs/contracts/obysphere.md, SPHERE-13; #1394):
// obid's remembered decisions, a standing GitHub approval among them, in words, each with Forget.
// Forgotten, the next such request asks the owner again. Proposed words, for Sangam.
export function WithoutAsking() {
  const [kept, setKept] = useState<RememberedDecision[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const change = (what?: () => Promise<unknown>) => void (what?.() ?? Promise.resolve())
    .then(() => readDecisions())
    .then((d) => { setKept(d); setProblem(null); })
    .catch((e: Error) => { setProblem(e.message); void readDecisions().then(setKept, () => {}); });
  useEffect(() => change(), []);
  return (
    <Group i={6} title="What your Oby does without asking" footer={kept?.length ? 'Forget one, and the next such request asks you again.' : undefined}>
      {kept?.map((d) => {
        const { label, about } = decisionWords(d);
        return (
          <Row key={d.id} label={label} about={about}>
            <button className="set-button set-member-action" onClick={() => change(() => forgetDecision(d.id))} aria-label={`Forget ${label}`}>Forget</button>
          </Row>
        );
      })}
      {kept?.length === 0 && <Row label="Nothing yet" about="Every request from another Oby waits for you." />}
      {problem && <Row label={<span className="set-problem" role="alert">{problem}</span>} />}
    </Group>
  );
}

// Who may ask whom: the Obysphere's rules, read once as the page opens, in words and read-only
// (docs/contracts/sphere-policy.md, POLICY-5 and 6; q048). Its admin, as the policy's answer says
// (the store's admin is never true for the live one), gets one button that adds the members rule
// at the version shown; main writes it only right after that click. Each answer, read or write,
// shows the policy it brings; one without keeps what's shown, and its why is a plain-text line.
export function SphereRules() {
  const [shown, setShown] = useState<{ policy: Policy; admin: boolean } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [came, setCame] = useState(false);
  const take = (a: PolicyAnswer | null) => {
    setCame(true);
    if (a?.policy) setShown({ policy: a.policy, admin: a.admin });
    setProblem(a ? a.why : 'Oby couldn’t read the Obysphere’s rules.');
  };
  useEffect(() => void window.app?.spherePolicy?.().then(take, () => take(null)), []);
  if (!came) return null;
  const rules = shown?.policy.rules;
  const letAsk = (version: number) => void window.app?.sphereSetPolicy?.(version).then(take, () => take(null));
  return (
    <Group i={3} title="Who may ask whom" footer={shown && !shown.admin ? 'Only its admins can change these.' : undefined}>
      {rules?.map((rule, n) => {
        const { label, about } = ruleWords(rule);
        return <Row key={n} label={label} about={about} />;
      })}
      {rules?.length === 0 && <Row label="Nobody may ask yet" about="Every question to another member’s Oby is refused." />}
      {shown?.admin && withStarterRule(shown.policy.rules) && (
        <Row label="Members may ask each other’s Obys" about="Questions, reviews and handoffs, in every repository.">
          <button className="set-button set-member-action" onClick={() => letAsk(shown.policy.version)}>Let members ask</button>
        </Row>
      )}
      {problem && <Row label={<span className="set-problem">{problem}</span>} />}
    </Group>
  );
}

// The card, whole, as Home shows it: what its Confirm signs is on screen. Closed with Esc or a click
// outside, it stays open on Home.
export function ShareModal({ card: opened, onClose, onAnswered }: { card: InboxItem; onClose: () => void; onAnswered: () => void }) {
  const [card, setCard] = useState(opened);
  const [hold, setHold] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const ticks = repoTicks(card);
  const answer = (done: Promise<unknown>) => void done.then(onAnswered, (e: Error) => setProblem(e.message));
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal is-small" role="alertdialog" aria-modal="true" aria-labelledby="share-title" aria-describedby="share-what">
        <header className="modal-head">
          <h2 id="share-title">{card.title}</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <p id="share-what" className="modal-lede">{card.text}</p>
        {ticks && <RepoTicks {...ticks} onCard={setCard} onHold={setHold} />}
        {problem && <p className="set-problem">{problem}</p>}
        <div className="modal-actions">
          <button type="button" className="set-button" autoFocus onClick={() => answer(keyed((key) => api.answer(card.id, { answer: 'Cancel' }, key)))}>Cancel</button>
          <button type="button" className="modal-primary" disabled={confirmIsOff(ticks, hold)} onClick={() => answer(api.confirm(card.id))}>Confirm</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

// Before it's deleted (or left): what goes with it, and, to delete, its name
// typed in, so it isn't done by a slip. Esc, Cancel, the x or a click outside
// closes it.
export function ConfirmModal({ sphere, onClose }: { sphere: Sphere; onClose: () => void }) {
  const [typed, setTyped] = useState('');
  const leaving = !sphere.admin;
  const ready = leaving || typed.trim() === sphere.name;
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <form className="modal is-small" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-what" onSubmit={(e) => { e.preventDefault(); if (ready) deleteSphere(sphere.id); }}>
        <header className="modal-head">
          <h2 id="confirm-title">{leaving ? `Leave ${sphere.name}?` : `Delete ${sphere.name}?`}</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <p id="confirm-what" className="modal-lede">
          {leaving
            ? `You’ll lose access to ${sphere.name} and everything its Obys share. To come back, someone has to invite you again.`
            : `This deletes ${sphere.name} for all ${memberCount(sphere)} members, cancels open invites and erases everything its Obys shared. You can’t undo this.`}
        </p>
        {!leaving && <>
          <label className="modal-label" htmlFor="confirm-name">Type <strong>{sphere.name}</strong> to confirm</label>
          <input id="confirm-name" className="modal-field" autoFocus value={typed} onChange={(e) => setTyped(e.target.value)} autoComplete="off" spellCheck={false} />
        </>}
        <div className="modal-actions">
          <button type="button" className="set-button" autoFocus={leaving} onClick={onClose}>Cancel</button>
          <button type="submit" className="modal-primary is-danger" disabled={!ready}>{leaving ? 'Leave' : 'Delete Obysphere'}</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
