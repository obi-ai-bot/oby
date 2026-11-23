import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Copy, X } from 'lucide-react';
import { useScrollThumb } from './useScrollThumb';
import EmailField, { useEmails } from './EmailField';
import type { InvitesAnswer } from '../../shared/sphere';

// Inviting people into an Obysphere by email (EmailField), then Send
// invitation. Under it, the other way in: a link to pass on yourself, copied
// with the icon in its field, which can be held to the org's own email
// addresses. Send invitation, at the foot; sent, it says so and closes.
// Esc, the x or a click outside closes it.
// On the live Obysphere (docs/contracts/sphere-app.md, SPHEREAPP-20 to 24), for its admins: the
// link is the one main made on Invite member's click (`made`, SPHEREAPP-10's), copied as it is, and
// Send invitation has main open a new email in the person's own mail app (the Obysphere sends no
// mail); no domain toggle, since this app can't sign a joiner in for one. The mock's stays as it was.
const code = () => Math.random().toString(36).slice(2, 8);
const UNREACHED = 'Oby couldn’t reach your Obysphere.';

// Invite member shows on the live Obysphere only to its admins: the Sphere's word, as the rules'
// read brings it (as Invite teammates reads it); nothing asked while `on` is false
export function useLiveAdmin(on: boolean, sphere: string) {
  const [admin, setAdmin] = useState(false);
  useEffect(() => {
    setAdmin(false);
    if (!on || !window.app?.spherePolicy) return;
    let gone = false;
    void window.app.spherePolicy().then((p) => { if (!gone) setAdmin(!!p?.admin); }, () => {});
    return () => { gone = true; };
  }, [on, sphere]);
  return on && admin;
}

export default function InviteModal({ sphere, domain, made, onSent, onClose }: {
  sphere: { name: string; url: string };
  domain?: string;  // the org's email domain: the link can be held to it
  made?: Promise<InvitesAnswer | null>;  // the live Obysphere's: the link main is making on Invite member's click
  onSent: (emails: string[]) => void;  // who was asked in
  onClose: () => void;
}) {
  const live = !!made;
  const [mock] = useState(() => `${sphere.url}/join/${code()}`);
  // the live link, or why there's none (in words), or neither while main makes it
  const [real, setReal] = useState<{ link: string | null; why: string | null }>({ link: null, why: null });
  const link = live ? real.link : mock;
  const [problem, setProblem] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [copied, setCopied] = useState(false);
  const [onlyDomain, setOnlyDomain] = useState(true);
  const list = useEmails();
  const [sent, setSent] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  // the app's scroll thumb, for the card (in a short window)
  const card = useScrollThumb<HTMLFormElement>();
  const ready = list.all.length > 0 && list.valid && !sent && !!link && !sending;
  useEffect(() => {
    if (!made) return;
    let gone = false;
    const take = (a: InvitesAnswer | null) => {
      if (gone) return;
      setReal(!a ? { link: null, why: UNREACHED } : a.link && !a.refused ? { link: a.link, why: null } : { link: null, why: a.why ?? UNREACHED });
    };
    made.then(take, () => take(null));
    return () => { gone = true; };
  }, [made]);

  // the latest onClose (the panel behind re-renders often, as Oby's words stream in)
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    input.current?.focus();
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopImmediatePropagation(); close.current(); } };
    addEventListener('keydown', onKey, true);
    return () => removeEventListener('keydown', onKey, true);
  }, []);
  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(t);
  }, [copied]);
  // sent: a moment to read it, then it closes
  useEffect(() => {
    if (!sent) return;
    const t = setTimeout(() => close.current(), 1200);
    return () => clearTimeout(t);
  }, [sent]);

  const send = () => {
    if (!ready) return;
    list.take(list.draft);
    if (!live) { onSent(list.all); setSent(true); return; }
    // main opens the draft, only right after this click or Enter, and only for the link it made
    const asked = window.app?.sphereInviteMail?.(list.all, link!);
    if (!asked) return setProblem('Oby couldn’t open your mail app. Copy the link and send it yourself.');
    setSending(true);
    setProblem(null);
    asked.then((why) => { setSending(false); if (why) setProblem(why); else setSent(true); },
      () => { setSending(false); setProblem('Oby couldn’t open your mail app. Copy the link and send it yourself.'); });
  };

  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <form ref={card.list} onScroll={card.onScroll} className="modal" role="dialog" aria-modal="true" aria-labelledby="invite-title" onSubmit={(e) => { e.preventDefault(); send(); }}>
        <header className="modal-head">
          <h2 id="invite-title">Invite to {sphere.name}’s Obysphere</h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={onClose}><X size={16} strokeWidth={1.6} /></button>
        </header>
        <label className="modal-label" htmlFor="invite-emails">Email</label>
        <EmailField list={list} id="invite-emails" inputRef={input} onEnterEmpty={send} />

        <span className="modal-label invite-link-label">Invite link</span>
        <div className="invite-link">
          {real.why
            ? <span className="invite-url is-problem" role="alert">{real.why}</span>
            : <span className="invite-url">{link ?? 'Making a link…'}</span>}
          <button
            type="button"
            className="icon-button"
            aria-label={copied ? 'Copied' : 'Copy link'}
            disabled={!link}
            onClick={() => { navigator.clipboard?.writeText(live ? link! : `https://${link}`).then(() => setCopied(true), () => {}); }}
          >
            {copied ? <Check size={16} strokeWidth={1.8} /> : <Copy size={16} strokeWidth={1.6} />}
          </button>
        </div>
        {/* held to the org's addresses, where it has one */}
        {domain && !live && (
          <div className="invite-option">
            <span>Only allow @{domain} emails to join</span>
            <button type="button" role="switch" aria-checked={onlyDomain} aria-label={`Only allow @${domain} emails to join with the link`} className="set-switch" onClick={() => setOnlyDomain(!onlyDomain)} />
          </div>
        )}
        {problem && <p className="modal-problem set-problem" role="alert">{problem}</p>}
        <div className="modal-actions">
          <button type="submit" className="modal-primary" disabled={!ready && !sent}>
            {sent ? <><Check size={14} strokeWidth={2} />{live ? 'Draft opened' : 'Sent'}</> : 'Send invitation'}
          </button>
        </div>
      </form>
      <div {...card.thumbProps} />
    </div>,
    document.body,
  );
}
