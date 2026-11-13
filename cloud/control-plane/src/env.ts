/// What the Worker gets from wrangler.jsonc (vars, the D1 binding) and from its secrets.
export interface Env {
  DB: D1Database;
  /// The sealed push relay (#398): the iPhone apps' bundle ids it pushes to, comma-separated
  /// (APNs topics). Empty: no phone registers.
  APNS_TOPICS: string;
  /// The most phones registered for pushes at once: a whole number.
  MAX_PUSH_DEVICES: string;

  // Secrets: `wrangler secret put`, or .dev.vars. Never in D1 and never logged.
  /// The APNs key (an Apple Developer key with Apple Push Notifications service): the .p8 file's
  /// text, its Key ID and the team's ID. Without all three, the relay sends nothing (503).
  APNS_KEY?: string;
  APNS_KEY_ID?: string;
  APNS_TEAM_ID?: string;
  /// What every phone's pushSecret is made from, with its pushId (src/app.ts, `pushSecret`), so
  /// no secret is kept: at least 32 random characters. Changing it breaks every registered phone.
  PUSH_SECRET_KEY?: string;
}
