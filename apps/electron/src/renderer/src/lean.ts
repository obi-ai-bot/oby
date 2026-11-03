// The app shows only the conversation (Dmytro and Wellington's call of
// 2026-10-26): a new user starts empty, Oby sets them up in a conversation,
// and what isn't used yet (Home's kinds of work, the side panel's tasks and
// Obyspheres, the archive) is hidden, not deleted. ?everything shows all of
// Sangam's design again, with its demo data.
export const everything = new URLSearchParams(location.search).has('everything');
// In the Mac app (window.app), without ?everything, what's shown is obid's (live.ts); in a browser,
// or with it, the mock's.
// A dev scenario (?dev=, dev/ControlCenter.tsx) plays on the mock, never on the real obid.
export const onObid = !everything && !!window.app?.obid && !(import.meta.env.DEV && new URLSearchParams(location.search).has('dev'));
// The app opens on your own conversation with Oby, a chat, not on Home (the owner's word of
// 2026-11-02, docs/contracts/chat-first.md). Sangam's Home stays to compare: ?home, or the oby.home
// default; ?everything is his whole design as it was, and with ?chat, this on its demo data.
const homeKept = () => { try { return localStorage.getItem('oby.home') === '1'; } catch { return false; } };
export const chatFirst = !new URLSearchParams(location.search).has('home') && !homeKept() && (!everything || new URLSearchParams(location.search).has('chat'));
// ?welcome: set up again (onboarding.ts)
export const welcome = new URLSearchParams(location.search).has('welcome');
