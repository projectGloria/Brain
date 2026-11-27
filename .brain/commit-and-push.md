---
apply_when: always
mode: inline
---
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, commit on `main`, and push to `origin main`. Pushing never releases anything.
- **Releases:** Only through `/gloria-github-release` (`brain release`); never create release tags or releases by hand.
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.
- [has-github-remote] **Release Workflow:** `.github/workflows/release.yml` is a copy of `GloriaSetup/templates/release.yml`; edit only its `build` job (see `.brain/github-actions.md`).