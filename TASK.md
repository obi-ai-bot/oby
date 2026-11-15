SPRINT: 0
ORDER: 66
RISK: 
DESIGN: 
DEPENDS: none
FIXES: none

# feat(factory): the build step can run as an Oby job (factory/bin/oby-build)

The factory's build step can run as a job of a separate, factory-only obid instead of a builder subagent, so builds spread over the owner's Claude Code, Codex and Cursor accounts and show in Oby's Tasks view. This task adds the script and the builder's brief; wiring the orchestrator to call it is a later change to .claude/skills/factory/SKILL.md, which the owner merges.
