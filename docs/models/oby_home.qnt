// verify: main=oby_home_specs steps=8 invariants=typeOk witnesses=isolated
// verify: main=oby_home_fixed steps=10 invariants=isolated,switchMovesNothing,projectsInTheirProfile witnesses=neverTwoProfilesUsed
//
// The new Home's stores taken together: profiles, projects, conversations, helpers, accounts and the vault.
// Each task (A00-70 projects, A00-75 profiles, A00-77 helpers) has a model of its own feature alone; this
// one checks that they fit together on the owner's goal: "separate environments, one per company".
//
// isolated is that goal: nothing the owner sees while in one profile was made in another.
// oby_home_specs is the design as the three specs have it on 2026-11-15: every store is one per Mac
// (projects.json, helpers.json, conversations, accounts, the vault), and a profile is a name and a colour.
//   Expected: isolated is VIOLATED (it is listed as a witness there, so a violation is the passing result).
// oby_home_fixed is the design with each store kept under the current profile's root, as A00-79 asks.
//   Expected: isolated holds, and so do switching moves nothing and a project's conversations stay in its profile.
// The limit: a model checks a design, not that obid implements it.

module home_common {
  pure val Profiles = Set("p1", "p2")
  pure val Kinds = Set("project", "conversation", "helper", "account", "secret")
  type Item = { kind: str, id: int, madeIn: str, project: int }
}

module oby_home_specs {
  import home_common.*
  var current: str
  var items: Set[Item]
  var nextId: int
  // what the owner sees: one store per Mac, so everything, whatever the profile
  val visible = items
  action init = all { current' = "p1", items' = Set(), nextId' = 1 }
  action make = {
    nondet k = Kinds.oneOf()
    all { nextId < 6, items' = items.union(Set({ kind: k, id: nextId, madeIn: current, project: 0 })),
          nextId' = nextId + 1, current' = current }
  }
  action switch = {
    nondet p = Profiles.oneOf()
    all { current' = p, items' = items, nextId' = nextId }
  }
  action step = any { make, switch }
  val typeOk = items.forall(i => Profiles.contains(i.madeIn))
  val isolated = visible.forall(i => i.madeIn == current)
}

module oby_home_fixed {
  import home_common.*
  var current: str
  var items: Set[Item]
  var nextId: int
  var seenBefore: Set[Item]   // what the owner saw in the profile they last left, kept to check a switch
  // each store under the current profile's root
  val visible = items.filter(i => i.madeIn == current)
  val projectsHere = visible.filter(i => i.kind == "project").map(i => i.id)
  action init = all { current' = "p1", items' = Set(), nextId' = 1, seenBefore' = Set() }
  action make = {
    nondet k = Kinds.oneOf()
    // a conversation may belong to a project of this profile, or to none
    nondet proj = projectsHere.union(Set(0)).oneOf()
    all { nextId < 6,
          items' = items.union(Set({ kind: k, id: nextId, madeIn: current, project: if (k == "conversation") proj else 0 })),
          nextId' = nextId + 1, current' = current, seenBefore' = seenBefore }
  }
  action switch = {
    nondet p = Profiles.oneOf()
    all { current' = p, items' = items, nextId' = nextId, seenBefore' = visible }
  }
  action step = any { make, switch }
  val isolated = visible.forall(i => i.madeIn == current)
  // switching moves nothing: everything made anywhere is still kept
  val switchMovesNothing = seenBefore.forall(i => items.contains(i))
  val projectsInTheirProfile = items.forall(c => c.kind != "conversation" or c.project == 0
    or items.exists(p => p.kind == "project" and p.id == c.project and p.madeIn == c.madeIn))
  // witness: the owner does use two profiles with things in each
  val neverTwoProfilesUsed = not(items.exists(a => items.exists(b => a.madeIn != b.madeIn)))
}
