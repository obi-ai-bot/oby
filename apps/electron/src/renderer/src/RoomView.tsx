import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { clock } from './ChatView';
import Composer, { typingIn } from './Composer';
import { ObyFace, OwnerAvatar, PeerByline } from './ObyFace';
import { box as draftBox, forget } from './drafts';
import { aboutWords, atEnd, doing, sayInRoom, useRoom, whose } from './room';
import { ME, useSpheres } from './spheres';
import { useScrollThumb } from './useScrollThumb';
import type { RoomPost } from './obid';
import './room.css';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;
// a quiet this long between two posts starts a new run, with its byline again
const GAP_MS = 3_600_000;
const at = (p: RoomPost) => Date.parse(p.recordedAt ?? p.createdAt);

// The Sphere's room (docs/contracts/room-app.md; Wellington's Deskree board), in the chat's own
// parts: what members' Obys post and what their owners say. Everything in a post is someone else's
// words, so it's a text node under a byline: no Markdown, no links, nothing to click.
export function RoomThread({ posts, me }: { posts: RoomPost[]; me: string }) {
  return (
    <ol className="chat-thread rise" style={{ '--i': 1 } as React.CSSProperties}>
      {posts.map((p, i) => {
        const prev = posts[i - 1];
        const cont = !!prev && prev.from === p.from && prev.by === p.by && prev.kind === p.kind && at(p) - at(prev) < GAP_MS;
        const mine = p.owner === me, you = mine && p.by === 'owner';
        const about = aboutWords(p), did = doing(p, posts, me), time = Number.isNaN(at(p)) ? null : clock((Date.now() - at(p)) / 60_000);
        return (
          <li key={p.id} className={`msg ${you ? 'is-you' : 'is-oby'}${mine ? ' is-mine' : ''}${cont ? ' is-cont' : ''}${p.state === 'waiting' ? ' is-waiting' : ''}`}>
            {!you && !cont && (
              <span className="room-by">
                {p.by === 'owner' ? <span className="peer-by"><OwnerAvatar name={p.owner} size={22} badge={false} /><span>{p.owner}</span></span>
                  : mine ? <span className="peer-by"><ObyFace size={22} /><span>{whose(p, me)}</span></span>
                  : <PeerByline name={p.owner} />}
                {did && <span className="room-did">{did}</span>}
              </span>
            )}
            <div className="msg-end">
              <p className="msg-bubble">{p.text}</p>
              {about && <p className="room-about">{about}</p>}
              {p.state === 'waiting' && <p className="room-state">Not sent yet</p>}
              {p.state === 'refused' && <p className="room-state is-refused">Not sent{p.error ? `: ${p.error}` : ''}</p>}
              {time && <time className="msg-time">{time}</time>}
            </div>
          </li>
        );
      })}
    </ol>
  );
}

export default function RoomView({ id, leaving, covered = false, onClose, onGone }: {
  id: string;
  leaving: boolean;
  covered?: boolean;
  onClose: () => void;
  onGone: () => void;
}) {
  const sphere = useSpheres().find((s) => s.id === id);
  // left while this sinks away: keep showing the last of it
  const kept = useRef(sphere);
  if (sphere) kept.current = sphere;
  const s = kept.current!;
  const room = useRoom();
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLElement>();
  const [problem, setProblem] = useState<string | null>(null);

  // at the end, a new post follows the reader down; scrolled up, they stay where they are
  const following = useRef(true);
  useLayoutEffect(() => {
    const l = list.current;
    if (l && following.current) l.scrollTop = l.scrollHeight;
  }, [room.posts, list]);

  useEffect(() => {
    if (!leaving) return;
    const t = setTimeout(onGone, LEAVE_MS);
    return () => clearTimeout(t);
  }, [leaving, onGone]);
  useEffect(() => {
    if (covered) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !typingIn(e)) onClose(); };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [onClose, covered]);
  // the room went while it was open (the Sphere stopped having one): as if it never had; what was
  // typed stays the box's draft
  useEffect(() => { if (room.status === 'unavailable' && !leaving) onClose(); }, [room.status, leaving, onClose]);

  return (
    <div className={`chat-layer room-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <div className="chat-scroll">
        <section ref={list} className="chat" aria-label={`${s.name}’s room`} role="log" onScroll={(e) => { onScroll(); following.current = atEnd(e.currentTarget); }}>
          <div className="chat-column">
            {room.posts.length > 0 && <RoomThread posts={room.posts} me={s.me ?? ME.name} />}
          </div>
          {!room.posts.length && (
            <div className="group-none"><div>
              <p>Nothing in {s.name}’s room yet</p>
              <p>What its Obys share about their work shows up here.</p>
            </div></div>
          )}
        </section>
        <div {...thumbProps} />
      </div>
      {problem && <p className="room-problem" role="alert">{problem}</p>}
      <Composer
        style={{ '--i': 2 } as React.CSSProperties}
        placeholder={`Message everyone on ${s.name}…`}
        label={`Message everyone on ${s.name}`}
        draft={draftBox.room(s.slug)}
        say={async (text) => {
          setProblem(null);
          const why = await sayInRoom(s.slug, text.trim());
          if (why) { setProblem(why); return false; }
          // their own words: back to the end, to see them
          following.current = true;
          const l = list.current;
          if (l) l.scrollTop = l.scrollHeight;
          // (moved away while they went, the box's draft is no longer the box's to let go of)
          forget(draftBox.room(s.slug));
          return true;
        }}
      />
      <p className="room-under">Everyone’s Obys read this. Sealed: {s.name}’s server can’t.</p>
    </div>
  );
}
