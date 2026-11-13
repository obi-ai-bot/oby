// What R03-02 took out of the control plane (docs/contracts/retired.md, "The control plane"):
// sign-in, installations, tunnels, hooks and kept deliveries have no route left, the sweep only
// clears ended rate-limit windows, and migration 0009 drops their tables and keeps the push relay's.

import { applyD1Migrations, env } from "cloudflare:test";
import { afterEach, describe, expect, it, vi } from "vitest";
import { sweep } from "../src/app";
import { testApp } from "./helpers";

afterEach(() => vi.restoreAllMocks());

async function table(name: string, db: D1Database = env.DB) {
  return (await db.prepare(`SELECT * FROM ${name} ORDER BY 1`).all()).results;
}

describe("the control plane", () => {
  it("retired_3_dead_routes_answer_404", async () => {
    const app = testApp();
    const seconds = Math.floor(app.time.now().getTime() / 1000);
    await env.DB.prepare("INSERT INTO rate_limits (key, window_start, count) VALUES ('push:60:id:seen', ?1, 1)").bind(seconds).run();
    await env.DB.prepare("INSERT INTO push_devices (id, apns_token, topic, sandbox) VALUES ('phone-0000000000000000000000', ?1, 'com.deskree.oby.ios', 1)").bind("ab".repeat(32)).run();
    const before = { limits: await table("rate_limits"), phones: await table("push_devices") };
    const hook = "0123456789abcdefghijklmnopqrstuv";
    const calls: [string, string, string?][] = [
      ["POST", "/v1/auth/apple/start"],
      ["POST", "/v1/auth/google/callback", "state=x&id_token=y"],
      ["GET", "/v1/auth"],
      ["POST", "/v1/installations", JSON.stringify({ name: "Mac", platform: "macos" })],
      ["GET", "/v1/installations/self"],
      ["POST", "/v1/installations/self/hooks"],
      ["POST", `/h/${hook}`, JSON.stringify({ zen: "hi" })],
      ["GET", `/h/${hook}`],
    ];
    for (const [method, path, body] of calls) {
      const answer = await app.raw(method, path, body, { "CF-Connecting-IP": "198.51.100.7" });
      expect({ method, path, status: answer.status, body: answer.body }).toEqual({ method, path, status: 404, body: { error: "No such route." } });
    }
    expect({ limits: await table("rate_limits"), phones: await table("push_devices") }).toEqual(before);
  });

  it("retired_5_the_sweep_asks_cloudflare_nothing", async () => {
    const app = testApp();
    const seconds = Math.floor(app.time.now().getTime() / 1000);
    const minute = seconds - (seconds % 60);
    const day = seconds - (seconds % 86_400);
    const windows: [string, number][] = [
      ["push:60:id:ended", minute - 60],
      ["push:60:id:current", minute],
      ["push:86400:id:current", day],
      ["sign-ins:address:203.0.113.7", seconds - 86_401],
      ["phones:recent", seconds - 3_600],
    ];
    await env.DB.batch(windows.map(([key, start]) => env.DB.prepare("INSERT INTO rate_limits (key, window_start, count) VALUES (?1, ?2, 1)").bind(key, start)));
    await env.DB.prepare("INSERT INTO push_devices (id, apns_token, topic, sandbox) VALUES ('phone-0000000000000000000000', ?1, 'com.deskree.oby.ios', 1)").bind("ab".repeat(32)).run();
    const phones = await table("push_devices");
    const fetch = vi.spyOn(globalThis, "fetch");
    expect(await sweep(env, app.deps)).toBeUndefined();
    expect(fetch).not.toHaveBeenCalled();
    const left = (await env.DB.prepare("SELECT key FROM rate_limits ORDER BY key").all<{ key: string }>()).results.map((row) => row.key);
    expect(left).toEqual(["phones:recent", "push:60:id:current", "push:86400:id:current"]);
    expect(await table("push_devices")).toEqual(phones);
  });

  it("retired_6_migration_keeps_push_rows", async () => {
    const db = env.MIGRATION_DB;
    await applyD1Migrations(db, env.TEST_MIGRATIONS.filter((migration) => migration.name < "0009"));
    const at = "2026-10-12T12:00:00.000Z";
    await db.batch([
      db.prepare("INSERT INTO users (sub, created_at, provider, workspace_domain, email_domain, limit_domain) VALUES ('google:1', ?1, 'google', 'deskree.com', 'deskree.com', 'deskree.com')").bind(at),
      db.prepare("INSERT INTO sign_ins (state, public_key, nonce_hash, created_at, expires_at, sub, code_hash, provider) VALUES ('state', 'key', 'hash', ?1, 1, 'google:1', 'code', 'google')").bind(at),
      db.prepare("INSERT INTO installations (id, public_key, user_sub, name, platform, created_at, last_seen_at, endpoint_at, queue_key) VALUES ('i1', 'key', 'google:1', 'Mac', 'macos', ?1, ?1, ?1, 'queue')").bind(at),
      db.prepare("INSERT INTO endpoints (installation_id, tunnel_id, hostname, dns_record_id, created_at, network) VALUES ('i1', 't1', 'oby-x.deskree.com', 'r1', ?1, '203.0.113.0/24')").bind(at),
      db.prepare("INSERT INTO nonces (public_key, nonce, expires_at) VALUES ('key', 'nonce', 1)"),
      db.prepare("INSERT INTO hooks (id, installation_id, created_at) VALUES ('h1', 'i1', ?1)").bind(at),
      db.prepare("INSERT INTO hook_queue (id, hook_id, installation_id, received_at, size, sealed) VALUES ('q1', 'h1', 'i1', ?1, 3, x'010203')").bind(at),
      db.prepare("INSERT INTO rate_limits (key, window_start, count) VALUES ('push:60:id:p1', 60, 2), ('sign-ins:address:203.0.113.7', 3600, 1)"),
      db.prepare("INSERT INTO push_devices (id, apns_token, topic, sandbox) VALUES ('p1', ?1, 'com.deskree.oby.ios', 1), ('p2', ?2, 'com.deskree.oby.ios', 0)").bind("ab".repeat(32), "cd".repeat(32)),
      db.prepare("INSERT INTO apns_provider_token (id, made_at, sealed) VALUES (1, 1791800000, 'sealed')"),
    ]);
    const kept = ["push_devices", "push_device_count", "apns_provider_token", "rate_limits"];
    const before = await Promise.all(kept.map((name) => table(name, db)));
    expect(before.every((rows) => rows.length > 0)).toBe(true);
    await applyD1Migrations(db, env.TEST_MIGRATIONS);
    const tables = (await db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all<{ name: string }>()).results.map((row) => row.name);
    for (const gone of ["hook_queue", "hooks", "endpoints", "nonces", "installations", "sign_ins", "users"]) expect(tables).not.toContain(gone);
    expect(await Promise.all(kept.map((name) => table(name, db)))).toEqual(before);
  });
});
