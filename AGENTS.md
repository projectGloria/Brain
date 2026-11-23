# AGENTS.md — Brainz (agent-brain)

Personal AI coding-agent rule compiler, multi-tool skill generator, and SHA-256 sync CLI (Windows PowerShell wrapped in a `.bat` entry point).

## Commands
- Install / Compile Skills & Sync Master Brain: `.\install-brain.bat`
- Check Repo Sync Status: `.\install-brain.bat check .`
- Check Sibling Repos: `.\install-brain.bat check-all [dir]`
- Sync Master Brain to Project `.brain/`: `.\install-brain.bat sync [dir]`
- Print Sync Marker Hash: `.\install-brain.bat hash [dir]`
- Remove Legacy Antigravity Skill: `.\install-brain.bat remove-legacy`

## Architecture
- `install-brain.bat` — Hybrid batch/PowerShell CLI (`# --- POWERSHELL_START ---` payload) that compiles `brain/init.md` into the 3 tool `monjita-init-brain/SKILL.md` files, mirrors `brain/*.md` to `%USERPROFILE%\.brain\`, installs all skills under `skills/<tool>/*`, and computes/verifies the 12-char `brain-sync` SHA-256 fingerprint.
- `brain/` — Canonical source of truth for personal rules (`workflow.md`, `commit-and-push.md`, `ui-preferences.md`, `building-the-project.md`, `github-actions.md`) and the tool-agnostic `init.md` prompt. Edit rules and the `monjita-init-brain` prompt here, never only in `skills/`.
- `skills/` — Per-tool skill directories for `claude-code/`, `codex/`, and `antigravity/` containing `monjita-init-brain/` (generated from `brain/init.md`), `monjita-github-release/`, `monjita-github-repo-about/`, and `monjita-github-repo-readme/`.

## Personal Rules (Brain)
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until explicitly told otherwise.
- **Versioning (Minimal Changes):** When runtime code or packaging changes, bump the alpha number by `+1` (e.g., `0.1.0-alpha.1` -> `0.1.0-alpha.2`) across all version manifests in the repo **at most once per uncommitted change set** (check `git diff HEAD` first—if the working tree version is already ahead of `HEAD`, do not bump again until committed). Skip version bumps on docs-only or read-only tasks.
- **Versioning (Major/Minor Changes):** Before changing minor/major versions or dropping `-alpha` on breaking or major feature work, stop and ask for the exact target version.
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, confirm version manifests are bumped and synced for code/packaging changes, commit on `main`, and push to `origin main` (do not create manual release tags—pushing to `main` triggers `.github/workflows/release.yml` to build with `gloria-setup` and publish to GitHub Releases).
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.

## Repo Guardrails & Gotchas
- **Generated Skill Files:** Always edit `brain/init.md` and run `.\install-brain.bat` to regenerate `skills/*/monjita-init-brain/SKILL.md`; direct edits to `monjita-init-brain/SKILL.md` under `skills/` will be overwritten on the next install.
- **Init Marker Anchor:** `Sync-SkillFilesFromInit` in `install-brain.bat` slices `brain/init.md` at `## Non-Negotiable Rules`; renaming or removing that heading breaks skill compilation.
- **Fingerprint Normalization:** `Get-BrainFingerprint` normalizes line endings to LF and trims trailing newlines per file before SHA-256 hashing; always verify sync hashes via `.\install-brain.bat check .` or `hash`.

## Done Criteria
Run `.\install-brain.bat` after editing `brain/` or `install-brain.bat` to regenerate `skills/*/monjita-init-brain/SKILL.md`, install all skills, and update `%USERPROFILE%\.brain\`, then run `.\install-brain.bat check .` to verify `AGENTS.md` matches the master fingerprint and stays within 120 lines.

<!-- brain-sync: 5f20fae6c0e6 -->
