# AGENTS.md — Brainz (agent-brain)

Personal AI coding-agent rule compiler, multi-tool skill generator, SHA-256 sync CLI (Windows PowerShell wrapped in a `.bat` entry point), and Tauri v2 desktop app for managing it.

## Commands
- Install / Compile Skills & Sync Master Brain: `.\install-brain.bat`
- Check Repo Sync Status: `.\install-brain.bat check .`
- Check Sibling Repos: `.\install-brain.bat check-all [dir]`
- Sync Master Brain to Project `.brain/`: `.\install-brain.bat sync [dir]`
- Print Sync Marker Hash: `.\install-brain.bat hash [dir]`
- Remove Legacy Antigravity Skill: `.\install-brain.bat remove-legacy`
- Run Desktop App (dev): `npm run tauri:dev`
- Type-check & Build Frontend: `npm run build`
- Build Desktop App: `npm run tauri:build`
- Rust Tests & Lints: `cargo test` and `cargo clippy --all-targets` (run in `src-tauri/`)

## Architecture
- `install-brain.bat` — Hybrid batch/PowerShell CLI (`# --- POWERSHELL_START ---` payload) that compiles `brain/init.md` into the 3 tool `gloria-init-brain/SKILL.md` files, mirrors `brain/*.md` to `%USERPROFILE%\.brain\`, installs all skills under `skills/<tool>/*`, and computes/verifies the 12-char `brain-sync` SHA-256 fingerprint.
- `brain/` — Canonical source of truth for personal rules (`workflow.md`, `commit-and-push.md`, `ui-preferences.md`, `building-the-project.md`, `github-actions.md`) and the tool-agnostic `init.md` prompt. Edit rules and the `gloria-init-brain` prompt here, never only in `skills/`.
- `skills/` — Per-tool skill directories for `claude-code/`, `codex/`, and `antigravity/` containing `gloria-init-brain/` (generated from `brain/init.md`), `gloria-github-release/`, `gloria-github-repo-about/`, and `gloria-github-repo-readme/`.
- `src-tauri/src/` — Tauri v2 Rust backend: `brain_core.rs` (fingerprint, drift, `.brain/` sync), `clis.rs` (CLI detect/install/update via winget, config edit), `gitignore.rs`, `rules.rs`, `skills.rs`, `config.rs`; all commands in `main.rs` run off the UI thread.
- `src/` — React + TypeScript UI (`views/*View.tsx`, `hooks/useUnsavedGuard.ts`).
- `templates/gitignore` — Shared `.gitignore` draft the UI writes into repos.
- **Version Manifests:** `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (the `brain` package), `src-tauri/tauri.conf.json`, and `gloria.toml` must always carry the same version.

## Personal Rules (Brain)
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until explicitly told otherwise.
- **Versioning:** Never bump versions during normal work; the user picks the version at release time and `/gloria-github-release` sets it in every version file at once. Otherwise keep version files in sync.
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, commit on `main`, and push to `origin main`. Pushing never releases anything.
- **Releases:** Only through `/gloria-github-release` (bumps the version, pushes, runs `gh workflow run release.yml`, and watches until the GitHub Release is live); never create release tags or releases by hand.
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.
- **Release Workflow:** `.github/workflows/release.yml` must be a copy of `GloriaSetup/templates/release.yml` (shared check/release jobs plus this repo's own `build` job); never hand-write it or pin `gloria-setup`.
- **Window Chrome:** Frameless titlebar with a drag region, double-click to maximize, and custom min/max/close controls.
- **Desktop Shell Lock:** Lock the shell to `100vw x 100vh` with `overflow: hidden`, use internal scroll panes, and set `user-select: none` on UI chrome.
- **Scrollbars:** No raw OS scrollbars; use thin 4–6px themed scrollbars with transparent tracks.
- **Interactive Controls:** Custom-style every control with hover, active, focus-visible, and disabled states and 120–180ms transitions.

## Playbooks (Read Before Task)
- **Building & Packaging:** Before packaging, compiling release binaries, or building an installer, read `.brain/building-the-project.md`.
- **GitHub Actions & Releases:** Before creating/modifying `.github/workflows/*.yml`, setting up release automation, or debugging a failed release, read `.brain/github-actions.md`.

## Repo Guardrails & Gotchas
- **Generated Skill Files:** Always edit `brain/init.md` and run `.\install-brain.bat` to regenerate `skills/*/gloria-init-brain/SKILL.md`; direct edits to `gloria-init-brain/SKILL.md` under `skills/` will be overwritten on the next install.
- **Init Marker Anchor:** `Sync-SkillFilesFromInit` in `install-brain.bat` slices `brain/init.md` at `## Non-Negotiable Rules`; renaming or removing that heading breaks skill compilation.
- **Fingerprint Normalization:** `Get-BrainFingerprint` normalizes line endings to LF and trims trailing newlines per file before SHA-256 hashing; always verify sync hashes via `.\install-brain.bat check .` or `hash`.

## Done Criteria
- After editing `brain/` or `install-brain.bat`, run `.\install-brain.bat` to regenerate `skills/*/gloria-init-brain/SKILL.md`, install all skills, and update `%USERPROFILE%\.brain\`, then run `.\install-brain.bat check .` to verify `AGENTS.md` matches the master fingerprint and stays within 120 lines.
- After app changes, `npm run build` and `cargo test` / `cargo clippy --all-targets` (in `src-tauri/`) pass with no warnings, and all version manifests agree.

<!-- brain-sync: dbdb119dacdc -->
