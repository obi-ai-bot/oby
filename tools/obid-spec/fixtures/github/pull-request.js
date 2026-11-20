// The one pull request of the recording gh's GitHub (routes.json beside it, core/crates/obid/tests/
// fakes/gh), as GitHub keeps it, on the fixture's bare remote (`../../fixture/remote.git` from this
// folder, `gh-recording` beside the scratch obid's HOME). Run by a route as
// `node pull-request.js <what> <gh's arguments…>`, in this folder:
//
//   list     `gh pr list … --head <branch>`: once `pr create` opened it, the pull request of that
//            branch at the branch's tip on the remote (what obid pushed); before, none
//   create   `gh pr create … --base <b> --head <branch>`: keeps the branch and the base, and prints
//            the pull request's URL, #7
//   view     `gh pr view 7 --json …`: open (merged once `flags/merged` is here), green, clean, its
//            head the branch's tip
//   pull     `gh api repos/<repo>/pulls/7`: the same, as the REST API says it
//   reviews  its reviews: the reviewer's approval of the head once obid posted one (`flags/approved`)
//   commits  its commits' messages
//   merge    `gh api -X PUT repos/<repo>/pulls/7/merge --input <file>`: as GitHub, refuses a `sha`
//            that isn't the head (409) and a head that isn't on the base's tip (405); otherwise
//            squashes the head onto the base on the remote, moving the base, and keeps the input as
//            `merge.json`. (routes.json refuses it before this runs while `flags/refuse-merge` is
//            here, as GitHub does a merge it won't make: 405.)
//
// Never the real GitHub: nothing here reaches a network.

const fs = require("fs");
const { execFileSync } = require("child_process");

const [what, ...args] = process.argv.slice(2);
const REPO = "obi-e2e/fixture-app";
const URL = `https://github.com/${REPO}/pull/7`;
const remote = "../../fixture/remote.git";

const git = (...a) =>
  execFileSync("git", ["--git-dir", remote, ...a], {
    encoding: "utf8",
    env: {
      PATH: process.env.PATH,
      GIT_AUTHOR_NAME: "GitHub",
      GIT_AUTHOR_EMAIL: "noreply@github.invalid",
      GIT_COMMITTER_NAME: "GitHub",
      GIT_COMMITTER_EMAIL: "noreply@github.invalid",
    },
  }).trim();
const kept = (name) => (fs.existsSync(name) ? fs.readFileSync(name, "utf8").trim() : "");
const flag = (name) => fs.existsSync(`flags/${name}`);
const after = (name) => {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1];
};
const say = (value) => process.stdout.write(JSON.stringify(value) + "\n");
const refuse = (words) => {
  process.stderr.write(words + "\n");
  process.exit(1);
};

const branch = kept("branch");
const base = kept("base") || "main";
// Once merged GitHub keeps the head it merged, whatever happens to the branch after.
const head = kept("merged-head") || (branch ? git("rev-parse", `refs/heads/${branch}`) : "");
const merged = flag("merged");
const checks = [{ __typename: "CheckRun", name: "test", status: "COMPLETED", conclusion: "SUCCESS" }];

switch (what) {
  case "list":
    if (flag("opened") && after("--head") === branch)
      say([{ url: URL, isCrossRepository: false, headRefOid: head, baseRefName: base }]);
    break;
  case "create":
    fs.writeFileSync("branch", after("--head"));
    fs.writeFileSync("base", after("--base") || "main");
    process.stdout.write(`Creating pull request…\n${URL}\n`);
    break;
  case "view":
    say({
      state: merged ? "MERGED" : "OPEN",
      reviewDecision: "",
      mergedAt: merged ? new Date().toISOString() : null,
      baseRefName: base,
      headRefOid: head,
      statusCheckRollup: checks,
      mergeable: "MERGEABLE",
      mergeStateStatus: "CLEAN",
      author: { login: "alex" },
    });
    break;
  case "pull":
    say({
      state: merged ? "closed" : "open",
      merged,
      title: "The job's change",
      base: { ref: base, repo: { full_name: REPO } },
      head: { ref: branch, sha: head, repo: { full_name: REPO } },
      mergeable: true,
      mergeable_state: "clean",
    });
    break;
  case "reviews":
    say([
      flag("approved")
        ? [{ id: 1, user: { login: "oby-reviewer", type: "User" }, state: "APPROVED", commit_id: head,
             author_association: "COLLABORATOR", body: "" }]
        : [],
    ]);
    break;
  case "commits":
    say([
      git("log", "--format=%B%x00", `refs/heads/${base}..${head}`)
        .split("\0")
        .map((m) => m.trim())
        .filter(Boolean)
        .map((message) => ({ commit: { message } })),
    ]);
    break;
  case "merge": {
    if (merged) refuse("gh: Pull Request is not mergeable (HTTP 405)");
    const input = JSON.parse(fs.readFileSync(after("--input"), "utf8"));
    if (input.sha && input.sha !== head) refuse("gh: Head branch was modified. Review and try the merge again. (HTTP 409)");
    const tip = git("rev-parse", `refs/heads/${base}`);
    try {
      git("merge-base", "--is-ancestor", tip, head);
    } catch {
      refuse("gh: Pull Request is not mergeable (HTTP 405)");
    }
    const message = [input.commit_title, input.commit_message].filter(Boolean).join("\n\n") || "Merge #7";
    const squashed = git("commit-tree", `${head}^{tree}`, "-p", tip, "-m", message);
    git("update-ref", `refs/heads/${base}`, squashed, tip);
    fs.writeFileSync("merge.json", JSON.stringify(input));
    fs.writeFileSync("merged-head", head);
    fs.mkdirSync("flags", { recursive: true });
    fs.writeFileSync("flags/merged", "");
    say({ sha: squashed, merged: true, message: "Pull Request successfully merged" });
    break;
  }
  default:
    refuse(`pull-request.js: ${what}?`);
}
