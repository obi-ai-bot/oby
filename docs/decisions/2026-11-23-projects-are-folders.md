# A project is made of folders: no Add from GitHub…

- **Date:** 2026-11-23
- **Status:** In force
- **Replaced:** The project sheet's "Add from GitHub…" (A00-71, Sangam's screen 02, `docs/design/oby-home/02-new-project.png`), which listed `GET /v1/repos` and added the one chosen as a row "GitHub"
- **Changed by:** Nothing
- **Contracts and scenarios:** [projects.md](../contracts/projects.md) (PROJECTS-20, PROJECTS-26, PROJECTS-27, PROJECTS-28)

## The owner's words

> "On the new project remove the button and the entire logic for Add From GitHub; the idea is that you add the folder and that's it!"

The owner's words on a screenshot of the New project sheet, as the orchestrator relayed them.

## Where it is stated

- [projects.md](../contracts/projects.md), "Actors and ownership", PROJECTS-20, PROJECTS-26, PROJECTS-27 and PROJECTS-28

## Notes

The New project and Edit project sheets offer "Add a folder…" only (with "Delete project…", "Cancel" and "Save" or "Create project" as before); nothing in the window adds a repository to a project. A project kept with repositories before this still opens and works: its repositories show in the sheet as rows "GitHub", each with its ×, and a save that doesn't touch them keeps them. obid is unchanged (its API only grows): `repos` is still a project's key on `POST`/`PATCH /v1/projects`, and `GET /v1/repos` still answers, the app still reading it for other screens. Sangam's screen 02 draws the button; the owner's word decides here, and his sheet's words ("Folders and repositories", its lede) stay as he wrote them.
