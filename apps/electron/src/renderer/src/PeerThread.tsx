import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import Composer, { typingIn } from './Composer';
import { box as draftBox } from './drafts';
import { askFromBox, boxShut, lineUnder, nextQuiet, openThread, useThreads, whoseOby, withWords } from './peers';
import { atEnd } from './room';
import { RoomThread } from './RoomView';
import { useSpheres } from './spheres';
import { useScrollThumb } from './useScrollThumb';
import './room.css';

// how long the way out takes (the sink animation in app.css)
const LEAVE_MS = 220;

// A member's thread (docs/contracts/peer-thread.md; the canvas's Peer board), in the room's parts:
// everything their Oby and yours said to each other, as obid has it, and a box that asks. Every word
// from them is a text node under a byline (RoomThread); a word is yours only from this Mac's Oby.
export default function PeerThreadView({ owner, leaving, covered = false, onClose, onGone }: {
  owner: string;
  leaving: boolean;
  covered?: boolean;
  onClose: () => void;
  onGone: () => void;
}) {
  const held = useThreads().get(owner) ?? {};
  const t = held.thread;
  const sphere = useSpheres().find((s) => s.live);
  const posts = useMemo(() => withWords(t?.posts ?? []), [t]);
  const who = whoseOby(t?.name ?? owner, owner);
  const { list, onScroll, thumbProps } = useScrollThumb<HTMLElement>();
  const [tick, setTick] = useState(0);
  // this view's box sent them: it keeps what's typed meanwhile. Another's (the owner clicked away and
  // back): no box until they've gone or been refused, when its draft says what's left
  const sentHere = useRef(false);

  useEffect(() => { openThread(owner); }, [owner]);
  // an open question goes quiet with no event: the clock's next word
  useEffect(() => {
    const at = nextQuiet(t?.posts ?? [], Date.now());
    if (at === Infinity) return;
    const timer = setTimeout(() => setTick((n) => n + 1), at - Date.now());
    return () => clearTimeout(timer);
  }, [t, tick]);

  // at the end, a new post follows the reader down; scrolled up, they stay where they are
  const following = useRef(true);
  useLayoutEffect(() => {
    const l = list.current;
    if (l && following.current) l.scrollTop = l.scrollHeight;
  }, [posts, list]);

  useEffect(() => {
    if (!leaving) return;
    const timer = setTimeout(onGone, LEAVE_MS);
    return () => clearTimeout(timer);
  }, [leaving, onGone]);
  useEffect(() => {
    if (covered) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !typingIn(e)) onClose(); };
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [onClose, covered]);

  const shut = held.gone ?? (t ? boxShut(t) : null);
  const waiting = held.sending !== undefined && !sentHere.current;

  return (
    <div className={`chat-layer room-layer${leaving ? ' is-leaving' : ''}${covered ? ' is-covered' : ''}`} inert={covered}>
      <div className="chat-scroll">
        <section ref={list} className="chat" aria-label={who} role="log" onScroll={(e) => { onScroll(); following.current = atEnd(e.currentTarget); }}>
          <div className="chat-column">
            {posts.length > 0 && <RoomThread posts={posts} obi={sphere?.obi} standing={(i) => lineUnder(posts, i, owner, Date.now())} />}
          </div>
          {t && !posts.length && (
            <div className="group-none"><div>
              <p>Nothing with {who} yet</p>
              <p>What you and it ask each other shows up here.</p>
            </div></div>
          )}
        </section>
        <div {...thumbProps} />
      </div>
      {held.problem && <p className="room-problem" role="alert">{held.problem}</p>}
      {shut || waiting ? <p className="room-under">{shut ?? `Asking ${who}…`}</p> : (
        <Composer
          style={{ '--i': 2 } as React.CSSProperties}
          placeholder={`Ask ${who}…`}
          label={`Ask ${who}`}
          draft={draftBox.member(sphere?.slug ?? '', owner)}
          say={async (text) => {
            sentHere.current = true;
            const went = await askFromBox(owner, text, draftBox.member(sphere?.slug ?? '', owner));
            sentHere.current = false;
            if (!went) return false;
            // their own words: back to the end, to see them
            following.current = true;
            const l = list.current;
            if (l) l.scrollTop = l.scrollHeight;
            return true;
          }}
        />
      )}
    </div>
  );
}
