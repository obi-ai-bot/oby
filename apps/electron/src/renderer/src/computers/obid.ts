// The picker's one way to obid (docs/contracts/computers-app.md, "Interfaces"). Every call the
// picker makes goes through a ComputersApi: the demo's (demo.ts) stands in for obid on the design's
// screens; obid's comes here once obid serves the picker's routes.
//
// Live today: nothing. obid on dev serves none of the routes the design proposes for the picker
// (README, "What obid provides"): the list of computers and its events, adding over SSH or with a
// code, reading a link, switching, retrying, renaming and removing. #2633 adds only obid's own
// pieces (routes and their order, the pin, the link, the supervisor, SSH's trouble, `obid
// pair-computer`) and the `"computer": true` invitation, whose link doesn't yet carry the direct
// routes and pin a code needs (COMPUTERS-13, proposed). The route guard refuses a client call to a
// path obid doesn't serve (docs/contracts/ci.md, CI-10), so this file names none until obid does:
// on obid the picker isn't offered (COMPUTERSAPP-12), as the browser waited for its routes.
//
// The routes, as the design proposes them, each to wire here when obid serves it:
//   list, follow  GET computers, and the window's stream's `computers` and `computer.route` events
//   addOverSsh    POST computers {ssh}, its progress as `computer.adding` events
//   readLink      POST computers/link {link}
//   addWithCode   POST computers {link, code}
//   invite        POST pairing/invite {computer: true, ...}; setReach: PUT computers/reach
//   use           PUT computers/current {id}
//   retry         POST computers/{id}/retry
//   rename        PATCH computers/{id} {name}
//   remove        DELETE computers/{id}
// The owner's acts among them (add, switch, rename, remove, the reach switches) go signed by main
// (shared/words.ts), which has no shape for them yet.
import type { AddProgress, ComputerList, Invitation, LinkRead, RouteKind } from './view';

/** obid's refusal, in its own words (`say`), shown as it says it. */
export class Refusal extends Error {
  constructor(readonly say: string) { super(say); }
}

export interface ComputersApi {
  list(): Promise<ComputerList>;
  /** every change to the list, as obid sends it; a function that stops following */
  follow(on: (list: ComputerList) => void): () => void;
  /** adds the SSH host the person typed; the steps as they happen; the new computer's id, or a Refusal */
  addOverSsh(host: string, progress: (p: AddProgress) => void): Promise<string>;
  /** the name and fingerprint a link holds, or a Refusal: the window never parses a link itself */
  readLink(link: string): Promise<LinkRead>;
  addWithCode(link: string, code: string): Promise<string>;
  /** this computer's code, for another Mac to add it */
  invite(): Promise<Invitation>;
  setReach(kind: RouteKind, on: boolean): Promise<void>;
  /** the window's computer: one of the list's, or "this" */
  use(id: 'this' | string): Promise<void>;
  /** probes every route now */
  retry(id: string): Promise<void>;
  rename(id: string, name: string): Promise<void>;
  remove(id: string): Promise<void>;
}

/** obid's ComputersApi, once obid answers its list; null while obid serves none of the picker's routes. */
export async function obidComputers(): Promise<ComputersApi | null> {
  return null;
}
