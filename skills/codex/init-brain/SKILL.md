---
name: init-brain
description: Generate, bootstrap, or refresh AGENTS.md by combining repo ground truth with personal rules from .brain/*.md (synced from the master ~/.brain). Use ONLY when the user explicitly asks to run init-brain, bootstrap or refresh project context, or check whether AGENTS.md is stale. Do not use for ordinary coding tasks.
---
# init-brain — Project Context & Brain Compiler

Generate, bootstrap, or refresh `AGENTS.md` by combining **repository ground truth** with matching personal rules from `.brain/*.md`, enforcing a strict 120-line budget and aggressive pruning on every run.

User arguments: any text the user typed when invoking this skill (e.g. `check`, or a short project description such as "Tauri desktop app for X").

## Non-Negotiable Rules
1. **Strict Line Budget (Hard Cap: 120 lines):** Target **40–90 total lines** (including blank lines, headings, and the sync marker). Never exceed **120 lines**. If a draft exceeds 120 lines, compress, prune, or offload lengthy reference material before previewing.
2. **Never Compile Yourself:** When scanning `.brain/*.md` or the master brain, always ignore `init.md`, `init-brain.md`, and `INIT.md` (case-insensitive).
3. **Brain Compilation Modes:** Match every `.brain/*.md` file against the repo using its YAML frontmatter (`apply_when`, `mode`, `trigger`):
   - `mode: inline` — Compile concise, concrete bullets directly into `AGENTS.md` under `## Personal Rules (Brain)`.
   - `mode: reference` — Keep the full playbook in `.brain/<file>.md` and add a single 1-line trigger bullet in `AGENTS.md` under `## Playbooks (Read Before Task)`.
   - **No frontmatter fallback:** Infer relevance from filename/content; treat files `<= 15` lines as `inline` and longer files as `reference`, and flag the assumption in Phase 4.
   - **Stub Guard (inline):** Never copy unfilled `<...>` placeholder bullets into `AGENTS.md`. Skip stub lines and flag them under `Incomplete Brain Files`.
   - **Stub Guard (reference):** If a `reference` file has no filled (non-`<...>`) content bullets after its heading, add **no pointer** to `AGENTS.md` and flag the file under `Incomplete Brain Files`. Never point the agent at an empty playbook.
   - **Refresh sync:** On refresh, always replace previous `Brain-Managed` bullets with current `.brain/*.md` content so updated personal rules propagate automatically.
4. **Master Brain Sync:** The source of truth for personal rules is the master brain: `~/.brain/` (Windows: `%USERPROFILE%\.brain\`). If it exists, stage a sync into the project's `.brain/`: master files overwrite same-named project files, project-only files are kept, nothing is ever deleted. Compile from the **post-sync** content. If no master exists, use the project's `.brain/` as-is and say so in Phase 4.
5. **Stay on `main`:** Never create new branches or git worktrees. On an empty folder with no `.git/`, stage `git init -b main` for Phase 5. If `.git/` exists on another branch, flag it in Phase 4—never silently run `git checkout`.
6. **Aggressive Pruning on Refresh:** Actively delete stale file paths, obsolete commands, resolved one-off workarounds, and generic filler. Never append new rules blindly onto an old file.
7. **Protect Repo-Specific Human Guardrails:** Non-derivable constraints written manually for this repo (outside `.brain/`) are immutable unless proven dead. If one references a deleted file or removed tool, tag it `Human-Stale` and list it under `Confirm to Prune` instead of deleting it silently. If a repo guardrail intentionally overrides a `.brain/` rule, keep the repo override and note it in Phase 4.
8. **Zero Generic Filler:** Ban advice any competent LLM already knows (`write clean code`, `follow SOLID`, `handle edge cases`, `write unit tests`, `use meaningful names`). Every line must encode a **repo-specific fact, command, boundary, trap, or explicit `.brain/` rule**.
9. **Source Truth Only:** Every path, directory, CLI command, and script written into `AGENTS.md` must be verified against the current workspace. Never invent commands, copy default scaffold stubs (e.g., `echo "Error: no test specified"`), or emit `N/A` placeholders.
10. **No Directory Trees or Dependency Dumps:** Name only key entry points and layer boundaries; never mirror `package.json`, `pyproject.toml`, or full file trees.
11. **Windows-Safe Cross-Tool Sync (No Symlinks):** Keep `AGENTS.md` as the single source of truth. Write a 1-line `@AGENTS.md` pointer in `CLAUDE.md` (and `GEMINI.md` only if it already exists or the user requests it). Hide `.brain/` from Git via `.git/info/exclude` rather than modifying `.gitignore`.
12. **Never Commit:** This command never runs `git add`, `git commit`, `git push`, or bumps versions. Committing the result is the user's call, under their own `.brain/` commit rules.

---

## Phase 0: Refresh Check (always runs first, read-only)

1. If `AGENTS.md` does not exist, skip to Phase 1.
2. Compute a **brain fingerprint**: the first 12 hex chars of the SHA-256 of the concatenated contents of all applicable `.brain/*.md` files (post-sync view of the master if it exists), sorted by filename, excluding `init*.md`.
3. Read the last line of `AGENTS.md`, `<!-- brain-sync: <fingerprint> -->`. Compare:
   - **Match** and no manifest/workflow file is newer than `AGENTS.md` → `AGENTS.md is up to date.`
   - **Mismatch** → `Refresh due: brain rules changed.` Name which `.brain/*.md` files differ from the last sync when determinable.
   - **Marker missing** (older or hand-written `AGENTS.md`) → `Refresh due: no sync marker.` Fall back to comparing file modification times.
   - **Repo drift:** if any manifest, `.github/workflows/*.yml`, or version file is newer than `AGENTS.md`, add `Refresh suggested: repo files changed since last sync.`
4. If the user's invocation text is exactly `check`, print only this report and **stop**. Do not continue to Phase 1. Otherwise print the result in one line and continue.

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
   - `desktop-gui` / `windows-app` (Tauri, Electron, PySide/PyQt, WinUI, GloriaSetup configs)
   - `web-ui` (React, Next.js, Vite, HTML/CSS frontend)
   - `cli-backend` (CLI tools, Discord bots, scripts, APIs, libraries)
   - `has-github-remote` (`.github/` exists or `git remote -v` points to GitHub)
3. **Locate & Load Brain Files:** Check for the master brain (`~/.brain/`) and the project `.brain/`. Read all `.md` files from both (post-sync view), excluding `init*.md` / `INIT.md`. Match each against detected traits using `apply_when`. Record unmatched files in a `Skipped` list and note unfilled `<...>` stub lines for Phase 4.
4. **Read Existing Context Files:** Read `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.cursorrules`, and `.github/copilot-instructions.md` (whichever exist). Tag every existing rule as:
   - `Brain-Managed` — Originated from `.brain/` (re-sync fresh from disk).
   - `Verified` — Repo-derived fact still matching current code/files.
   - `Stale` — Repo-derived fact referencing deleted files, renamed scripts, or removed tools.
   - `Human-Active` — Repo-specific manual guardrail (not from `.brain/`) still valid.
   - `Human-Stale` — Repo-specific manual guardrail referencing a missing file/tool (route to `Confirm to Prune`).
   - `Bloat` — Generic LLM advice, marketing prose, or redundant listings.
5. **Build, Test, Lint & Version Truth (skip if `Bootstrap`):** Inspect root and sub-app manifests up to 2 levels deep (`package.json`, `pyproject.toml`, `Cargo.toml`, `src-tauri/Cargo.toml`, `tauri.conf.json`, `go.mod`, `*.csproj`, `Makefile`, `justfile`, `docker-compose.yml`, `.env.example`) plus `.github/workflows/*.yml`. Identify the exact version file(s) so version-bump rules target the right files.
6. **Architecture & Git History (skip if `Bootstrap`):** Map main entry points, core logic folders, IPC/API bridges, subprocess wrappers, and config loaders in 4–8 bullets max. If `.git/` exists, check `git branch --show-current` and run `git log -n 25 --oneline` for recurring `fix:` commits, platform quirks (Windows paths, UTF-8, threading, process cleanup), and recent refactors.

---

### Phase 2: Consolidate, Prune & Compile (Budget Enforcement)
Apply in exact order until the draft fits cleanly inside **40–90 lines** (hard max **120 lines**):

1. **Consolidate:** Merge surviving `Verified` and `Human-Active` rules from `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, and `.cursorrules` into the single `AGENTS.md` draft so manual edits in pointer files are never lost.
2. **Prune:** Delete all `Stale` and `Bloat` items. Move `Human-Stale` items to Phase 4 `Confirm to Prune`.
3. **Compile `.brain/`:**
   - Inject fresh, non-stub bullets from matched `mode: inline` files into `## Personal Rules (Brain)`.
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
- `<core-module-path>` — <Where the IPC, model, state, worker, or version manifest lives.>

## Personal Rules (Brain)
- **<Topic>:** <Rule compiled from a matched mode:inline .brain/*.md file.>

## Playbooks (Read Before Task)
- **<Task>:** Read `.brain/<file>.md` <exact trigger condition>.

## Repo Guardrails & Gotchas
- **<Topic>:** <Repo-specific human guardrail, forbidden pattern, or preserved platform quirk.>

## Done Criteria
<Exact commands and checks to run before reporting complete, plus required version-bump verification steps.>

<!-- brain-sync: <12-char fingerprint from Phase 0> -->
```

The sync marker must be the **last line** of the file.

---

### Phase 4: Preview & Report (read-only)
Print, in this order, then stop and wait:

1. **Summary:** Mode, detected traits, current branch (or "no git repo"), master brain found/not found, and the Phase 0 refresh result.
2. **Draft:** The full `AGENTS.md` in a code block, with its line count (`NN / 120`).
3. **Report** (omit any empty item):
   - `Brain Sync` — Files to copy or overwrite from master → project `.brain/`.
   - `Skipped` — Brain files not matching any detected trait.
   - `Incomplete Brain Files` — Files with stub `<...>` lines, and reference files skipped for having no filled content. Say exactly what to fill in.
   - `Assumptions` — No-frontmatter inferences, guessed traits.
   - `Confirm to Prune` — `Human-Stale` items; each is kept unless the user confirms deletion.
   - `Overrides` — Repo guardrails that intentionally beat a `.brain/` rule.
   - `Branch Warning` — If `.git/` is on a branch other than `main`.
   - `Hidden Brain Warning` — If `has-github-remote` and playbook pointers were added: `.brain/` will be excluded from Git, so those playbooks won't exist in other clones or CI.
   - `Overflow` — Lowest-signal bullets to cut, if still over budget.
4. **Staged Writes:** The exact list Phase 5 will perform (files created/overwritten, `git init -b main` if needed, `.git/info/exclude` entry, `docs/ARCHITECTURE.md` if staged).
5. **Prompt:** End with: `Reply "apply" to write these changes, or tell me what to change.`

If the user requests changes, revise the draft and repeat Phase 4. Do not write anything without an explicit `apply`.

---

### Phase 5: Apply (only after the user replies `apply`)
Perform exactly the Phase 4 staged writes, in this order:

1. **Git:** If there is no `.git/`, run `git init -b main`. If `.git/` exists, change nothing about branches.
2. **Brain sync:** Copy master → project `.brain/` per Rule 4. Create `.brain/` if missing.
3. **Offload:** Write `docs/ARCHITECTURE.md` if one was staged.
4. **`AGENTS.md`:** Write the approved draft, including the `brain-sync` marker as its final line.
5. **Pointers:** Write `CLAUDE.md` as a single line, `@AGENTS.md` (this replaces its prior content, which was consolidated in Phase 2). Do the same for `GEMINI.md` only if it already exists or the user requested it.
6. **Exclude:** Append `.brain/` to `.git/info/exclude` if not already present (create the file if needed). Do not touch `.gitignore`.
7. **Verify:** Confirm `AGENTS.md` is ≤ 120 lines, ends with the sync marker, contains no `<...>` placeholders, and that every `.brain/<file>.md` it references exists. Fix any failure before reporting.
8. **Report:** One short summary: files written, final line count, and any `Incomplete Brain Files` still to fill. Do not commit, push, or bump versions (Rule 12).
