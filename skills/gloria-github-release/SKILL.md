---
name: gloria-github-release
description:  Releases the current projectGloria repo end to end with Brain's `brain release` command - asks for the version, bumps every version file, installs the shared GloriaSetup release.yml, commits, pushes, dispatches and watches the workflow, and on a code failure fixes the code and retries until the GitHub Release is live. Trigger when the user invokes gloria-github-release or asks to cut/publish a new release.
---

# Release a projectGloria repo

Running this skill is the user's request to commit, push and release. `brain` = `"%USERPROFILE%\.brain\brain.cmd"`.

1. Run `brain release` in the repo root (uncommitted changes are not released: commit the ones that belong in this release first): it prints the current version, files that disagree, the last release, the release setup and a suggested version. Ask `What should the new version be? (e.g. <suggested>)` unless the user already gave one (`patch` / `minor` also work).
2. Run `brain release <version> --watch` in the repo root. Brain bumps every version file, bootstraps/migrates `release.yml` from GloriaSetup, commits only the release files, pushes `main`, dispatches, watches, and auto-fixes formatting, lockfiles and version drift.
3. On `FAILED`, read the printed diagnosis and log lines, fix the code (never the shared `check`/`release` jobs, never pin `gloria-setup`; a broken shared job is GloriaSetup's to fix, so stop and tell the user), commit the fix (`git add <files>`, `git commit -m "<one plain sentence>"`; Brain commits only its own release files), then run `brain release --retry --watch`. Stop after 3 failed attempts and report why.
4. Report the release URL and assets only when Brain prints `Released v<version>`.

Never create tags or releases by hand, never create branches. Details: `.brain/github-actions.md`.
