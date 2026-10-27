import type { Message } from './mock';
import { onObid } from './lean';

// Oby's words come in a word at a time, as a model's do: its message goes in
// empty, then each word is added 35ms after the last (edit: the message, as it
// is now, changed). Shared by a task's conversation, a conversation with Oby
// and conversations with other Obys (peers.ts).
const WORD_MS = 35;
// Every wait in the app goes through here. Dev only, on the mock (dev/ControlCenter.tsx): one
// speed for all of them (Instant: none at all, so it jumps even in a background window), and a
// gate that holds each for Step and Pause. On obid they stay real: a sign-in's wait for obid
// would never yield at Instant
export const clock = { speed: 1, gate: undefined as ((ms: number) => Promise<void>) | undefined };
const sleep = (ms: number) => new Promise<void>((r) => (ms ? setTimeout(r, ms) : r()));
export const wait = (ms: number): Promise<void> => {
  if (!import.meta.env.DEV || onObid) return new Promise((r) => setTimeout(r, ms));
  return (clock.gate?.(ms) ?? Promise.resolve()).then(() => sleep(ms / clock.speed));
};
export async function streamWords(text: string, edit: (fn: (m: Message) => Message) => void) {
  const words = text.match(/\S+\s*/g) ?? [];
  for (let n = 1; n <= words.length; n++) {
    await wait(WORD_MS);
    edit((m) => ({ ...m, text: words.slice(0, n).join('') }));
  }
}
