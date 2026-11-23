// The computer picker's shapes and words (docs/contracts/computers-app.md; the approved design,
// docs/design/computer-picker/README.md). The shapes follow the design's "What obid provides"
// (GET /v1/computers and its events, proposed there): obid owns the list, the window shows it.

/** How a route reaches a computer: the LAN, a tailnet (Tailscale or another), a VPN, a public address, or SSH. */
export type RouteKind = 'lan' | 'tailnet' | 'vpn' | 'public' | 'ssh';

/** connected: a route answers; reconnecting: moving to another route; lost: no route answers; idle: not in use. */
export type ComputerState = 'connected' | 'reconnecting' | 'lost' | 'idle';

/** One route obid tried when none answered: its kind, what it is (an address; none for SSH) and why it failed. */
export type Tried = { route: RouteKind; what?: string; why: string };

export type Computer = {
  id: string;
  /** the name its Oby gives (its host name), or the one Rename… gave it on this Mac */
  name: string;
  os: 'mac' | 'linux';
  state: ComputerState;
  /** the route in use */
  route?: { kind: RouteKind; address?: string };
  /** the route it moved from while reconnecting (obid's computer.route "moved") */
  from?: RouteKind;
  /** every route tried, once none answers (obid's computer.route "lost") */
  tried?: Tried[];
};

/** What obid says the window's computers are: the one in use ("this" for this Mac) and the others. */
export type ComputerList = { current: 'this' | string; computers: Computer[] };

/** Adding over SSH: its three steps, each doing or done, as obid reports them (computer.adding). */
export type AddStep = 'ssh' | 'pair' | 'routes';
export type AddProgress = { step: AddStep; state: 'doing' | 'done' };

/** A link read by obid: the other computer's name, its certificate's fingerprint, and whether it goes with a code (a link of version 1) or adds alone (a one link, COMPUTERS-29). */
export type LinkRead = { name: string; fingerprint: string; needsCode: boolean };

/** This computer's link, for another Mac to add it (screen 05, one link by the owner's word of 2026-11-23). */
export type Invitation = {
  name: string;
  /** the six digits of a link of version 1; a one link has none */
  code?: string;
  link: string;
  fingerprint: string;
  /** the addresses another computer can reach this one at, each with its switch */
  reach: { kind: RouteKind; address: string; on: boolean }[];
  /** when the link ends, in ms since the epoch */
  expiresAt: number;
};

export const THIS_MAC = 'This Mac';

// ---- the words, as the owner approved them ----

/** A route's name on a row's second line and in Home's list of what was tried. */
export const ROUTE_NAME: Record<RouteKind, string> = { lan: 'Local network', tailnet: 'Tailscale', vpn: 'VPN', public: 'Internet', ssh: 'SSH' };

/** A route's name inside the connection line's sentence (README: "the route kinds"). */
export const ROUTE_IN_LINE: Record<RouteKind, string> = { lan: 'local network', tailnet: 'Tailscale', vpn: 'VPN', public: 'internet address', ssh: 'SSH' };

export const osName = (c: Pick<Computer, 'os'>) => (c.os === 'linux' ? 'Linux' : 'Mac');

/** A row's second line: the computer in use says how it's reached; one no route answers says so; another its kind. */
export function rowAbout(c: Computer, inUse: boolean): string {
  if (c.state === 'lost') return 'Can’t reach it right now';
  if (inUse && c.route) return `${osName(c)} · ${ROUTE_NAME[c.route.kind]}`;
  return osName(c);
}

/** The button's name for screen readers. */
export const buttonLabel = (name: string) => `Computer: ${name}. Change`;

/** The connection line while a fallback route takes over (shown only after 2 s). */
export const movingLine = (name: string, from: RouteKind, to: RouteKind) =>
  `${name}’s ${ROUTE_IN_LINE[from]} doesn’t answer. Reconnecting over ${ROUTE_IN_LINE[to]}…`;

/** The connection line, outside Home, while no route answers. */
export const lostLine = (name: string) => `Oby can’t reach ${name} right now.`;

/** Home, while no route answers. */
export const lostHome = (name: string) => `Oby can’t reach ${name} right now.\nIts tasks keep running there, and Oby keeps trying.`;

/** Home, for the moment a switch takes. */
export const switchingHome = (to: string, from: string) => `Switching to ${to}…\n${from} keeps its tasks running.`;

/** Adding over SSH: each step's words. */
export const stepWords = (step: AddStep, host: string) =>
  step === 'ssh' ? `Reached ${host} over SSH` : step === 'pair' ? `Pairing this Mac with Oby on ${host}…` : 'Finding the fastest way to reach it';

/** "Good until {time}, for one computer.", the time in the person's own clock. */
export const goodUntil = (at: number) =>
  `Good until ${new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}, for one computer.`;

/** A fingerprint as the design shows it: 16 bytes in groups of four hex digits, a dot halfway. */
export function fingerprintText(hex: string): string {
  const h = hex.replace(/^sha256:/i, '').replace(/[^0-9a-f]/gi, '').toUpperCase().slice(0, 32);
  const groups = h.match(/.{1,4}/g) ?? [];
  return [groups.slice(0, 4).join(' '), groups.slice(4, 8).join(' ')].filter(Boolean).join(' · ');
}
