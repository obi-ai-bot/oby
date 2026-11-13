-- R03-02: sign-in, installations, tunnels, hooks and kept deliveries left the control plane, and
-- nothing calls them (obid posts only /v1/push, the iPhone only /v1/push/devices), so their tables
-- go. Children before parents, for the foreign keys. The push relay's tables (rate_limits,
-- push_devices, push_device_count and its triggers, apns_provider_token) stay as they are.
--
-- The owner applies it remotely (`pnpm migrate:remote --env <name>`) only after the code without
-- those routes is deployed, so no running Worker reads a dropped table. Any endpoint left in D1
-- is a live Cloudflare Tunnel and DNS record: delete those first, as 0002 says, since nothing can
-- reach them after this.

DROP TABLE hook_queue;
DROP TABLE hooks;
DROP TABLE endpoints;
DROP TABLE nonces;
DROP TABLE installations;
DROP TABLE sign_ins;
DROP TABLE users;
