// verify: main=helpers steps=12 invariants=onlyGrantedConnectors,noSecretReachesTheTool,oneProfile witnesses=neverUsesAConnector
// Helpers (task A00-77, docs/contracts/helpers.md): which connectors a helper's turn uses, what its tool is given,
// and the one profile a helper is in. It models, by function:
// - core/crates/obid/src/helpers.rs: `create` (POST /v1/helpers: a profile on this Mac, set once), `change` (PATCH:
//   its grants and instructions, since a profile in it is refused), `delete` (DELETE: its conversation goes, and so
//   does the turn under way in it), and `may_use` (mayUse);
// - harness::run and harness::mcp_config in core/crates/obid/src/brain/harness.rs: a turn starts with its helper's
//   grants at that moment (what it is offered: Tetrix's server, the web's tools), and is given its instructions
//   through `Helper::told`, which scrubs them with vault::for_agent (scrub), the owner's words and the turn's token;
// - each connector's guard, as its code path has it (allowed):
//   - tetrix: mcp_config offers its server only when the turn's starting grants hold it, and Tetrix::converse in
//     core/crates/obid/src/tetrix.rs refuses a call while the helper's grants as kept now lack it: both;
//   - github: tools::call in core/crates/obid/src/brain/tools.rs refuses a call while the helper's grants as kept
//     now lack it: the current grants only, so a grant taken away stops at once and one given counts at once;
//   - web: the tool's own search, given or not by claude_flags' `--tools` and harness_codex's `web_search` when
//     the turn starts: the starting grants only;
//   obid makes a tetrix or github call with a vault secret of its own (its login), and the tool gets only the
//   answer scrubbed. The web's search is the tool's own: obid neither holds a secret for it nor sees its answer.
// The invariants: onlyGrantedConnectors (requirement 10: a use of tetrix or github was granted when it was made,
// and a use of web was granted when its turn started), noSecretReachesTheTool (nothing the tool was given holds a
// vault secret, though instructions and a connector's answer may) and oneProfile (every helper's profile is a known
// profile and the one it was made with), each expected to hold in every sequence up to 12 steps.
// neverUsesAConnector is their witness: a helper is made in a profile, its turn starts and uses a connector, and obid
// uses its secret on its own side, so it is expected to be violated, and a pass of the three is not vacuous.
// Every state is checked, so keeping the last use is enough for onlyGrantedConnectors. Routines are left out.
// The limit: a model checks the design, not that the code implements it. Nothing here replays a trace against the
// code (Quint Connect is not used).
module helpers {
  pure val CONNECTORS = Set("github", "tetrix", "web")
  pure val PROFILES = Set("personal", "work")
  pure val HELPERS = Set("h1", "h2")
  // A text (instructions, a connector's answer): plain words, or words holding the vault's secret, and the
  // latter as vault::for_agent leaves it, the secret's name in its place.
  pure val PLAIN = "words"
  pure val HOLDING = "words with vault-secret"
  pure val SCRUBBED = "words with [vault: secret]"
  pure val TEXTS = Set(PLAIN, HOLDING)
  // Everything the tool could be given.
  pure val PARTS = Set(PLAIN, HOLDING, SCRUBBED, "the owner's words", "the turn's token")
  pure def mayUse(granted: Set[str], asked: str): bool = granted.contains(asked)
  // vault::for_agent: the secret taken out, its name left in its place.
  pure def scrub(text: str): str = if (text == HOLDING) SCRUBBED else text
  // Whether obid lets a turn that started with `started` use `asked` while its helper holds `granted`.
  pure def allowed(started: Set[str], granted: Set[str], asked: str): bool =
    if (asked == "tetrix") mayUse(started, asked) and mayUse(granted, asked)
    else if (asked == "github") mayUse(granted, asked)
    else mayUse(started, asked)

  // The helpers there are; each one's profile, and the one it was made with (kept by the model alone).
  var made: Set[str]
  var profileOf: str -> str
  var madeIn: str -> str
  var grants: str -> Set[str]
  var instructions: str -> str
  // Whether a turn is under way in each one's conversation, and the grants it started with.
  var turnOn: str -> bool
  var started: str -> Set[str]
  // The last connector use obid allowed: the grants its turn started with, and its helper's at the use.
  var used: bool
  var lastUse: { connector: str, started: Set[str], granted: Set[str] }
  // What the tool has been given, of PARTS, and whether obid used its secret on its own side.
  var given: str -> bool
  var obidUsedSecret: bool

  action init = all {
    made' = Set(), profileOf' = HELPERS.mapBy(_ => ""), madeIn' = HELPERS.mapBy(_ => ""),
    grants' = HELPERS.mapBy(_ => Set()), instructions' = HELPERS.mapBy(_ => PLAIN),
    turnOn' = HELPERS.mapBy(_ => false), started' = HELPERS.mapBy(_ => Set()),
    used' = false, lastUse' = { connector: "", started: Set(), granted: Set() }, given' = PARTS.mapBy(_ => false),
    obidUsedSecret' = false,
  }
  action create = {
    nondet h = HELPERS.oneOf()
    nondet p = PROFILES.oneOf()
    nondet g = CONNECTORS.powerset().oneOf()
    nondet i = TEXTS.oneOf()
    all {
      not(made.contains(h)),
      made' = made.union(Set(h)), profileOf' = profileOf.set(h, p), madeIn' = madeIn.set(h, p), grants' = grants.set(h, g),
      instructions' = instructions.set(h, i),
      turnOn' = turnOn, started' = started, used' = used, lastUse' = lastUse, given' = given,
      obidUsedSecret' = obidUsedSecret,
    }
  }
  action edit = {
    nondet h = HELPERS.oneOf()
    nondet g = CONNECTORS.powerset().oneOf()
    nondet i = TEXTS.oneOf()
    all {
      made.contains(h),
      grants' = grants.set(h, g), instructions' = instructions.set(h, i),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, turnOn' = turnOn, started' = started, used' = used,
      lastUse' = lastUse, given' = given, obidUsedSecret' = obidUsedSecret,
    }
  }
  action delete = {
    nondet h = HELPERS.oneOf()
    all {
      made.contains(h),
      made' = made.exclude(Set(h)), turnOn' = turnOn.set(h, false),
      profileOf' = profileOf, madeIn' = madeIn, grants' = grants, instructions' = instructions, started' = started,
      used' = used, lastUse' = lastUse, given' = given, obidUsedSecret' = obidUsedSecret,
    }
  }
  action startTurn = {
    nondet h = HELPERS.oneOf()
    all {
      made.contains(h), not(turnOn.get(h)),
      turnOn' = turnOn.set(h, true), started' = started.set(h, grants.get(h)),
      given' = given.set(scrub(instructions.get(h)), true).set("the owner's words", true).set("the turn's token", true),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, grants' = grants, instructions' = instructions,
      used' = used, lastUse' = lastUse, obidUsedSecret' = obidUsedSecret,
    }
  }
  action useConnector = {
    nondet h = HELPERS.oneOf()
    nondet c = CONNECTORS.oneOf()
    nondet answer = TEXTS.oneOf()
    // obid's own calls (tetrix, github) use its login and give the tool their answer scrubbed.
    val obids = c != "web"
    all {
      turnOn.get(h),
      allowed(started.get(h), grants.get(h), c),
      used' = true, lastUse' = { connector: c, started: started.get(h), granted: grants.get(h) },
      obidUsedSecret' = (obidUsedSecret or obids),
      given' = if (obids) given.set(scrub(answer), true) else given,
      made' = made, profileOf' = profileOf, madeIn' = madeIn, grants' = grants, instructions' = instructions,
      turnOn' = turnOn, started' = started,
    }
  }
  action endTurn = {
    nondet h = HELPERS.oneOf()
    all {
      turnOn.get(h),
      turnOn' = turnOn.set(h, false),
      made' = made, profileOf' = profileOf, madeIn' = madeIn, grants' = grants, instructions' = instructions,
      started' = started, used' = used, lastUse' = lastUse, given' = given, obidUsedSecret' = obidUsedSecret,
    }
  }
  action step = any { create, edit, delete, startTurn, useConnector, endTurn }

  val onlyGrantedConnectors = not(used) or (
    if (lastUse.connector == "web") mayUse(lastUse.started, lastUse.connector)
    else mayUse(lastUse.granted, lastUse.connector)
  )
  val noSecretReachesTheTool = not(given.get(HOLDING))
  val oneProfile = made.forall(h => PROFILES.contains(profileOf.get(h)) and profileOf.get(h) == madeIn.get(h))
  val neverUsesAConnector = not(used)
}
