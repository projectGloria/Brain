# AGENTS.md — Brain

Personal AI coding-agent rule compiler and release/sync CLI for projectGloria repos, shipped as one Tauri v2 app (`Brain.exe`: window with no args, headless CLI with a command). Windows-first.

## Commands
- Install rules + skills: `.\install-brain.bat` (= `brain install`; the launcher finds `Brain.exe` or runs `cargo build --release`)
- Check / sync a repo: `brain check .`, `brain check --all [root]`, `brain sync [path] [--accept-repo]`
- Release: `brain release [path]` (preview), `brain release <x.y.z|patch> --watch`, `brain release --retry --watch`, `brain release --all --patch --plan`
- Dev tools: `brain tools [path] --plan`, `brain tools [--only dev,check]`
- Run app (dev): `npm run tauri:dev`; build: `npm run tauri:build`
- Frontend typecheck/build: `npm run build`
- Rust tests/lints (in `src-tauri/`): `cargo test`, `cargo clippy --all-targets`

## Architecture
- `src-tauri/src/main.rs` — Tauri commands (all off the UI thread); `main()` hands off to `cli::run_from_args()` first.
- `src-tauri/src/cli.rs` — headless CLI (exit 0 ok, 1 error, 2 needs attention); attaches the parent console only when stdout is not redirected.
- `src-tauri/src/brain_core.rs` — fingerprint (SHA-256 over sorted name+content, 12 hex), status (NO MARKER / BRAIN DRIFT / REPO DRIFT / OVER BUDGET / UP TO DATE), `.brain/` mirror.
- `src-tauri/src/compile.rs` — owns `## Personal Rules (Brain)`, `## Playbooks (Read Before Task)` and the `brain-sync` marker line.
- `src-tauri/src/release.rs` — release pipeline behind a `Sink` trait (UI events or CLI output); reads `release.yml` from `../GloriaSetup/templates/` (else GitHub via `gh api`), never a hardcoded copy.
- `src-tauri/src/skills.rs` — installs `skills/<name>/` for every tool target; renders `gloria-init-brain` from `brain/init.md`.
- `src-tauri/src/dna.rs` (stack analyzer, `gloriaTools/` generator), `gitignore.rs`, `rules.rs`, `clis.rs`, `agent_runs.rs`, `config.rs`.
- `brain/` — source of truth for personal rules and `init.md`; mirrored to `%USERPROFILE%\.brain\`.
- `src/` — React + TypeScript UI (`views/*View.tsx`, `components/`, `hooks/`, `types.ts` mirrors Rust structs).
- **Version Manifests:** `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (`brain` package), `src-tauri/tauri.conf.json`, `gloria.toml` (always the same version).

## Personal Rules (Brain)
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, commit on `main`, and push to `origin main`. Pushing never releases anything.
- **Releases:** Only through `/gloria-github-release` (`brain release`); never create release tags or releases by hand.
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.
- **Release Workflow:** `.github/workflows/release.yml` is a copy of `GloriaSetup/templates/release.yml`; edit only its `build` job (see `.brain/github-actions.md`).
- **Window Chrome:** Disable native OS titlebars; build an integrated frameless titlebar with drag region, double-click maximize, and custom min/max/close controls.
- **Desktop Shell Lock:** Lock the app shell to `100vw x 100vh` (`overflow: hidden`) with internal scroll panes and `user-select: none` on UI chrome.
- **Scrollbars:** Ban raw OS scrollbars; use thin (4–6px) themed scrollbars with transparent tracks and zero layout shift.
- **Interactive Controls:** Custom-style all form controls; define explicit `hover`, `active`, `focus-visible`, and `disabled` states with snappy 120–180ms transitions.
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until explicitly told otherwise.
- **Versioning:** Never bump versions during normal work. The user picks the version at release time and `/gloria-github-release` sets it in every version file at once; keep version files in sync with each other otherwise.

## Playbooks (Read Before Task)
- **Building & Packaging:** Read `.brain/building-the-project.md` before packaging, compiling release binaries, or building an installer.
- **GitHub Actions & Releases:** Read `.brain/github-actions.md` before creating/modifying `.github/workflows/*.yml`, setting up release automation, or debugging a failed release.

## Repo Guardrails & Gotchas
- **Init Skill Source:** edit `brain/init.md`, never an installed `gloria-init-brain/SKILL.md`; `skills.rs` slices it at `## Non-Negotiable Rules`, so keep that heading.
- **Fingerprint Changes:** any edit to `brain/*.md` changes the fingerprint; run `brain install` then `brain sync` so this repo (and others) stop showing BRAIN DRIFT.
- **Deterministic Work in Code:** sync, release, version bumps and tool generation belong in Rust; skills only call `brain` and handle judgment calls.
- **Rust/TS Contract:** when a serialized Rust struct changes, update `src/types.ts` in the same change.

## Done Criteria
- `cargo test` and `cargo clippy --all-targets` (in `src-tauri/`) and `npm run build` pass with no warnings.
- After editing `brain/` or `skills/`: `brain install`, then `brain check .` reports UP TO DATE within 120 lines.
- All version manifests agree.

<!-- brain-sync: 5c5ab84327c4 repo: ad83871fa3b2 -->
