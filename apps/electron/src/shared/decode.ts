// What the page sends main, read before main acts on it (docs/contracts/ipc.md): each argument of
// a channel goes through a decoder (main/ipc.ts). A decoder answers the value it read, or
// undefined when it can't read one, so a value read as undefined (optional's) is still a success.

export type Decoder<T> = (v: unknown) => { value: T } | undefined;

const when = <T>(ok: (v: unknown) => v is T): Decoder<T> => (v) => (ok(v) ? { value: v } : undefined);

export const string = when((v): v is string => typeof v === 'string');
export const boolean = when((v): v is boolean => typeof v === 'boolean');
export const safeInteger = when((v): v is number => Number.isSafeInteger(v));
// a page in the person's browser: https only
export const https = when((v): v is string => typeof v === 'string' && v.startsWith('https://'));
// undefined too
export const optional = <T>(d: Decoder<T>): Decoder<T | undefined> => (v) => (v === undefined ? { value: undefined } : d(v));
export const oneOf = <const T extends readonly unknown[]>(...values: T): Decoder<T[number]> =>
  (v) => (values.includes(v) ? { value: v as T[number] } : undefined);

// an obid call (main/obid.ts): one of its methods, on its API. send checks the path again, closer.
export const method = oneOf('GET', 'POST', 'PUT', 'PATCH', 'DELETE');
export const v1Path = when((v): v is string => typeof v === 'string' && v.startsWith('/v1/'));

// Lenient, for arguments main took as they came before it decoded them: these never fail.
// true only for exactly true
export const flag: Decoder<boolean> = (v) => ({ value: v === true });
// d's value, or undefined when d can't read one
export const orUndefined = <T>(d: Decoder<T>): Decoder<T | undefined> => (v) => ({ value: d(v)?.value });
// as it came (obid's body: obid says what a path takes)
export const anything: Decoder<unknown> = (v) => ({ value: v });
