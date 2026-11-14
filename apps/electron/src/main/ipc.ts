import { ipcMain, type IpcMainEvent, type IpcMainInvokeEvent } from 'electron';
import type { Decoder } from '../shared/decode';

// Every channel the page reaches main on, registered here and nowhere else (docs/contracts/ipc.md):
// first the sender (only the window's own page, never a frame in it: trusted), then each argument
// read by its decoder (shared/decode.ts). The handler runs only when both pass; anything else gets
// the channel's refusal, and arguments past the decoders are ignored.

// The page's own frame only: no other page, or frame in it, gets main
export type Trusted = (e: IpcMainEvent | IpcMainInvokeEvent) => boolean;

type Values<D extends Decoder<unknown>[]> = { [K in keyof D]: D[K] extends Decoder<infer T> ? T : never };

// the arguments as the decoders read them, or null when one can't
function decoded<D extends Decoder<unknown>[]>(decoders: D, args: unknown[]): Values<D> | null {
  const values: unknown[] = [];
  for (const [i, d] of decoders.entries()) {
    const read = d(args[i]);
    if (!read) return null;
    values.push(read.value);
  }
  return values as Values<D>;
}

export function ipc(trusted: Trusted) {
  return {
    // invoke: refused answers an untrusted sender, invalid an argument a decoder can't read
    handle<D extends Decoder<unknown>[], R, A>(channel: string, decoders: [...D], fn: (e: IpcMainInvokeEvent, ...args: Values<D>) => R,
      refused: A, invalid: A = refused) {
      ipcMain.handle(channel, (e, ...args) => {
        if (!trusted(e)) return refused;
        const values = decoded(decoders, args);
        return values ? fn(e, ...values) : invalid;
      });
    },
    // send, or sendSync when the channel names its refusal: then the page always gets an answer,
    // the refusal or the handler's (e.returnValue)
    on<D extends Decoder<unknown>[], R, A>(channel: string, decoders: [...D], fn: (e: IpcMainEvent, ...args: Values<D>) => R,
      ...refusal: [] | [refused: A, invalid?: A]) {
      const [refused, invalid = refused] = refusal;
      ipcMain.on(channel, (e, ...args) => {
        let answer: R | A | undefined = refused;
        if (trusted(e)) {
          const values = decoded(decoders, args);
          answer = values ? fn(e, ...values) : invalid;
        }
        if (refusal.length) e.returnValue = answer;
      });
    },
  };
}
