// Where the window's page may go. It holds obid's API through main, so it never leaves for another page
// or opens one. It may reload itself: once main has opened another profile, the page reads everything
// again as a window of that profile (profiles.ts, docs/contracts/profiles.md PROFILES-41 and 42), and
// Chromium reports a page's own reload as a navigation to the URL it is at. In dev, it may also go to
// itself with other words after ? (?welcome).
export function stays(to: string, at: string, dev: boolean): boolean {
  if (to === at) return true;
  if (!dev) return false;
  try { return new URL(to).origin === new URL(at).origin; } catch { return false; }
}
