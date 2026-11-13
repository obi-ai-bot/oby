# Oby's control plane

A Cloudflare Worker with a D1 database (#285) that relays pushes to iPhones (#398) and answers its health check, `GET /v1/health`. A phone registers its APNs token (checked with Apple) for a `pushId` and `pushSecret`, gives obid the `pushKey` made from the secret, obid posts alerts sealed to the phone's key, and the Worker sends Apple a fixed alert with the sealed blob, which it can't read: [docs/remote-access.md, Pushes](../../docs/remote-access.md#pushes).

- **Anyone can run one.** Each instance is a named environment in `wrangler.jsonc` (`env.<name>`), deployed with `--env <name>`; the top level is a template with nothing instance-specific. Deskree's own is `env.deskree`, at `oby-control.deskree.com`.
- Every setting is documented in `src/env.ts`. Only public values go in `wrangler.jsonc`; secrets (the APNs key and `PUSH_SECRET_KEY`) are never in git, D1 or logs (the APNs provider token made from one is kept in D1 only sealed): `wrangler secret put --env <name>` in production, `.dev.vars` locally (`.dev.vars.example` has the placeholders).
- `pnpm install`, then `pnpm test` (inside workerd, with APNs faked), `pnpm check` (TypeScript), `pnpm dev --env <name>` (`wrangler dev`).

Sign-in, installations, tunnels, hooks and kept deliveries were removed by R03-02 ([docs/contracts/retired.md](../../docs/contracts/retired.md), "The control plane"); git history keeps them.
