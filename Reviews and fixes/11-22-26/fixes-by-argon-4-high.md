# Fixes Applied — Agent Brain (11-22-26)

**Author:** Gemini 4 Argon (High)

---

## 1. Core Compiler Specification Updates (`brain/init.md` & 3 `SKILL.md` Files)

Updated `brain/init.md`, `skills/claude-code/init-brain/SKILL.md`, `skills/codex/init-brain/SKILL.md`, and `skills/antigravity/init-brain/SKILL.md` with the following improvements:

- **Managed Mirror Sync / No Ghost Files (Rule 4):**
  - `<project>/.brain/` is now synced as a managed mirror of `~/.brain/`: master `.md` files (excluding `init*.md`) overwrite same-named project files, and any stale file in `<project>/.brain/` that no longer exists in `~/.brain/` is automatically pruned.
  - Project-only untracked playbooks are supported via the `local-*.md` naming convention (e.g., `.brain/local-notes.md`), which the sync step preserves.
- **Deterministic Section Provenance on Refresh (Rule 6 & Phase 1.4 / 2.2):**
  - `## Personal Rules (Brain)` and `## Playbooks (Read Before Task)` are now strictly 100% owned by `.brain/` and regenerated from scratch on every refresh—old bullets in those sections are never carried over.
  - `## Repo Guardrails & Gotchas` is strictly 100% repo-specific (`Human-Active`) and preserved across refreshes.
  - **Topic Override Rule:** Any bullet in `## Repo Guardrails & Gotchas` sharing the exact `**<Topic>:**` prefix of a `.brain/` rule automatically overrides and suppresses the `.brain/` rule (reported under `Overrides` in Phase 4).
- **Per-Bullet Trait Tags & Expanded Traits (Rule 3 & Phase 1.2):**
  - Added support for per-bullet trait filters (`- [trait1, trait2] **<Topic>:** ...`) inside `mode: inline` brain files. Matching bullets have their `[...]` prefix stripped on compilation; non-matching bullets are skipped.
  - Added `browser-extension` and `android-app` to the recognized repository traits.
- **Reverse-Flow Learn Mode (`/init-brain learn <rule>`, Phase 0.1):**
  - Added `learn <rule>` routing so new lessons discovered mid-session can be persisted directly to `<project>/AGENTS.md` (`## Repo Guardrails & Gotchas`) or promoted to `~/.brain/<file>.md` and synced.
- **Explicit Version Manifests Slot (Phase 1.5 & Phase 3):**
  - Added `- **Version Manifests:** \`<path1>\`, \`<path2>\`` under `## Architecture` in the `AGENTS.md` template so agents update `Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`, and `gloria.toml` together in lockstep.
- **Deterministic LF-Normalized Fingerprint & Backtick-Aware Stub Guard (Rule 3 & Phase 0.3):**
  - Standardized the 12-char SHA-256 `brain-sync` fingerprint over all post-sync `.brain/*.md` files (excluding `init*.md`), sorted by lowercase filename with LF-normalized (`\n`) line endings.
  - Clarified the Stub Guard so backticked CLI placeholders (e.g., `` `dist/<Name>_<version>_x64-setup.exe` ``) and arrows (`->`) are never mistaken for unfilled `<...>` template stubs.

---

## 2. Filled & Refined Master Brain Rule Files (`brain/*.md`)

- **`brain/workflow.md`:**
  - Replaced the ambiguous "on every minimal change" rule with a **working-tree version guard**: bump the alpha number by `+1` across all version manifests **at most once per uncommitted change set** (check `git diff HEAD` first—if the working tree version is already ahead of `HEAD`, do not bump again until committed), and skip version bumps on docs-only or read-only tasks.
- **`brain/ui-preferences.md`:**
  - Expanded `apply_when` to `desktop-gui, web-ui, browser-extension`.
  - Scoped `Window Chrome` and `Desktop Shell Lock` (`100vw x 100vh`) with `[desktop-gui]` per-bullet tags so pure web apps and browser extensions only receive the `Scrollbars` and `Interactive Controls` rules.
- **`brain/commit-and-push.md`:**
  - Replaced stubs with the exact commit format (one plain sentence stating what changed and why; no `feat:`/`fix:`/`chore:` prefixes), commit/push safety policy (never commit/tag/push unless asked, never force-push, require build+tests to pass), and path privacy rule (`C:\Users\you` in docs and examples).
- **`brain/building-the-project.md`:**
  - Filled in the complete **GloriaSetup** packaging playbook: `./gloria.toml` manifest schema and version lockstep requirement, `gloria-pack` command producing `dist/<Name>_<version>_x64-setup.exe`, per-user `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\projectGloria.<id>` contract for `GloriaLauncher`, custom app `.ico` enforcement, offline WebView2 bootstrapper rule, and PowerShell `Start-Process ... -ArgumentList '/S --dry-run'` verification.
- **`brain/github-actions.md`:**
  - Filled in the 3-stage `.github/workflows/release.yml` release pipeline (`version` diff check against `github.event.before` on `main` + `workflow_dispatch` `dry_run` → `build` on `windows-latest` with `projectGloria/GloriaSetup` and `/S --dry-run` check → `publish` on `ubuntu-latest` with `SHA256SUMS.txt` verification, `--prerelease` detection for `-alpha` versions, full commit-SHA action pinning, least-privilege `permissions`, and explicit `shell: pwsh` / `shell: bash`).

---

## 3. Created `install-brain.bat` & Updated `README.md`

Created `install-brain.bat` at the repository root (and automatically copied into `%USERPROFILE%\.brain\install-brain.bat`) to handle all mechanical tasks deterministically:

- **`install-brain.bat` (default):**
  1. Compiles `brain/init.md` into `skills/claude-code/init-brain/SKILL.md`, `skills/codex/init-brain/SKILL.md`, and `skills/antigravity/init-brain/SKILL.md` automatically so the four files never drift out of sync.
  2. Installs `brain/*.md` (and `install-brain.bat`) into `C:\Users\you\.brain\`, pruning any deleted ghost files.
  3. Installs the `init-brain` skill into Claude Code (`C:\Users\you\.claude\skills\init-brain\SKILL.md`), Codex (`C:\Users\you\.agents\skills\init-brain\SKILL.md`), and Antigravity (`C:\Users\you\.gemini\config\skills\init-brain\SKILL.md`).
  4. Computes and prints the master `<!-- brain-sync: <12-char-sha256> -->` fingerprint and verifies that 0 stub placeholders remain.
- **`install-brain.bat check [dir]`:** Runs the Phase 0 fingerprint, line-budget, and manifest-drift check on a repository in ~50ms without spending LLM tokens.
- **`install-brain.bat check-all [dir]`:** Scans all sibling repositories in `projectGloria` and prints a summary table of `AGENTS.md` sync status and line counts.
- **`install-brain.bat sync [dir]`:** Deterministically mirrors `~/.brain/*.md` into `<dir>/.brain/`, prunes ghost files, ensures `.brain/` is in `.git/info/exclude`, and outputs the exact `brain-sync` marker.
- **`install-brain.bat hash [dir]`:** Outputs the exact `<!-- brain-sync: <12-char-sha256> -->` line.
- **`install-brain.bat remove-legacy`:** Removes the legacy `C:\Users\you\.gemini\config\skills\agent-context-generator` skill folder so it cannot collide with `init-brain`.
