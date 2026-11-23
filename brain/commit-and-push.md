---
apply_when: always
mode: inline
---
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, confirm version manifests are bumped and synced for code/packaging changes, commit on `main`, and push to `origin main` (do not create manual release tags—pushing to `main` triggers `.github/workflows/release.yml` to build with `gloria-setup` and publish to GitHub Releases).
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.
- [windows-app] **Release Workflow Check:** Before pushing to `main`, check `.github/workflows/release.yml` against `.brain/github-actions.md` (runs on `push: branches: [main]`, builds and releases only when the version changed). If it's missing or set up differently, bring it in line in the same push and tell the user what changed.