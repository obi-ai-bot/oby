// verify: main=reply_target steps=10 invariants=wordsReachOneTarget witnesses=neverToAnAgent
// Where words typed in a conversation with Oby go once a task's card opens in place (task A00-72; LIVETASK-17 in
// docs/contracts/live-task.md). It models, by function:
// - replyTarget in apps/electron/src/renderer/src/replyTarget.ts: the box alone picks where its words go, the foot
//   box Oby and no job, a card's box that card's job and nothing else (target reads only the box);
// - openCards in apps/electron/src/renderer/src/AgentThread.tsx, with ChatView's open and fold in ChatView.tsx: a
//   card shows open when the owner opened it, or when it needs them and they didn't fold it during this need
//   (shownOpen, openCard, foldCard, needsYou, answered; LIVETASK-16). The spec's open, fold and type are
//   openCard, foldCard and typeIn here: fold is a Quint built-in and type a keyword.
// Which cards are open or which box is focused is in the state, so the model shows they never change a target.
// wordsReachOneTarget is the property: the last words typed in the foot box reached Oby and no job, and the last
// typed in a card's box reached that job alone. It is expected to hold in every sequence up to 10 steps
// (quint verify --max-steps=10 --invariant=wordsReachOneTarget).
// neverToAnAgent is its witness: words typed in an opened card's box do reach its agent, so it is expected to be
// violated, and a pass of wordsReachOneTarget is not vacuous.
// The limit: a model checks the design, not that replyTarget.ts or ChatView.tsx implements it. Nothing here
// replays a trace against the code (Quint Connect is not used); the vitest tests named LIVETASK-17 check the
// function against the same cases.
module reply_target {
  pure val Jobs = Set("a", "b")
  // the boxes: the message box at the foot, and one per card
  pure val Boxes = Jobs.union(Set("foot"))
  // words typed in a box, and where they went: to Oby, and to which jobs ("" before any)
  type Sent = { box: str, oby: bool, jobs: Set[str] }

  var opened: Set[str]   // the cards the owner opened in place
  var folded: Set[str]   // cards needing the owner they folded during this need
  var needing: Set[str]  // the jobs that need the owner
  var focused: str       // the box the owner is in: "foot" or a job's card
  var last: Sent         // the last words typed

  // replyTarget: only the box decides
  pure def target(b: str): Sent = { box: b, oby: b == "foot", jobs: if (b == "foot") Set() else Set(b) }
  // openCards: opened ∪ (needing ∖ folded)
  val shownOpen = opened.union(needing.exclude(folded))

  action init = all {
    opened' = Set(), folded' = Set(), needing' = Set(), focused' = "foot",
    last' = { box: "", oby: false, jobs: Set() },
  }

  // a click on a folded card opens it; on an open one, folds it, and a needing one stays folded for this need
  action openCard(j: str): bool = all {
    not(shownOpen.contains(j)),
    opened' = opened.union(Set(j)), folded' = folded.exclude(Set(j)),
    needing' = needing, focused' = focused, last' = last,
  }
  action foldCard(j: str): bool = all {
    shownOpen.contains(j),
    opened' = opened.exclude(Set(j)), folded' = if (needing.contains(j)) folded.union(Set(j)) else folded,
    needing' = needing, focused' = if (focused == j) "foot" else focused, last' = last,
  }
  // a job starts needing the owner, or stops: a fold lasts as long as its need
  action needsYou(j: str): bool = all {
    not(needing.contains(j)),
    needing' = needing.union(Set(j)),
    opened' = opened, folded' = folded, focused' = focused, last' = last,
  }
  action answered(j: str): bool = all {
    needing.contains(j),
    needing' = needing.exclude(Set(j)), folded' = folded.exclude(Set(j)),
    opened' = opened, focused' = focused, last' = last,
  }
  // the owner goes into a box: the foot one, or a card's shown open
  action focus(b: str): bool = all {
    b == "foot" or shownOpen.contains(b),
    focused' = b,
    opened' = opened, folded' = folded, needing' = needing, last' = last,
  }
  // the owner types into a box shown, and sends
  action typeIn(b: str): bool = all {
    b == "foot" or shownOpen.contains(b),
    last' = target(b),
    opened' = opened, folded' = folded, needing' = needing, focused' = focused,
  }

  action step = {
    nondet j = Jobs.oneOf()
    nondet b = Boxes.oneOf()
    any { openCard(j), foldCard(j), needsYou(j), answered(j), focus(b), typeIn(b) }
  }

  val wordsReachOneTarget = and {
    last.box == "foot" implies (last.oby and last.jobs == Set()),
    Jobs.contains(last.box) implies (not(last.oby) and last.jobs == Set(last.box)),
  }
  // the witness: expected to be violated
  val neverToAnAgent = last.jobs == Set()
}
