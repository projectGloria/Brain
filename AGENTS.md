# AGENTS.md — Brain

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
- `skills/` — Per-tool skill directories for `claude-code/`, `codex/`, and `antigravity/` containing `gloria-init-brain/` (generated from `brain/init.md`), `gloria-create-tools/`, `gloria-github-release/`, `gloria-github-repo-about/`, and `gloria-github-repo-readme/`.
- `src-tauri/src/` — Tauri v2 Rust backend: `brain_core.rs` (fingerprint, drift, `.brain/` sync), `agent_runs.rs` (non-interactive CLI runner streaming `agent-run-*` events), `clis.rs` (CLI detect/install/update via winget, config edit), `dna.rs` (repo stack analyzer & `gloriaTools/` generator), `gitignore.rs`, `rules.rs`, `skills.rs`, `config.rs`; all commands in `main.rs` run off the UI thread.
- `src/` — React + TypeScript UI (`views/*View.tsx`, `components/`, `hooks/` including `useAgentRuns.ts` and `useUnsavedGuard.ts`).
- `templates/gitignore` — Shared `.gitignore` draft the UI writes into repos.
- **Version Manifests:** `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (the `brain` package), `src-tauri/tauri.conf.json`, and `gloria.toml` must always carry the same version.

## Personal Rules (Brain)
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until explicitly told otherwise.
- **Versioning:** Never bump versions during normal work; the user picks the version at release time and `/gloria-github-release` sets it in every version file at once. Keep version files in sync with each other otherwise.
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, commit on `main`, and push to `origin main`. Pushing never releases anything.
- **Releases:** Only through `/gloria-github-release` (bumps the version, pushes, runs `gh workflow run release.yml`, and watches until the GitHub Release is live); never create release tags or releases by hand.
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.
- **Release Workflow:** `.github/workflows/release.yml` must be a copy of `GloriaSetup/templates/release.yml` (shared `check`/`release` jobs plus this repo's own `build` job); never hand-write it or pin `gloria-setup`—see `.brain/github-actions.md`.
- **Window Chrome:** Disable native OS titlebars; build an integrated frameless titlebar with drag region, double-click maximize, and custom min/max/close controls.
- **Desktop Shell Lock:** Lock the app shell to `100vw x 100vh` (`overflow: hidden`) with internal scroll panes and `user-select: none` on UI chrome.
- **Scrollbars:** Ban raw OS scrollbars; use thin (4–6px) themed scrollbars with transparent tracks and zero layout shift.
- **Interactive Controls:** Custom-style all form controls; define explicit `hover`, `active`, `focus-visible`, and `disabled` states with snappy 120–180ms transitions.

## Playbooks (Read Before Task)
- **Building & Packaging:** Read `.brain/building-the-project.md` before packaging, compiling release binaries, or building an installer.
- **GitHub Actions & Releases:** Read `.brain/github-actions.md` before creating/modifying `.github/workflows/*.yml`, setting up release automation, or debugging a failed release.

## Repo Guardrails & Gotchas
- **Generated Skill Files:** Always edit `brain/init.md` and run `.\install-brain.bat` to regenerate `skills/*/gloria-init-brain/SKILL.md`; direct edits to `gloria-init-brain/SKILL.md` under `skills/` will be overwritten on the next install.
- **Init Marker Anchor:** `Sync-SkillFilesFromInit` in `install-brain.bat` slices `brain/init.md` at `## Non-Negotiable Rules`; renaming or removing that heading breaks skill compilation.
- **Fingerprint Normalization:** `Get-BrainFingerprint` normalizes line endings to LF and trims trailing newlines per file before SHA-256 hashing; always verify sync hashes via `.\install-brain.bat check .` or `hash`.

## Done Criteria
- After editing `brain/` or `install-brain.bat`, run `.\install-brain.bat` to regenerate `skills/*/gloria-init-brain/SKILL.md`, install all skills, and update `%USERPROFILE%\.brain\`, then run `.\install-brain.bat check .` to verify `AGENTS.md` matches the master fingerprint and stays within 120 lines.
- After app changes, `npm run build` and `cargo test` / `cargo clippy --all-targets` (in `src-tauri/`) pass with no warnings, and all version manifests agree.

<!-- brain-sync: dbdb119dacdc -->