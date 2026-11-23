// The partitions a browser tab's page may live in (docs/contracts/browser-app.md, BROWSERAPP-24, 45
// to 48), one rule for main and the page.

/**
 * The partitions obid names for its agents' tabs: one per agent for the owner's own work (sign-ins
 * kept), and one per job for another Oby's or an untrusted job (not persistent). Never the default
 * session, the window's own, or the owner's own below.
 */
export const PARTITION = /^(persist:oby-agent-(claude|codex|cursor)|oby-box-[A-Za-z0-9_-]{1,128})$/;
export const partitionOk = (p: unknown): p is string => typeof p === 'string' && PARTITION.test(p);

/**
 * The owner's own tabs: the window's, never obid's. A tab they typed an address in lives in this
 * one persistent partition; main keys it by OWN_THREAD in place of a job, so it is never relayed to
 * obid, and nothing but the owner moves it.
 */
export const OWN_PARTITION = 'persist:oby-owner';
export const OWN_THREAD = '';
/**
 * A local server opened from the list (BROWSERAPP-47) gets a throwaway partition of its own, kept in
 * memory and cleared when its tab goes: a job's server (another Oby's or a boxed one among them)
 * never reads or sets the owner's own cookies, which don't keep `localhost` ports apart.
 */
export const scratchPartition = (tab: string) => `oby-owner-tab-${tab}`;
const SCRATCH = /^oby-owner-tab-[A-Za-z0-9_-]{1,128}$/;
/** an owner's own tab's partition: the persistent one, or a server tab's throwaway one */
export const ownPartitionOk = (p: unknown): p is string => p === OWN_PARTITION || (typeof p === 'string' && SCRATCH.test(p));
/** the owner's own tab ids, as the page makes them: never empty, never a path */
export const OWN_TAB = /^own-[a-z0-9]{1,16}-[0-9]{1,9}$/;
