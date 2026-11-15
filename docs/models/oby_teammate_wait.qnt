// verify: main=teammate_today steps=12 invariants=oneTeammateAtATime witnesses=neverSilent
// verify: main=teammate_fixed steps=14 invariants=oneTeammateAtATime,neverSilent witnesses=neverApprovedByTeammate,neverRefused,neverSecondAsked
//
// The sender's side of a teammate review (docs/contracts/teammate-review.md): the owner's pull request waits for an
// approval only a person can give on GitHub, and Oby asks teammates' Obys over the Obysphere. Time moves in steps of
// 5 minutes; a request lives 2 hours (24 steps). It models, by function:
// - Link::ask_teammate in core/crates/obid/src/sphere/link/reviews.rs: one teammate asked at a time (ask), the next
//   once one is spent, and a refusal remembered for the head so that teammate isn't asked again (sphereRefuses);
// - approval::with_refusals and approval::refused_words in core/crates/obid/src/sphere/approval.rs, and the merge
//   step's card in Jobs::carry (core/crates/obid/src/jobs.rs, Teammate::card): the owner told why it waits (told).
// neverSilent: while the merge waits on a teammate, the owner is never left without a reason for more than 15
//   minutes: the current request was answered or is under 15 minutes old, or the owner was told why it waits
//   (a refused request's card, TEAMREVIEW-8) and Oby has moved on to the next teammate (TEAMREVIEW-7).
// teammate_today: as built before TEAMREVIEW-8 (TEAMREVIEW-1, 2): a refusal (policy_denied) is taken for a Sphere
//   that can't be reached and asked again silently; a teammate who never answers holds the wait for the request's
//   2 hours. Expected: neverSilent is VIOLATED (witness there).
// teammate_fixed: TEAMREVIEW-8 as built and TEAMREVIEW-7 as proposed. A refusal spends that teammate and tells the
//   owner, and the next untried one is asked (the task's prose, which its intake ruled wins over the model as first
//   pasted: neither ask nor sphereRefuses waits on not(refused) any more, so the next untried teammate is asked
//   after a refusal too). mustAct leaves out the state just after a refusal, where the next ask may wait for Oby's
//   next look: there neverSilent holds only because the refusal told the owner, so a refusal that doesn't
//   (sphereRefuses with told' = told) violates it. Expected: neverSilent holds. This version is checked again with
//   Quint 0.33.1 by the factory's prove step, not the one verified on 2026-11-15.
// oneTeammateAtATime (both): at most one teammate asked that hasn't answered and isn't 15 minutes silent.
// The limit: a model checks the design, not that obid implements it.

module tm_common {
  pure val Mates = Set("m1", "m2")
  pure val Quiet = 3        // 15 minutes, in 5-minute steps
  pure val Life = 24        // 2 hours
}

module teammate_today {
  import tm_common.*
  var asked: str -> int     // teammate -> step it was asked (-1 never)
  var answered: Set[str]
  var refused: bool         // the Sphere refused the last request
  var told: bool            // the owner has a card saying why it waits
  var now: int
  var approved: bool
  val current = Mates.filter(m => asked.get(m) >= 0 and not(answered.contains(m)) and now - asked.get(m) < Life)
  action init = all { asked' = Mates.mapBy(m => -1), answered' = Set(), refused' = false, told' = false, now' = 0, approved' = false }
  action ask = {
    nondet m = Mates.oneOf()
    all { not(approved), current.size() == 0, asked.get(m) < 0 or refused,
          asked' = asked.set(m, now), answered' = answered, refused' = false, told' = told, now' = now, approved' = approved }
  }
  // the Sphere refuses: today taken for a Sphere that can't be reached; asked again later, nothing said
  action sphereRefuses = {
    nondet m = current.oneOf()
    all { not(refused), asked' = asked.set(m, -1), refused' = true, told' = told, answered' = answered, now' = now, approved' = approved }
  }
  action mateApproves = {
    nondet m = current.oneOf()
    all { answered' = answered.union(Set(m)), approved' = true, asked' = asked, refused' = refused, told' = told, now' = now }
  }
  action tick = all { now < 40, now' = now + 1, asked' = asked, answered' = answered, refused' = refused, told' = told, approved' = approved }
  action step = any { ask, sphereRefuses, mateApproves, tick }

  val waiting = not(approved)
  val silentFor = if (current.size() == 0) (if (refused) Quiet else 0)   // a refusal nobody was told about
                  else current.fold(0, (acc, m) => if (now - asked.get(m) > acc) now - asked.get(m) else acc)
  val neverSilent = waiting implies (told or silentFor < Quiet)
  val oneTeammateAtATime = current.filter(m => now - asked.get(m) < Quiet).size() <= 1
}

module teammate_fixed {
  import tm_common.*
  var asked: str -> int
  var answered: Set[str]
  var refused: bool
  var told: bool
  var now: int
  var approved: bool
  // a teammate silent for 15 minutes is passed over (their request keeps its 2 hours: either approval merges it)
  val fresh = Mates.filter(m => asked.get(m) >= 0 and not(answered.contains(m)) and now - asked.get(m) < Quiet)
  val current = Mates.filter(m => asked.get(m) >= 0 and not(answered.contains(m)) and now - asked.get(m) < Life)
  val untried = Mates.filter(m => asked.get(m) < 0)
  action init = all { asked' = Mates.mapBy(m => -1), answered' = Set(), refused' = false, told' = false, now' = 0, approved' = false }
  action ask = {
    nondet m = untried.oneOf()
    all { not(approved), fresh.size() == 0,
          asked' = asked.set(m, now), answered' = answered, refused' = false, told' = told, now' = now, approved' = approved }
  }
  // TEAMREVIEW-8: a refusal is a card for the owner, in obid's words
  action sphereRefuses = {
    nondet m = fresh.oneOf()
    all { asked' = asked, answered' = answered.union(Set(m)), refused' = true, told' = true, now' = now, approved' = approved }
  }
  action mateApproves = {
    nondet m = current.oneOf()
    all { answered' = answered.union(Set(m)), approved' = true, asked' = asked, refused' = refused, told' = told, now' = now }
  }
  // TEAMREVIEW-7: everyone asked and silent past 15 minutes, nobody left to ask: the task's line tells the owner
  action tellNoOneLeft = all { not(approved), fresh.size() == 0, untried.size() == 0, not(told), told' = true,
                               asked' = asked, answered' = answered, refused' = refused, now' = now, approved' = approved }
  // time passes only once Oby has done what it does at once (the next ask, the owner told)
  val mustAct = not(approved) and fresh.size() == 0 and ((untried.size() > 0 and not(refused)) or (untried.size() == 0 and not(told)))
  action tick = all { now < 40, not(mustAct), now' = now + 1, asked' = asked, answered' = answered, refused' = refused, told' = told, approved' = approved }
  action step = any { ask, sphereRefuses, mateApproves, tellNoOneLeft, tick }

  val waiting = not(approved)
  val silentFor = if (fresh.size() > 0) fresh.fold(0, (acc, m) => if (now - asked.get(m) > acc) now - asked.get(m) else acc)
                  else if (current.size() > 0) Quiet else 0
  // checked where time may pass: Oby has nothing it must do at once
  val neverSilent = (waiting and not(mustAct)) implies (told or fresh.size() > 0)
  val oneTeammateAtATime = fresh.size() <= 1
  val neverApprovedByTeammate = not(approved)
  val neverRefused = not(refused)
  val neverSecondAsked = Mates.filter(m => asked.get(m) >= 0).size() < 2
}
