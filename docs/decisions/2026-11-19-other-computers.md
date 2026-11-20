# Other computers: any network the person has, SSH as a tunnel only, Macs and Linux, never through the Obysphere

- **Date:** 2026-11-19
- **Status:** In force
- **Replaced:** Nothing AGENTS.md names
- **Changed by:** Nothing
- **Contracts and scenarios:** [remote.md](../contracts/remote.md), COMPUTERS-1 onward

## The owner's words

The owner's decisions of 2026-11-19 on using another computer's resources from Oby, as T3 Code's remote environments do. They're recorded in the report the owner answered that day and the lab's pull requests (#2595, #2605):

- Any network the person already has. Tailscale is an optional extra, never required; T3-style fallback routes apply.
- SSH is in v1 as a tunnel only: nothing is installed over SSH.
- The Obysphere is for connecting people, never the transport to your own computers.
- Macs and Linux in v1.
- The whole window switches to the other computer first, like changing workspace; choosing a computer per task comes later.
- Every feature is built and proven in the lab first (tools/obid-spec, `lab-remote`), then ported into obid, its workflows becoming its acceptance tests.

How a computer is added, as the owner asked:

- Over SSH: the person's own SSH writes this Mac's device key into the other obid's `home/pairing`, which obid already accepts.
- Without SSH: a pairing code made on the other computer, with its certificate's fingerprint pinned.
- A direct address is always HTTPS with a pinned certificate.

## Where it is stated

- [remote.md](../contracts/remote.md), every rule that links here
- [docs/api/remote.md](../api/remote.md), `obid pair-computer`
- [tools/obid-spec/workflows/lab/remote/README.md](../../tools/obid-spec/workflows/lab/remote/README.md), the lab that proved it

## Notes

On 2026-11-20 the owner added the real second Mac as the lab's other computer (#2605): their MacBook Pro, reached over SSH, with a scratch obid in a scratch folder only. The Oby already running there is never touched, and its address never goes in the repository.

The window's computer picker is the owner's to approve as a design, later. Where the window's computers live is also still the owner's decision: the supervisor, tunnels and tokens on the Mac's own obid (`home/computers.json`, tokens in the vault), or elsewhere. Until then, obid has the pieces that don't depend on it.
