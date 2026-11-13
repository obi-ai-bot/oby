// The Worker's entry: `handle` (src/app.ts), and the sweep every minute.

import { handle, sweep } from "./app";
import type { Env } from "./env";

export default {
  fetch(request: Request, env: Env): Promise<Response> {
    return handle(request, env, {});
  },
  scheduled(_controller: ScheduledController, env: Env, context: ExecutionContext): void {
    context.waitUntil(sweep(env, {}));
  },
} satisfies ExportedHandler<Env>;
