// Small helpers on Web Crypto for the push relay: random bytes, base64url, hashes, and the buckets
// its rate limits count clients and networks by.

export function randomBytes(count: number): Uint8Array {
  const bytes = new Uint8Array(count);
  crypto.getRandomValues(bytes);
  return bytes;
}

export function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function fromBase64url(text: string): Uint8Array | null {
  if (!/^[A-Za-z0-9_-]*$/.test(text)) return null;
  try {
    return Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (character) => character.charCodeAt(0));
  } catch {
    return null;
  }
}

/// An IPv6 address's eight groups, `::` filled in, leading zeros dropped; null for IPv4.
function ipv6Groups(address: string): string[] | null {
  if (!address.includes(":")) return null;
  const lower = address.toLowerCase();
  const [head = "", tail = ""] = lower.split("::", 2);
  const left = head ? head.split(":") : [];
  const right = lower.includes("::") && tail ? tail.split(":") : [];
  const groups = lower.includes("::") ? [...left, ...Array(Math.max(0, 8 - left.length - right.length)).fill("0"), ...right] : left;
  return groups.map((group) => group.replace(/^0+(?=.)/, ""));
}

/// What one client counts as for rate limits: an IPv4 address as it is, an IPv6 one by its /64
/// (one home or one server gets a whole /64, so each of its addresses isn't a client).
export function addressBucket(address: string): string {
  const groups = ipv6Groups(address);
  return groups ? `${groups.slice(0, 4).join(":")}::/64` : address;
}

/// The wider network a client is in: an IPv4 /24, or an IPv6 /48 (what one site is usually
/// given, so its /64s count together).
export function networkBucket(address: string): string {
  const groups = ipv6Groups(address);
  if (groups) return `${groups.slice(0, 3).join(":")}::/48`;
  const octets = address.split(".");
  return octets.length === 4 ? `${octets.slice(0, 3).join(".")}.0/24` : address;
}

export async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
