---
name: monjita-init-brain
description: Generate, bootstrap, refresh, or update AGENTS.md by combining repo ground truth with personal rules from .brain/*.md (synced from the master ~/.brain). Use ONLY when the user explicitly asks to run monjita-init-brain, init-brain, bootstrap or refresh project context, learn/persist a rule, or check whether AGENTS.md is stale. Do not use for ordinary coding tasks.
---
# monjita-init-brain — Project Context & Brain Compiler

Generate, bootstrap, refresh, or update `AGENTS.md` by combining **repository ground truth** with matching personal rules from `.brain/*.md`, enforcing a strict 120-line budget and aggressive pruning on every run.

User arguments: any text the user typed when invoking this skill (e.g. `check`, `learn <rule>`, or a short project description such as "Tauri desktop app for X").
## Non-Negotiable Rules
1. **Strict Line Budget (Hard Cap: 120 lines):** Target **40–90 total lines** (including blank lines, headings, and the sync marker). Never exceed **120 lines**. If a draft exceeds 120 lines, compress, prune, or offload lengthy reference material before previewing.
2. **Never Compile Yourself:** When scanning `.brain/*.md` or the master brain, always ignore `init.md`, `init-brain.md`, and `INIT.md` (case-insensitive).
3. **Brain Compilation Modes & Per-Bullet Traits:** Match every `.brain/*.md` file against the repo using its YAML frontmatter (`apply_when`, `mode`, `trigger`):
   - `mode: inline` — Compile concise, concrete bullets directly into `AGENTS.md` under `## Personal Rules (Brain)`.
   - `mode: reference` — Keep the full playbook in `.brain/<file>.md` and add a single 1-line trigger bullet in `AGENTS.md` under `## Playbooks (Read Before Task)`.
   - **Per-Bullet Trait Tags (`[trait1, trait2]`):** Inside `mode: inline` files, a bullet may start with `- [trait1, trait2] **<Topic>:** ...`. Include that bullet **only** if the repo matches at least one listed trait, and strip the `[...]` tag when compiling into `AGENTS.md`. Bullets without a `[...]` prefix inherit the file's top-level `apply_when`.
   - **No frontmatter fallback:** Infer relevance from filename/content; treat files `<= 15` lines as `inline` and longer files as `reference`, and flag the assumption in Phase 4.
   - **Stub Guard (inline):** Never copy unfilled `<...>` template bullets (where the bullet or its value after `**Topic:**` is an un-backticked `<...>` placeholder) into `AGENTS.md`. Do not mistake backticked CLI metavariables (e.g., `` `dist/<Name>_<version>_x64-setup.exe` ``) or arrows (`->`) for stubs. Skip stub lines and flag them under `Incomplete Brain Files`.
   - **Stub Guard (reference):** If a `reference` file has no filled (non-stub) content bullets after its heading, add **no pointer** to `AGENTS.md` and flag the file under `Incomplete Brain Files`. Never point the agent at an empty playbook.
4. **Master Brain Sync (No Ghost Files):** The source of truth for personal rules is the master brain: `~/.brain/` (Windows: `%USERPROFILE%\.brain\`). If it exists, stage a managed mirror sync into `<project>/.brain/`: master `.md` files (excluding `init*.md`) overwrite same-named project files, and **any non-`local-*.md` file in `<project>/.brain/` that no longer exists in `~/.brain/` is deleted** so renamed or removed master files never linger as ghost rules. Project-only playbooks must be named `local-*.md` (or live in `docs/`) to be preserved. Compile from the **post-sync** content. If no master exists, use `<project>/.brain/` as-is and note it in Phase 4.
5. **Stay on `main`:** Never create new branches or git worktrees. On an empty folder with no `.git/`, stage `git init -b main` for Phase 5. If `.git/` exists on another branch, flag it in Phase 4—never silently run `git checkout`.
6. **Deterministic Section Ownership & Pruning on Refresh:**
   - `## Personal Rules (Brain)` and `## Playbooks (Read Before Task)` are **100% owned by `.brain/`** and regenerated from scratch on every refresh—never carry over old bullets from these two sections.
   - `## Repo Guardrails & Gotchas` is **100% repo-specific (`Human-Active`)** and preserved across refreshes. If one references a deleted file or removed tool, tag it `Human-Stale` and list it under `Confirm to Prune` instead of deleting it silently.
   - **Topic Override Rule:** If a bullet in `## Repo Guardrails & Gotchas` shares the exact same `**<Topic>:**` prefix as a `.brain/` rule, the repo guardrail wins—omit the conflicting `.brain/` bullet from `## Personal Rules (Brain)` and report it under `Overrides` in Phase 4.
   - Actively delete stale file paths, obsolete commands, resolved one-off workarounds, and generic filler across all repo-derived sections.
7. **Zero Generic Filler:** Ban advice any competent LLM already knows (`write clean code`, `follow SOLID`, `handle edge cases`, `write unit tests`, `use meaningful names`). Every line must encode a **repo-specific fact, command, boundary, trap, or explicit `.brain/` rule**.
8. **Source Truth Only:** Every path, directory, CLI command, and script written into `AGENTS.md` must be verified against the current workspace. Never invent commands, copy default scaffold stubs (e.g., `echo "Error: no test specified"`), or emit `N/A` placeholders.
9. **No Directory Trees or Dependency Dumps:** Name only key entry points, layer boundaries, and exact version manifests; never mirror `package.json`, `pyproject.toml`, or full file trees.
10. **Windows-Safe Cross-Tool Sync (No Symlinks):** Keep `AGENTS.md` as the single source of truth. Write a 1-line `@AGENTS.md` pointer in `CLAUDE.md` (and `GEMINI.md` only if it already exists or the user requests it). Hide `.brain/` from Git via `.git/info/exclude` rather than modifying `.gitignore`.
11. **Never Commit:** This command never runs `git add`, `git commit`, `git push`, or bumps versions. Committing the result is the user's call, under their own `.brain/` commit rules.

---

## Phase 0: Refresh Check & Learn Routing (always runs first, read-only)

1. **Learn Mode Routing (`learn <rule>`):** If the user's invocation starts with `learn ` (or explicitly asks to remember/persist a new rule):
   - Determine whether the rule is **repo-specific** (belongs in `<project>/AGENTS.md` under `## Repo Guardrails & Gotchas`) or **global** (belongs in `~/.brain/<file>.md`). If ambiguous, ask 1 short question and wait.
   - Stage the updated `~/.brain/<file>.md` (if global), `<project>/.brain/<file>.md`, and `<project>/AGENTS.md` (enforcing the 120-line budget and updated `brain-sync` marker), then jump directly to **Phase 4 (Preview & Report)**.
2. If `AGENTS.md` does not exist, skip to Phase 1.
3. Compute the **brain fingerprint**: the first 12 lowercase hex chars of the SHA-256 of the concatenated LF-normalized (`\n`) contents of all `.brain/*.md` files (post-sync view of `~/.brain/` plus any `local-*.md`), sorted by lowercase filename, excluding `init*.md`, formatted as `<filename>\n<content>\n`. (If `%USERPROFILE%\.brain\install-brain.bat` exists, you may run `"%USERPROFILE%\.brain\install-brain.bat" check .` to compute and verify this deterministically.)
4. Read the last line of `AGENTS.md`, `<!-- brain-sync: <fingerprint> -->`. Compare:
   - **Match** and no manifest/workflow file is newer than `AGENTS.md` → `AGENTS.md is up to date.`
   - **Mismatch** → `Refresh due: brain rules changed.` Name which `.brain/*.md` files differ from the last sync when determinable.
   - **Marker missing** (older or hand-written `AGENTS.md`) → `Refresh due: no sync marker.` Fall back to comparing file modification times.
   - **Repo drift:** if any manifest, `.github/workflows/*.yml`, or version file is newer than `AGENTS.md`, add `Refresh suggested: repo files changed since last sync.`
5. If the user's invocation text is exactly `check`, print only this report and **stop**. Do not continue to Phase 1. Otherwise print the result in one line and continue.

---

## Workflow

Execute in strict order from the repository root. **Phases 0–4 are strictly read-only**: do not write files, run `git init`, create branches, or run mutating commands until the user replies `apply`.

### Phase 1: Inspect Workspace & `.brain/`
1. **Determine Mode:**
   - `Bootstrap` — Empty/new project folder (no source code or manifests yet). If neither the invocation text nor the conversation states the app's purpose, stack, or target type (desktop GUI vs web vs CLI), ask at most 3 short bullet questions and wait before drafting.
   - `Initial generation` — Existing codebase with no `AGENTS.md`.
   - `Refresh` — Existing codebase with an existing `AGENTS.md`.
2. **Classify Repo Traits:** Detect active traits from disk and/or the user's kickoff prompt:
   - `always` (every repo)
   - `desktop-gui` (Tauri, Electron, Win32/Direct2D, PySide/PyQt, WinUI)
   - `windows-app` (Windows desktop/CLI apps, `gloria.toml`, `.iss`, `windows` crate / Win32 APIs)
   - `web-ui` (React, Next.js, Vite, HTML/CSS frontend)
   - `browser-extension` (Chromium/Firefox extensions with `manifest.json` MV2/MV3)
   - `android-app` (Android/Gradle/Kotlin/Java apps)
   - `cli-backend` (CLI tools, Discord bots, scripts, APIs, libraries)
   - `has-github-remote` (`.github/` exists or `git remote -v` points to GitHub)
3. **Locate & Load Brain Files:** Check for the master brain (`~/.brain/`) and the project `.brain/`. Read all `.md` files from the post-sync view (Rule 4), excluding `init*.md` / `INIT.md`. Match each file against detected traits using `apply_when`, and filter per-bullet `[trait]` tags inside `mode: inline` files. Record unmatched files in `Skipped`, stale non-`local-*.md` project files in `Pruned Ghost Brain Files`, and unfilled `<...>` stub lines for Phase 4.
4. **Read Existing Context Files:** Read `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.cursorrules`, and `.github/copilot-instructions.md` (whichever exist). Classify every existing line by section and validity:
   - `Brain-Managed` — Anything under `## Personal Rules (Brain)` or `## Playbooks (Read Before Task)` (discard and regenerate fresh from `.brain/`).
   - `Verified` — Repo-derived command or architecture fact still matching current code/files.
   - `Stale` — Repo-derived fact referencing deleted files, renamed scripts, or removed tools.
   - `Human-Active` — Manual guardrail under `## Repo Guardrails & Gotchas` (or legacy `## Hard Rules & Gotchas` / pointer files) still valid.
   - `Human-Stale` — Manual guardrail referencing a missing file/tool (route to `Confirm to Prune`).
   - `Bloat` — Generic LLM advice, marketing prose, or redundant listings.
5. **Build, Test, Lint & Version Truth (skip if `Bootstrap`):** Inspect root and sub-app manifests up to 2 levels deep (`package.json`, `pyproject.toml`, `Cargo.toml`, `src-tauri/Cargo.toml`, `tauri.conf.json`, `gloria.toml`, `go.mod`, `*.csproj`, `Makefile`, `justfile`, `docker-compose.yml`, `.env.example`) plus `.github/workflows/*.yml`. Identify the exact version file(s) so version-bump rules target every version manifest in lockstep.
6. **Architecture & Git History (skip if `Bootstrap`):** Map main entry points, core logic folders, IPC/API bridges, subprocess wrappers, and config loaders in 4–8 bullets max. If `.git/` exists, check `git branch --show-current` and run `git log -n 25 --oneline` for recurring fixes, platform quirks (Windows paths, UTF-8, threading, process cleanup), and recent refactors.

---

### Phase 2: Consolidate, Prune & Compile (Budget Enforcement)
Apply in exact order until the draft fits cleanly inside **40–90 lines** (hard max **120 lines**):

1. **Consolidate:** Merge surviving `Verified` and `Human-Active` rules from `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, and `.cursorrules` into the single `AGENTS.md` draft so manual edits in pointer files are never lost.
2. **Prune:** Delete all `Brain-Managed` (to be regenerated), `Stale`, and `Bloat` items. Move `Human-Stale` items to Phase 4 `Confirm to Prune`.
3. **Compile `.brain/`:**
   - Inject fresh, non-stub, trait-matched bullets from matched `mode: inline` files into `## Personal Rules (Brain)` (stripping any `[trait]` prefix and omitting any bullet overridden by a same-`<Topic>:` rule in `## Repo Guardrails & Gotchas`).
   - Add 1-line trigger pointers for matched, **non-empty** `mode: reference` files into `## Playbooks (Read Before Task)`.
4. **Compress:** Deduplicate against manifests; flatten multi-sentence prose into single-line `**<Topic>:** <concrete constraint>` bullets.
5. **Offload Deep Repo Reference (If still > 120 lines):** Move lengthy repo-specific reference tables (exhaustive IPC schemas, protocol specs, full command matrices) to `docs/ARCHITECTURE.md`, stage it for Phase 4/5, and leave a 1-line pointer in `AGENTS.md`.
6. **Escalate Overflow:** If the draft still exceeds 120 lines after steps 1–5, **never exceed 120**—list the lowest-signal bullets in Phase 4 and ask the user which ones to cut or demote to `mode: reference`.

---

### Phase 3: Draft `AGENTS.md`
Include only sections and sub-bullets with verified signal (omit empty sections; never write `N/A`). In `Bootstrap` mode, omit `Commands`, `Architecture`, and any other section that cannot yet be verified.

```markdown
# AGENTS.md — <Project Name>

<One line: what the project does, its runtime/stack, and its target OS/type.>

## Commands
- Run / Dev: `<exact command>`
- Test (all): `<exact command>`
- Test (single): `<exact single-test command>`
- Lint / Typecheck: `<exact command>`
- Build / Package: `<exact command>`

## Architecture
- `<entry-point-path>` — <Startup role and core execution path.>
- `<layer-or-dir/>` — <What responsibility lives here and what must NOT live here.>
- `<core-module-path>` — <Where the IPC, model, state, or worker logic lives.>
- **Version Manifests:** `<path1>`, `<path2>` (keep versions synced across all listed files).

## Personal Rules (Brain)
- **<Topic>:** <Rule compiled from a matched mode:inline .brain/*.md file.>

## Playbooks (Read Before Task)
- **<Task>:** Read `.brain/<file>.md` <exact trigger condition>.

## Repo Guardrails & Gotchas
- **<Topic>:** <Repo-specific human guardrail, forbidden pattern, or preserved platform quirk.>

## Done Criteria
<Exact commands and checks to run before reporting complete, plus required version-bump verification across all Version Manifests.>

<!-- brain-sync: <12-char fingerprint from Phase 0> -->
```

The sync marker must be the **last line** of the file.

---

### Phase 4: Preview & Report (read-only)
Print, in this order, then stop and wait:

1. **Summary:** Mode (`Bootstrap`, `Initial generation`, `Refresh`, or `Learn`), detected traits, current branch (or "no git repo"), master brain found/not found, and the Phase 0 refresh result.
2. **Draft:** The full `AGENTS.md` in a code block, with its line count (`NN / 120`).
3. **Report** (omit any empty item):
   - `Brain Sync` — Files to copy/overwrite from master → project `.brain/`, plus any stale non-`local-*.md` ghost files to delete from project `.brain/`.
   - `Skipped` — Brain files (or per-bullet `[trait]` rules) not matching any detected trait.
   - `Incomplete Brain Files` — Files with stub `<...>` lines, and reference files skipped for having no filled content. Say exactly what to fill in.
   - `Assumptions` — No-frontmatter inferences, guessed traits.
   - `Confirm to Prune` — `Human-Stale` items; each is kept unless the user confirms deletion.
   - `Overrides` — Repo guardrails in `## Repo Guardrails & Gotchas` that overrode a same-`<Topic>:` `.brain/` rule.
   - `Branch Warning` — If `.git/` is on a branch other than `main`.
   - `Hidden Brain Warning` — If `has-github-remote` and playbook pointers were added: `.brain/` will be excluded from Git, so those playbooks won't exist in other clones or CI.
   - `Overflow` — Lowest-signal bullets to cut, if still over budget.
4. **Staged Writes:** The exact list Phase 5 will perform (files created/overwritten/deleted, `git init -b main` if needed, `.git/info/exclude` entry, `docs/ARCHITECTURE.md` if staged).
5. **Prompt:** End with: `Reply "apply" to write these changes, or tell me what to change.`

If the user requests changes, revise the draft and repeat Phase 4. Do not write anything without an explicit `apply`.

---

### Phase 5: Apply (only after the user replies `apply`)
Perform exactly the Phase 4 staged writes, in this order:

1. **Git:** If there is no `.git/`, run `git init -b main`. If `.git/` exists, change nothing about branches.
2. **Brain sync:** Copy master → project `.brain/` and delete stale non-`local-*.md` files in project `.brain/` per Rule 4 (or run `"%USERPROFILE%\.brain\install-brain.bat" sync .` if available). Create `.brain/` if missing.
3. **Offload:** Write `docs/ARCHITECTURE.md` if one was staged.
4. **`AGENTS.md`:** Write the approved draft, including the `brain-sync` marker as its final line.
5. **Pointers:** Write `CLAUDE.md` as a single line, `@AGENTS.md` (this replaces its prior content, which was consolidated in Phase 2). Do the same for `GEMINI.md` only if it already exists or the user requested it.
6. **Exclude:** Append `.brain/` to `.git/info/exclude` if not already present (create the file if needed). Do not touch `.gitignore`.
7. **Verify:** Confirm `AGENTS.md` is ≤ 120 lines, ends with the sync marker, contains no `<...>` placeholders, and that every `.brain/<file>.md` it references exists. Fix any failure before reporting.
8. **Report:** One short summary: files written, final line count, and any `Incomplete Brain Files` still to fill. Do not commit, push, or bump versions (Rule 11).
