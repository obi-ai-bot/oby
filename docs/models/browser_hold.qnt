// verify: main=browser_hold steps=12 invariants=noActStartsWhileHeld,takeAnswersOnlyWithNoActRunning witnesses=neverHeldWithAnActLetThrough,takeNeverWaits
// The owner taking an agent's browser tab and giving it back, racing the agent's acts on it (task W01-08;
// docs/contracts/browser.md, BROWSER-5 and BROWSER-6; T3 Code's SessionControl). One agent tab, two agent acts and
// two of the owner's devices. It models, by function:
// - Api::lease in core/crates/obid/src/browser/api/state.rs: an act is let through, with the hold generation it was
//   leased at, only while nobody holds the tab, and refused `human_control` otherwise (lease);
// - Api::commit there, right before the act runs: refused `control_changed` when the generation moved since the lease,
//   otherwise counted in `inflight` and running (commit);
// - Api::done there: the act ended, uncounted (done);
// - Api::take there, plus the wait in `take` in core/crates/obid/src/browser/routes.rs: the device holds the tab (the
//   generation up when nobody held it, refused `held_by_other` when another device does) and its call waits while
//   `inflight` is above 0 (take);
// - the take route answering once `inflight` is 0 (takeAnswers);
// - Api::give_back in state.rs, called by `give_back` in routes.rs: only the holder gives the tab back, the
//   generation up (giveBack);
// - plus a stutter.
// An act is idle, let through (at the generation it was leased at), running, or refused; a refused act may be leased
// again. startedWhileHeld is a ghost flag, set when a commit succeeds while the tab is held.
// noActStartsWhileHeld (BROWSER-5): no act starts while the owner holds the tab. takeAnswersOnlyWithNoActRunning
// (BROWSER-6): a take that has answered means no act is running. Both are expected to hold in every sequence up to
// 12 steps (quint verify --max-steps=12 --invariant=<name>).
// Their witnesses, each expected to be violated, so a pass is not vacuous: neverHeldWithAnActLetThrough, violated by a
// lease followed by a take (the race noActStartsWhileHeld guards); takeNeverWaits, violated by a take made while an act
// runs (the race takeAnswersOnlyWithNoActRunning guards).
// The limit: a model checks the design, not that state.rs and routes.rs implement it; nothing here replays a trace
// against the code. It leaves out the wait's time bound (engine::MAX_DEADLINE_MS + 5000 ms: an act's own deadline
// ends it first), the device's signature (signed() in routes.rs, covered by tests/browser_api.rs), the engine core's
// own hold (Browsers::take and release) and the agent's refs (stale_ref, covered by the Rust tests). A give back while
// its device's own take still waits isn't modelled.
module browser_hold {
  pure val ACTS = Set(1, 2)
  pure val DEVICES = Set(1, 2)
  // The device holding the tab, 0 for nobody.
  var heldBy: int
  var generation: int
  var act: int -> str
  var leasedAt: int -> int
  var inflight: int
  // The device whose take waits to answer, 0 for none.
  var taking: int
  var answered: bool
  var startedWhileHeld: bool
  action init = all {
    heldBy' = 0,
    generation' = 0,
    act' = ACTS.mapBy(a => "idle"),
    leasedAt' = ACTS.mapBy(a => 0),
    inflight' = 0,
    taking' = 0,
    answered' = false,
    startedWhileHeld' = false,
  }
  action lease(a: int): bool = all {
    act.get(a).in(Set("idle", "refused")),
    if (heldBy == 0) all { act' = act.set(a, "let"), leasedAt' = leasedAt.set(a, generation) }
    else all { act' = act.set(a, "refused"), leasedAt' = leasedAt },
    heldBy' = heldBy,
    generation' = generation,
    inflight' = inflight,
    taking' = taking,
    answered' = answered,
    startedWhileHeld' = startedWhileHeld,
  }
  action commit(a: int): bool = all {
    act.get(a) == "let",
    if (leasedAt.get(a) == generation) all {
      act' = act.set(a, "running"),
      inflight' = inflight + 1,
      startedWhileHeld' = (startedWhileHeld or heldBy != 0),
    } else all { act' = act.set(a, "refused"), inflight' = inflight, startedWhileHeld' = startedWhileHeld },
    heldBy' = heldBy,
    generation' = generation,
    leasedAt' = leasedAt,
    taking' = taking,
    answered' = answered,
  }
  action done(a: int): bool = all {
    act.get(a) == "running",
    act' = act.set(a, "idle"),
    inflight' = inflight - 1,
    heldBy' = heldBy,
    generation' = generation,
    leasedAt' = leasedAt,
    taking' = taking,
    answered' = answered,
    startedWhileHeld' = startedWhileHeld,
  }
  action take(d: int): bool = all {
    taking == 0,
    heldBy == 0 or heldBy == d,
    heldBy' = d,
    generation' = if (heldBy == 0) generation + 1 else generation,
    taking' = d,
    act' = act,
    leasedAt' = leasedAt,
    inflight' = inflight,
    answered' = answered,
    startedWhileHeld' = startedWhileHeld,
  }
  action takeAnswers = all {
    taking != 0,
    inflight == 0,
    taking' = 0,
    answered' = true,
    heldBy' = heldBy,
    generation' = generation,
    act' = act,
    leasedAt' = leasedAt,
    inflight' = inflight,
    startedWhileHeld' = startedWhileHeld,
  }
  action giveBack(d: int): bool = all {
    heldBy == d,
    taking == 0,
    heldBy' = 0,
    generation' = generation + 1,
    answered' = false,
    taking' = taking,
    act' = act,
    leasedAt' = leasedAt,
    inflight' = inflight,
    startedWhileHeld' = startedWhileHeld,
  }
  action stutter = all {
    heldBy' = heldBy,
    generation' = generation,
    act' = act,
    leasedAt' = leasedAt,
    inflight' = inflight,
    taking' = taking,
    answered' = answered,
    startedWhileHeld' = startedWhileHeld,
  }
  action step = any {
    nondet a = ACTS.oneOf()
    any { lease(a), commit(a), done(a) },
    nondet d = DEVICES.oneOf()
    any { take(d), giveBack(d) },
    takeAnswers,
    stutter,
  }
  val noActStartsWhileHeld = not(startedWhileHeld)
  val takeAnswersOnlyWithNoActRunning = answered implies inflight == 0
  val neverHeldWithAnActLetThrough = not(heldBy != 0 and ACTS.exists(a => act.get(a) == "let"))
  val takeNeverWaits = not(taking != 0 and inflight > 0)
}
