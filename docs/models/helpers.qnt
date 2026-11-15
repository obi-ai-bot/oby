// verify: main=helpers steps=12 invariants=onlyGrantedConnectors,noSecretReachesTheTool,oneProfile witnesses=neverUsesAConnector
// Helpers (task A00-77, docs/contracts/helpers.md): which connectors a helper's turn uses, what its tool is given,
// and the one profile a helper is in. It models, by function:
// - core/crates/obid/src/helpers.rs: `create` (POST /v1/helpers: a profile on this Mac, set once), `change` (PATCH:
//   here its grants only, since a profile in it is refused), `delete` (DELETE: its conversation goes, and so does
//   the turn under way in it), and `may_use` (mayUse);
// - harness::run and harness::mcp_config in core/crates/obid/src/brain/harness.rs: a turn starts with its helper's
//   grants at that moment, and is given its instructions (scrubbed), the owner's words and the turn's token;
// - tools::call in core/crates/obid/src/brain/tools.rs and Tetrix::converse in core/crates/obid/src/tetrix.rs: obid
//   lets a connector be used only when mayUse holds for the grants the turn started with (what it was offered) and
//   for the helper's grants as kept now; obid makes that call with a vault secret of its own (its login) and the
//   tool gets only the scrubbed answer.
// The invariants: onlyGrantedConnectors (every connector use was in the grants its turn started with),
// noSecretReachesTheTool (nothing the tool was given is a vault secret) and oneProfile (every helper's profile is a
// known profile and the one it was made with), each expected to hold in every sequence up to 12 steps.
// neverUsesAConnector is their witness: a helper is made in a profile, its turn starts and uses a connector, and obid
// uses its secret on its own side, so it is expected to be violated, and a pass of the three is not vacuous.
// Every state is checked, so keeping the last use is enough for onlyGrantedConnectors. Routines are left out.
// The limit: a model checks the design, not that the code implements it. Nothing here replays a trace against the
// code (Quint Connect is not used).
module helpers {
  pure val CONNECTORS = Set("github", "tetrix", "web")
  pure val PROFILES = Set("personal", "work")
  pure val HELPERS = Set("h1", "h2")
  // The vault's secret obid holds; the tool's own words are never it.
  pure val SECRET = "vault-secret"
  pure def mayUse(granted: Set[str], asked: str): bool = granted.contains(asked)

  // The helpers there are; each one's profile, and the one it was made with (kept by the model alone).
  var made: Set[str]
  var profileOf: str -> str
  var madeIn: str -> str
  var grants: str -> Set[str]
  // Whether a turn is under way in each one's conversation, and the grants it started with.
  var turnOn: str -> bool
  var started: str -> Set[str]
  // The last connector use obid allowed, and the grants its turn started with.
  var used: bool
  var lastUse: { connector: str, started: Set[str] }
  // What the tool has been given, and whether obid used its secret on its own side.
  var given: Set[str]
  var obidUsedSecret: bool

  action init = all {
    made' = Set(), profileOf' = HELPERS.mapBy(_ => ""), madeIn' = HELPERS.mapBy(_ => ""),
    grants' = HELPERS.mapBy(_ => Set()), turnOn' = HELPERS.mapBy(_ => false), started' = HELPERS.mapBy(_ => Set()),
    used' = false, lastUse' = { connector: "", started: Set() }, given' = Set(), obidUsedSecret' = false,
  }
  action create = {
    nondet h = HELPERS.oneOf()
    nondet p = PROFILES.oneOf()
    nondet g = CONNECTORS.powerset().oneOf()
    all {
      not(made.contains(h)),
      made' = made.union(Set(h)), profileOf' = profileOf.set(h, p), madeIn' = madeIn.set(h, p), grants' = grants.set(h, g),
      turnOn' = turnOn, started' = started, used' = used, lastUse' = lastUse, given' = given,
      obidUsedSecret' = obidUsedSecret,
    }
  }
  action edit = {
    nondet h = HELPERS.oneOf()
    nondet g = CONNECTORS.powerset().oneOf()
    all {
      made.contains(h),
      grants' = grants.set(h, g),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, turnOn' = turnOn, started' = started, used' = used,
      lastUse' = lastUse, given' = given, obidUsedSecret' = obidUsedSecret,
    }
  }
  action delete = {
    nondet h = HELPERS.oneOf()
    all {
      made.contains(h),
      made' = made.exclude(Set(h)), turnOn' = turnOn.set(h, false),
      profileOf' = profileOf, madeIn' = madeIn, grants' = grants, started' = started, used' = used, lastUse' = lastUse,
      given' = given, obidUsedSecret' = obidUsedSecret,
    }
  }
  action startTurn = {
    nondet h = HELPERS.oneOf()
    all {
      made.contains(h), not(turnOn.get(h)),
      turnOn' = turnOn.set(h, true), started' = started.set(h, grants.get(h)),
      given' = given.union(Set("instructions, scrubbed", "the owner's words", "the turn's token")),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, grants' = grants, used' = used, lastUse' = lastUse,
      obidUsedSecret' = obidUsedSecret,
    }
  }
  action useConnector = {
    nondet h = HELPERS.oneOf()
    nondet c = CONNECTORS.oneOf()
    all {
      turnOn.get(h),
      mayUse(started.get(h), c), mayUse(grants.get(h), c),
      used' = true, lastUse' = { connector: c, started: started.get(h) },
      obidUsedSecret' = true,
      given' = given.union(Set("the answer, scrubbed")),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, grants' = grants, turnOn' = turnOn, started' = started,
    }
  }
  action endTurn = {
    nondet h = HELPERS.oneOf()
    all {
      turnOn.get(h),
      turnOn' = turnOn.set(h, false),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, grants' = grants, started' = started, used' = used,
      lastUse' = lastUse, given' = given, obidUsedSecret' = obidUsedSecret,
    }
  }
  action step = any { create, edit, delete, startTurn, useConnector, endTurn }

  val onlyGrantedConnectors = not(used) or mayUse(lastUse.started, lastUse.connector)
  val noSecretReachesTheTool = not(given.contains(SECRET))
  val oneProfile = made.forall(h => PROFILES.contains(profileOf.get(h)) and profileOf.get(h) == madeIn.get(h))
  val neverUsesAConnector = not(used)
}
