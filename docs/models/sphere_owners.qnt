// verify: main=sphere_owners_dev steps=10 invariants=linkFirst,noTakeover,accountNeverMoves,atLeastOneAdmin witnesses=neverSignedInAgain,neverLinkedAfterJoining,neverAdminRemoved,neverReset,linkedAccountIsTheCreators
// verify: main=broken_rebind_any_account steps=8 invariants=accountNeverMoves,atLeastOneAdmin witnesses=linkFirst,noTakeover
// verify: main=broken_link_without_key steps=8 invariants=accountNeverMoves,atLeastOneAdmin witnesses=linkFirst
// verify: main=broken_link_moves_account steps=8 invariants=atLeastOneAdmin witnesses=accountNeverMoves,noTakeover
// verify: main=broken_last_admin_removable steps=8 invariants=linkFirst,noTakeover witnesses=atLeastOneAdmin
//
// The Obysphere server's owner lifecycle, one Sphere: who an owner is (their bound owner key), the account
// they linked, getting back in by a sign-in, an admin's or the operator's reset, and who administers it.
// It models, by function, obi-ai-bot/oby-sphere `dev` at 1607ad6:
// - redeem() in server/src/routes/members.ts (join): a card signed by the owner key it names (verifyCard), the
//   bound key kept ("the owner already has another bound key"), and with a domain invitation's sign-in the
//   account linked only when the owner has none and no other owner has it (`account IS NULL`, "this account
//   joined as another owner");
// - linkAccount() in server/src/routes/owners.ts (link): the owner's own token, minted with the key bound now
//   (requireOwnerToken, `caller.subject === ownerId`), a sign-in started for that key (checkSignIn in
//   server/src/signin.ts), never a second account ("the owner is linked to another account") and never an
//   account another owner has;
// - rebind() in owners.ts (sign in again): only an owner with a linked account (`account_not_linked`), a sign-in
//   from that account started for the new key, a key that isn't the bound one; replaceOwnerKey keeps the
//   account and the admin flag;
// - resetOwner() in owners.ts (reset): an admin's own token or the operator's binds a key given out of band, the
//   account and admin flag kept;
// - putAdmin() and deleteAdmin() in server/src/routes/spheres.ts (the admin flag): an admin or the operator; the
//   last admin is never removed (`COUNT(*) … admin = 1) > 1` in one statement). An admin token needs the flag
//   (oauth.ts, `owner.admin === 1`). Revoking a member (members.ts revoke) touches no owner row, so it isn't
//   modelled; deleting the Sphere deletes every row with it, so it isn't either.
// People: each signs in only to their own account (account p is p), and holds only keys they made, until they
// lose them (a lost Mac). A person proves a key by signing with it: a card, an owner token, a sign-in's key.
//
// linkFirst: every change of an owner's key by a sign-in came from the account linked to that owner then, and
// whoever linked that account held the owner's key bound at that moment.
// noTakeover: the account that gets an owner's key re-bound by a sign-in is the first account ever linked to
// that owner: no other account becomes that owner.
// accountNeverMoves: once linked, an owner's account never changes, by any act.
// atLeastOneAdmin: some owner administers the Sphere, whatever anyone does. There's no act that removes the last
// admin, not even the owner's own: only deleting the Sphere ends it.
// Witnesses (each expected violated, so the properties above aren't vacuous): a sign-in re-bind happens, an
// account is linked after joining, an admin is removed, an owner is reset.
// linkedAccountIsTheCreators is expected violated too, and is a finding, not a bug in the model: an admin (or the
// operator) can reset an owner who never linked an account to a key the admin holds, then link the admin's own
// account to that owner; from then on the person who joined as that owner can get back in only through another
// reset, never by signing in. Link first is what closes it: an owner who linked can't be linked to anyone else.
//
// The broken modules each drop one check and must fail: rebind without the account check, link without the
// owner's token, a link that replaces an account, and a deleteAdmin with no count.
// The limit: a model checks the design, not that the TypeScript implements it.

module sphere_owners {
  // owners.ts rebind, step 4: `owner.account !== signin.account` refuses.
  const RebindChecksAccount: bool
  // owners.ts linkAccount: requireOwnerToken and `caller.subject === ownerId`, a sign-in for the key bound now.
  const LinkNeedsOwnerKey: bool
  // linkAccount's and redeem's `account IS NULL` (or the same account): a linked account is never replaced.
  const LinkKeepsAccount: bool
  // spheres.ts deleteAdmin's `COUNT(*) … admin = 1) > 1`.
  const LastAdminKept: bool

  pure val People = Set("ana", "bob", "eve")
  pure val Names = Set("ana", "bob", "eve")
  pure val NoOne = ""
  pure val Keys = 0.to(4)

  // One row of the owners table; `bound` false: no row yet.
  type Owner = { bound: bool, key: int, account: str, admin: bool }

  var owners: str -> Owner         // the owners table, by owner id
  var holder: int -> str           // who holds each owner private key ("" while unmade, or lost)
  var made: int                    // keys made so far: 0 to made - 1
  // Ghosts: what the properties read, never what an act decides by.
  var creator: str -> str          // who first bound each owner name
  var linkedHeld: str -> bool      // whether whoever linked the owner's account held the owner's key bound then
  var firstAccount: str -> str     // the first account ever linked to each owner
  var signedInAgain: bool          // a re-bind by sign-in happened
  var signInFromLinked: bool       // every re-bind came from the owner's linked account, linked while holding the key
  var signInFromFirst: bool        // every re-bind came from the first account ever linked to that owner
  var adminRemoved: bool
  var reset: bool

  def holdsKeyOf(p: str, o: str): bool = holder.get(owners.get(o).key) == p
  // The account p signs in to is linked to some owner other than o.
  def accountTakenElsewhere(p: str, o: str): bool =
    Names.exists(x => x != o and owners.get(x).bound and owners.get(x).account == p)
  // An admin's own token: minted with the key of an owner whose admin flag is set.
  def isAdmin(p: str): bool = Names.exists(o => owners.get(o).bound and owners.get(o).admin and holdsKeyOf(p, o))

  action init = {
    nondet linked = Set("ana", NoOne).oneOf()
    all {
      // createSphere: the admin owner, with the creator's sign-in's account or (by the operator) none.
      owners' = Names.mapBy(o => if (o == "ana") { bound: true, key: 0, account: linked, admin: true }
                                 else { bound: false, key: -1, account: NoOne, admin: false }),
      holder' = Keys.mapBy(k => if (k == 0) "ana" else NoOne),
      made' = 1,
      creator' = Names.mapBy(o => if (o == "ana") "ana" else NoOne),
      linkedHeld' = Names.mapBy(o => true),
      firstAccount' = Names.mapBy(o => if (o == "ana") linked else NoOne),
      signedInAgain' = false,
      signInFromLinked' = true,
      signInFromFirst' = true,
      adminRemoved' = false,
      reset' = false,
    }
  }

  action keepGhosts = all {
    creator' = creator, linkedHeld' = linkedHeld, firstAccount' = firstAccount, signedInAgain' = signedInAgain,
    signInFromLinked' = signInFromLinked, signInFromFirst' = signInFromFirst, adminRemoved' = adminRemoved,
    reset' = reset,
  }

  // A person makes a new owner key (a new Mac, or a fresh key for a reset).
  action newKey(p: str): bool = all {
    Keys.contains(made),
    holder' = holder.set(made, p),
    made' = made + 1,
    owners' = owners,
    keepGhosts,
  }

  // A person loses every key they hold.
  action lose(p: str): bool = all {
    Keys.exists(k => holder.get(k) == p),
    holder' = holder.keys().mapBy(k => if (holder.get(k) == p) NoOne else holder.get(k)),
    made' = made, owners' = owners,
    keepGhosts,
  }

  // redeem: p joins as owner o with a card signed by key k (theirs), with or without a domain invitation's
  // sign-in; a new name binds k, a bound one must be signed by its bound key.
  action join(p: str, o: str, k: int, withSignIn: bool): bool = {
    val row = owners.get(o)
    val before = row.account
    val links = withSignIn and (before == NoOne or not(LinkKeepsAccount))
    all {
      holder.get(k) == p,
      not(withSignIn and accountTakenElsewhere(p, o)),
      not(row.bound) or row.key == k,
      not(row.bound and withSignIn and LinkKeepsAccount and before != NoOne and before != p),
      owners' = owners.set(o, { bound: true, key: k, account: if (links) p else before, admin: row.admin }),
      creator' = if (row.bound) creator else creator.set(o, p),
      linkedHeld' = if (links) linkedHeld.set(o, true) else linkedHeld,
      firstAccount' = if (links and firstAccount.get(o) == NoOne) firstAccount.set(o, p) else firstAccount,
      holder' = holder, made' = made,
      signedInAgain' = signedInAgain, signInFromLinked' = signInFromLinked, signInFromFirst' = signInFromFirst,
      adminRemoved' = adminRemoved, reset' = reset,
    }
  }

  // linkAccount: p links the account they sign in to to owner o.
  action link(p: str, o: str): bool = all {
    owners.get(o).bound,
    not(LinkNeedsOwnerKey) or holdsKeyOf(p, o),
    not(LinkKeepsAccount) or owners.get(o).account == NoOne,
    not(accountTakenElsewhere(p, o)),
    owners.get(o).account != p,
    owners' = owners.set(o, { ...owners.get(o), account: p }),
    linkedHeld' = linkedHeld.set(o, holdsKeyOf(p, o)),
    firstAccount' = if (firstAccount.get(o) == NoOne) firstAccount.set(o, p) else firstAccount,
    holder' = holder, made' = made,
    creator' = creator, signedInAgain' = signedInAgain, signInFromLinked' = signInFromLinked,
    signInFromFirst' = signInFromFirst, adminRemoved' = adminRemoved, reset' = reset,
  }

  // rebind: p signs in again, for a new key k they hold, to get back in as owner o.
  action rebind(p: str, o: str, k: int): bool = {
    val row = owners.get(o)
    all {
      row.bound,
      holder.get(k) == p,
      // Link first: `account_not_linked`.
      row.account != NoOne,
      not(RebindChecksAccount) or row.account == p,
      k != row.key,
      owners' = owners.set(o, { ...row, key: k }),
      signedInAgain' = true,
      signInFromLinked' = (signInFromLinked and p == row.account and linkedHeld.get(o)),
      signInFromFirst' = (signInFromFirst and p == firstAccount.get(o)),
      holder' = holder, made' = made,
      creator' = creator, linkedHeld' = linkedHeld, firstAccount' = firstAccount,
      adminRemoved' = adminRemoved, reset' = reset,
    }
  }

  // resetOwner: an admin p, or the operator, binds key k (any key made) to owner o.
  action resetOwner(p: str, o: str, k: int, byOperator: bool): bool = all {
    byOperator or isAdmin(p),
    owners.get(o).bound,
    k < made,
    k != owners.get(o).key,
    owners' = owners.set(o, { ...owners.get(o), key: k }),
    reset' = true,
    holder' = holder, made' = made,
    creator' = creator, linkedHeld' = linkedHeld, firstAccount' = firstAccount, signedInAgain' = signedInAgain,
    signInFromLinked' = signInFromLinked, signInFromFirst' = signInFromFirst, adminRemoved' = adminRemoved,
  }

  // putAdmin: an admin p, or the operator, makes owner o an admin.
  action putAdmin(p: str, o: str, byOperator: bool): bool = all {
    byOperator or isAdmin(p),
    owners.get(o).bound,
    not(owners.get(o).admin),
    owners' = owners.set(o, { ...owners.get(o), admin: true }),
    holder' = holder, made' = made,
    keepGhosts,
  }

  // deleteAdmin: an admin p (themself too), or the operator, takes owner o's admin flag away.
  action deleteAdmin(p: str, o: str, byOperator: bool): bool = all {
    byOperator or isAdmin(p),
    owners.get(o).admin,
    not(LastAdminKept) or Names.exists(x => x != o and owners.get(x).admin),
    owners' = owners.set(o, { ...owners.get(o), admin: false }),
    adminRemoved' = true,
    holder' = holder, made' = made,
    creator' = creator, linkedHeld' = linkedHeld, firstAccount' = firstAccount, signedInAgain' = signedInAgain,
    signInFromLinked' = signInFromLinked, signInFromFirst' = signInFromFirst, reset' = reset,
  }

  action step = {
    nondet p = People.oneOf()
    nondet o = Names.oneOf()
    nondet k = Keys.oneOf()
    nondet flag = Set(true, false).oneOf()
    any {
      newKey(p),
      lose(p),
      join(p, o, k, flag),
      link(p, o),
      rebind(p, o, k),
      resetOwner(p, o, k, flag),
      putAdmin(p, o, flag),
      deleteAdmin(p, o, flag),
    }
  }

  // ---- properties ----
  val linkFirst = signInFromLinked
  val noTakeover = signInFromFirst
  val accountNeverMoves = Names.forall(o =>
    firstAccount.get(o) == NoOne or owners.get(o).account == firstAccount.get(o))
  val atLeastOneAdmin = Names.exists(o => owners.get(o).admin)

  // ---- witnesses: each expected violated ----
  val neverSignedInAgain = not(signedInAgain)
  val neverLinkedAfterJoining = Names.forall(o => o == "ana" or firstAccount.get(o) == NoOne or not(linkedHeld.get(o)))
  val neverAdminRemoved = not(adminRemoved)
  val neverReset = not(reset)
  // The finding (see the header): expected violated by an admin's reset, then link, of an unlinked owner.
  val linkedAccountIsTheCreators = Names.forall(o =>
    owners.get(o).account == NoOne or owners.get(o).account == creator.get(o))
}

// The server as it is on oby-sphere dev at 1607ad6.
module sphere_owners_dev {
  import sphere_owners(RebindChecksAccount = true, LinkNeedsOwnerKey = true, LinkKeepsAccount = true, LastAdminKept = true).*
}

// Broken: rebind takes a sign-in from any account. linkFirst and noTakeover must fail.
module broken_rebind_any_account {
  import sphere_owners(RebindChecksAccount = false, LinkNeedsOwnerKey = true, LinkKeepsAccount = true, LastAdminKept = true).*
}

// Broken: anyone signed in may link their account to any owner. linkFirst must fail.
module broken_link_without_key {
  import sphere_owners(RebindChecksAccount = true, LinkNeedsOwnerKey = false, LinkKeepsAccount = true, LastAdminKept = true).*
}

// Broken: a link replaces the owner's account. accountNeverMoves and noTakeover must fail.
module broken_link_moves_account {
  import sphere_owners(RebindChecksAccount = true, LinkNeedsOwnerKey = true, LinkKeepsAccount = false, LastAdminKept = true).*
}

// Broken: deleteAdmin without its count. atLeastOneAdmin must fail.
module broken_last_admin_removable {
  import sphere_owners(RebindChecksAccount = true, LinkNeedsOwnerKey = true, LinkKeepsAccount = true, LastAdminKept = false).*
}
