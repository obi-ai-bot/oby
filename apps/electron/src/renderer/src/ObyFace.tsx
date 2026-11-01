// Oby as a small icon: Sangam's 2D Oby (2026-10-27), a soft square a little
// wider than it's tall, its two short ears rising out of its shoulders, and
// two tall eyes.
// ObyIcon: a line drawing of Oby, for menus (the + panel's Add other Obys).
// ObyFace is filled like Oby itself.
// The outline, as Sangam drew it: 105 × 95, the ears' tips at the top.
export const OBY_PATH = 'M70.6738 1.4132C71.0781 0.099296 72.6571 -0.424821 73.7666 0.386834C76.0714 2.07339 80.0073 4.97625 82.921 7.12762C85.3816 8.94448 88.192 10.212 90.91 11.615C99.2785 15.935 105 24.6647 105 34.7316V68.7316C105 83.091 93.3594 94.7316 79 94.7316H26C11.6406 94.7316 0 83.091 0 68.7316V34.7316C8.34945e-07 24.3936 6.03367 15.4655 14.7715 11.2754C17.9651 9.74397 21.2856 8.39647 24.144 6.30517L32.2334 0.386834C33.3429 -0.424821 34.9219 0.099296 35.3262 1.4132L36.9257 6.61352C37.3129 7.87244 38.476 8.73156 39.7931 8.73156H66.2069C67.524 8.73156 68.6871 7.87245 69.0743 6.61352L70.6738 1.4132Z';
// placed in the 24-unit square: 21 wide at the bottom (1.5 to 22.5 across,
// 3.5 to 22.5 down), so the body sits where the old square one did
export const OBY_PLACE = 'translate(1.5 3.5) scale(0.2)';

export function ObyIcon({ size = 20 }: { size?: number }) {
  // the line drawing: the same outline, a little smaller so its 1.5 stroke fits
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d={OBY_PATH} transform="translate(3 4.85) scale(0.1714)" stroke="currentColor" strokeWidth="8.75" strokeLinejoin="round" />
      <rect x="6.9" y="11" width="2.2" height="4" rx="1.1" fill="currentColor" />
      <rect x="14.3" y="11" width="2.2" height="4" rx="1.1" fill="currentColor" />
    </svg>
  );
}

// badge: the small one on an owner's picture, the same shape a little higher,
// its eyes a little closer together so they still read
export function ObyFace({ size = 24, badge = false }: { size?: number; badge?: boolean }) {
  if (badge) {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" overflow="visible" aria-hidden="true">
        {/* the cutout in the picture under it: the same shape, a little bigger,
            in the window's colour (its width is set in app.css) */}
        <path className="oby-badge-cut" d={OBY_PATH} transform="translate(1.5 2.5) scale(0.2)" strokeLinejoin="round" />
        <path d={OBY_PATH} transform="translate(1.5 2.5) scale(0.2)" fill="#f1f1ee" />
        <rect x="7.4" y="10" width="2.6" height="5" rx="1.3" fill="#0d0d0e" />
        <rect x="14" y="10" width="2.6" height="5" rx="1.3" fill="#0d0d0e" />
      </svg>
    );
  }
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path d={OBY_PATH} transform={OBY_PLACE} fill="#f1f1ee" />
      {/* the eyes, as one (they blink together when Oby stops talking: ObyFaceLive) */}
      <g className="oby-eyes" fill="#0d0d0e">
        <rect x="6" y="10.6" width="2.8" height="5" rx="1.4" />
        <rect x="14.6" y="10.6" width="2.8" height="5" rx="1.4" />
      </g>
    </svg>
  );
}

// Someone else's Oby over the first of its run in a conversation: its
// owner's picture and its name
export function PeerByline({ name }: { name: string }) {
  return (
    <span className="peer-by">
      <OwnerAvatar name={name} size={22} />
      <span>{name}’s Oby</span>
    </span>
  );
}

// Someone else's Oby: its owner's round picture (initials on a colour picked
// from their name, until there are real photos), with a small Oby at the
// bottom right saying it's their Oby, not them. The owner is what tells two
// Obys apart; an Oby this small can't show its own colour or face clearly.
const AVATAR_COLORS = ['#3a5a8c', '#7a4a8c', '#2f7a6a', '#8c5a3a', '#8c3a52', '#5a6a2f', '#3a7a8c', '#6a4a3a'];
// djb2: spreads similar names (Dima, Daniel) over different colours
const hash = (s: string) => [...s].reduce((h, c) => ((h * 33) ^ c.charCodeAt(0)) >>> 0, 5381);
export function OwnerAvatar({ name, size = 28, badge = true }: { name: string; size?: number; badge?: boolean }) {
  const hue = hash(name) % AVATAR_COLORS.length;
  const initials = name.split(/\s+/).map((w) => w[0]).slice(0, 2).join('').toUpperCase();
  return (
    <span className="owner-avatar" style={{ width: size, height: size, background: AVATAR_COLORS[hue] }} aria-hidden="true">
      {initials}
      {badge && <span className="owner-avatar-oby"><ObyFace size={Math.round(size * 0.5)} badge /></span>}
    </span>
  );
}

// Everyone in a conversation, side by side and overlapping: up to 5 pictures,
// then how many more. Only the last carries the Oby badge (they're all Obys).
export function AvatarStack({ names, size = 22, max = 5 }: { names: string[]; size?: number; max?: number }) {
  const shown = names.slice(0, max), more = names.length - shown.length;
  return (
    <span className="avatar-stack" aria-label={names.map((n) => `${n}’s Oby`).join(', ')}>
      {shown.map((n, i) => <OwnerAvatar key={n} name={n} size={size} badge={i === shown.length - 1} />)}
      {more > 0 && <span className="avatar-more" style={{ height: size, minWidth: size }}>+{more}</span>}
    </span>
  );
}
