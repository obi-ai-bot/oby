// Projects and the conversations in them, through a delete, a crash and a restart (task A00-70,
// docs/contracts/projects.md). It models, by function:
// - projects::create in core/crates/obid/src/projects.rs (create);
// - projects::change in projects.rs, then Conversations::regroup in core/crates/obid/src/conversation.rs, which
//   gives every conversation in the project its new folders (changeFolders, one step here);
// - Conversations::change with Event::Assign, under the projects' lock (conversation::update), and
//   Conversations::begin with a project (brain::plan), in conversation.rs (assignProject: `assign` is a name Quint keeps);
// - Conversations::change with Event::TakeOff, under the projects' lock too, as conversation::update takes it on
//   every PATCH, so no take-off runs while a delete holds it (takeOff);
// - projects::delete in projects.rs: it takes the projects' lock (deleteStart), applies Event::ProjectGone to
//   each conversation still in the project through Conversations::regroup and Thread::moved, which makes the
//   project's folders its own (deleteStep), then removes the project from projects.json and lets the lock go
//   (deleteEnd);
// - obid stopping at any point, which lets every lock go and changes nothing kept (crash);
// - projects::reconcile, run by lib.rs at start before obid serves: every conversation whose project isn't kept
//   gets Event::ProjectGone (restart). Its other half, giving a conversation in a kept project that project's
//   folders as they are now, changes nothing here, where changeFolders is one step, so it isn't modelled. Its
//   first half has nothing to do here either: a delete takes every conversation out before the project goes, so
//   oneProjectOrNone rests on that order; a conversation naming a project projects.json lacks (the file removed
//   between runs, PROJECTS-9 (a)) is tested in tests/projects_api.rs, not modelled.
// A conversation answers its copy of its project's folders while it has a project, else its own list
// (projects::answered, Thread::summary).
// oneProjectOrNone (PROJECTS-9): every conversation's project is "none" or a kept project. deleteKeepsConversations
// (PROJECTS-8): every conversation answers the folders the owner last set for it (seen): a delete never changes
// them. Both are expected to hold in every sequence up to 12 steps.
// neverDeleted is deleteKeepsConversations' witness: a project with conversations in it is deleted, each taken
// out by deleteStep, as no take-off runs during a delete. neverRestartedMidDelete is oneProjectOrNone's: obid
// crashes while a delete holds the lock, after it took at least one conversation out and before the project
// goes, and starts again. Both are expected to be violated, so a pass of the invariants is not vacuous.
// The limit: a model checks the design, not that projects.rs and conversation.rs implement it, and changeFolders is
// one step here where the code writes projects.json first (the start's reconcile refreshes a copy a stop left
// behind). A conversation's own folders change only at init here (PATCH folders and begin with folders, outside a
// project, aren't modelled): init picks them freely. Nothing here replays a trace against the code (Quint Connect
// is not used).
// verify: main=projects steps=12 invariants=oneProjectOrNone,deleteKeepsConversations witnesses=neverDeleted,neverRestartedMidDelete
module projects {
  pure val PROJECTS = Set("p1", "p2")
  pure val CONVERSATIONS = Set("c1", "c2")
  pure val FOLDERS = Set("f1", "f2", "f3")
  pure val NONE = "none"

  var kept: Set[str]
  var projectFolders: str -> Set[str]
  var project: str -> str
  var own: str -> Set[str]
  var copy: str -> Set[str]
  var deleting: str
  var up: bool
  // Ghosts: what the owner last set each conversation to answer, and the flags the witnesses read.
  var seen: str -> Set[str]
  var hadConversations: bool
  var tookOneOut: bool
  var deletedWithConversations: bool
  var crashedMidDelete: bool
  var restartedMidDelete: bool

  def answered(c: str): Set[str] = if (project.get(c) != NONE) copy.get(c) else own.get(c)

  action init = {
    nondet owns = CONVERSATIONS.setOfMaps(FOLDERS.powerset()).oneOf()
    all {
      kept' = Set(),
      projectFolders' = PROJECTS.mapBy(p => Set()),
      project' = CONVERSATIONS.mapBy(c => NONE),
      own' = owns,
      copy' = CONVERSATIONS.mapBy(c => Set()),
      deleting' = NONE,
      up' = true,
      seen' = owns,
      hadConversations' = false,
      tookOneOut' = false,
      deletedWithConversations' = false,
      crashedMidDelete' = false,
      restartedMidDelete' = false,
    }
  }

  action keepFlags = all {
    hadConversations' = hadConversations,
    tookOneOut' = tookOneOut,
    deletedWithConversations' = deletedWithConversations,
    crashedMidDelete' = crashedMidDelete,
    restartedMidDelete' = restartedMidDelete,
  }

  action create = {
    nondet p = PROJECTS.oneOf()
    nondet folders = FOLDERS.powerset().oneOf()
    all {
      up, deleting == NONE, not(kept.contains(p)),
      kept' = kept.union(Set(p)),
      projectFolders' = projectFolders.set(p, folders),
      project' = project, own' = own, copy' = copy, deleting' = deleting, up' = up, seen' = seen,
      keepFlags,
    }
  }

  action changeFolders = {
    nondet p = PROJECTS.oneOf()
    nondet folders = FOLDERS.powerset().oneOf()
    all {
      up, deleting == NONE, kept.contains(p),
      kept' = kept,
      projectFolders' = projectFolders.set(p, folders),
      project' = project, own' = own,
      copy' = CONVERSATIONS.mapBy(c => if (project.get(c) == p) folders else copy.get(c)),
      seen' = CONVERSATIONS.mapBy(c => if (project.get(c) == p) folders else seen.get(c)),
      deleting' = deleting, up' = up,
      keepFlags,
    }
  }

  action assignProject = {
    nondet c = CONVERSATIONS.oneOf()
    nondet p = PROJECTS.oneOf()
    all {
      up, deleting == NONE, kept.contains(p),
      kept' = kept, projectFolders' = projectFolders,
      project' = project.set(c, p), own' = own,
      copy' = copy.set(c, projectFolders.get(p)),
      seen' = seen.set(c, projectFolders.get(p)),
      deleting' = deleting, up' = up,
      keepFlags,
    }
  }

  action takeOff = {
    nondet c = CONVERSATIONS.oneOf()
    all {
      up, deleting == NONE,
      kept' = kept, projectFolders' = projectFolders,
      project' = project.set(c, NONE), own' = own,
      copy' = copy.set(c, Set()),
      seen' = seen.set(c, own.get(c)),
      deleting' = deleting, up' = up,
      keepFlags,
    }
  }

  action deleteStart = {
    nondet p = PROJECTS.oneOf()
    all {
      up, deleting == NONE, kept.contains(p),
      kept' = kept, projectFolders' = projectFolders,
      project' = project, own' = own, copy' = copy, seen' = seen,
      deleting' = p, up' = up,
      hadConversations' = CONVERSATIONS.exists(c => project.get(c) == p),
      tookOneOut' = false,
      deletedWithConversations' = deletedWithConversations,
      crashedMidDelete' = crashedMidDelete,
      restartedMidDelete' = restartedMidDelete,
    }
  }

  // Event::ProjectGone for one conversation in the project being deleted: its own list becomes its copy.
  action deleteStep = {
    nondet c = CONVERSATIONS.oneOf()
    all {
      up, deleting != NONE, project.get(c) == deleting,
      kept' = kept, projectFolders' = projectFolders,
      project' = project.set(c, NONE),
      own' = own.set(c, copy.get(c)),
      copy' = copy.set(c, Set()),
      seen' = seen, deleting' = deleting, up' = up,
      hadConversations' = hadConversations,
      tookOneOut' = true,
      deletedWithConversations' = deletedWithConversations,
      crashedMidDelete' = crashedMidDelete,
      restartedMidDelete' = restartedMidDelete,
    }
  }

  action deleteEnd = all {
    up, deleting != NONE, CONVERSATIONS.forall(c => project.get(c) != deleting),
    kept' = kept.exclude(Set(deleting)),
    projectFolders' = projectFolders,
    project' = project, own' = own, copy' = copy, seen' = seen,
    deleting' = NONE, up' = up,
    hadConversations' = hadConversations,
    tookOneOut' = tookOneOut,
    deletedWithConversations' = (deletedWithConversations or hadConversations),
    crashedMidDelete' = crashedMidDelete,
    restartedMidDelete' = restartedMidDelete,
  }

  action crash = all {
    kept' = kept, projectFolders' = projectFolders,
    project' = project, own' = own, copy' = copy, seen' = seen,
    deleting' = NONE, up' = false,
    hadConversations' = hadConversations,
    tookOneOut' = tookOneOut,
    deletedWithConversations' = deletedWithConversations,
    crashedMidDelete' = (crashedMidDelete or (deleting != NONE and tookOneOut)),
    restartedMidDelete' = restartedMidDelete,
  }

  // The start's reconcile: Event::ProjectGone for each conversation whose project isn't kept.
  action restart = {
    val gone = CONVERSATIONS.filter(c => project.get(c) != NONE and not(kept.contains(project.get(c))))
    all {
      not(up),
      kept' = kept, projectFolders' = projectFolders,
      project' = CONVERSATIONS.mapBy(c => if (gone.contains(c)) NONE else project.get(c)),
      own' = CONVERSATIONS.mapBy(c => if (gone.contains(c)) copy.get(c) else own.get(c)),
      copy' = CONVERSATIONS.mapBy(c => if (gone.contains(c)) Set() else copy.get(c)),
      seen' = seen, deleting' = NONE, up' = true,
      hadConversations' = hadConversations,
      tookOneOut' = tookOneOut,
      deletedWithConversations' = deletedWithConversations,
      crashedMidDelete' = crashedMidDelete,
      restartedMidDelete' = (restartedMidDelete or crashedMidDelete),
    }
  }

  action step = any { create, changeFolders, assignProject, takeOff, deleteStart, deleteStep, deleteEnd, crash, restart }

  val oneProjectOrNone = CONVERSATIONS.forall(c => project.get(c) == NONE or kept.contains(project.get(c)))
  val deleteKeepsConversations = CONVERSATIONS.forall(c => answered(c) == seen.get(c))
  val neverDeleted = not(deletedWithConversations)
  val neverRestartedMidDelete = not(restartedMidDelete)
}
