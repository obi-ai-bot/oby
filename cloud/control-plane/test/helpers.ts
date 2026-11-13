// What the tests call the Worker with: `handle` on the test's own D1 and a clock the test moves.

import { env } from "cloudflare:test";
import { type Deps, handle } from "../src/app";
import type { Env } from "../src/env";

export interface Answer {
  status: number;
  headers: Headers;
  // biome-ignore lint/suspicious/noExplicitAny: the tests read what they expect.
  body: any;
}

export function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function testApp(overrides: Partial<Deps> = {}, envOverrides: Partial<Env> = {}) {
  let at = new Date("2026-10-12T12:00:00.000Z");
  const time = {
    now: () => at,
    advance(ms: number) {
      at = new Date(at.getTime() + ms);
    },
  };
  const deps: Deps = { now: time.now, ...overrides };
  const settings = { ...env, ...envOverrides };

  /// A request as it reaches the Worker, with `headers` as they are.
  async function raw(method: string, path: string, body?: string, headers: Record<string, string> = {}): Promise<Answer> {
    const request = new Request(`https://oby-control.oby.test${path}`, {
      method,
      headers: { ...(body === undefined ? {} : { "Content-Type": "application/json" }), ...headers },
      body,
    });
    const response = await handle(request, settings, deps);
    const text = await response.text();
    return { status: response.status, headers: response.headers, body: text ? JSON.parse(text) : null };
  }

  /// A rate limit's window, counted up to `count` now.
  async function counted(key: string, count: number) {
    await env.DB.prepare("INSERT INTO rate_limits (key, window_start, count) VALUES (?1, ?2, ?3) ON CONFLICT (key) DO UPDATE SET count = ?3")
      .bind(key, Math.floor(at.getTime() / 1000), count)
      .run();
  }

  return { raw, counted, deps, time };
}

export const MINUTE = 60_000;
export const HOUR = 60 * MINUTE;
export const DAY = 24 * HOUR;
