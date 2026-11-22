# Findings — Agent Brain Review (11-22-26)

**Reviewer:** Gemini 4 Argon (High)

---

## What Was Already Strong

- **`inline` vs. `reference` Progressive Disclosure:** Keeping `AGENTS.md` under a hard 120-line budget (target 40–90 lines) while placing deep packaging and CI playbooks in `.brain/<file>.md` behind 1-line triggers solves context bloat and preserves high instruction compliance.
- **Workspace-Local `.brain/` Mirror via `.git/info/exclude`:** Sandboxed coding agents (Codex, Claude Code, Antigravity) often block or prompt before reading outside the repository root (`~/.brain/`). Mirroring playbooks into `<project>/.brain/` keeps them inside the workspace sandbox without polluting `.gitignore`.
- **The `<...>` Stub Guard + Read-Only Preview (`apply` Gate):** Prevents unfinished templates or hallucinated commands from leaking into live project context.

---

## 7 Key Findings & Architectural Gaps

### 1. The "Ghost File" Bug in Master → Project Sync (Rule 4)
In the initial `brain/init.md` specification, Rule 4 stated:
> *"master files overwrite same-named project files, project-only files are kept, nothing is ever deleted. Compile from the post-sync content."*

- **Problem:** If a rule file in `~/.brain/` is ever renamed, split, or deleted (for example, splitting `ui-preferences.md` into `ui-desktop.md` and `ui-web.md`), every repository that previously synced the old filename retains it forever inside `<project>/.brain/`. On subsequent refreshes, the compiler treats that leftover file as a "project-only file" and continues compiling stale or duplicate rules indefinitely.

### 2. Provenance Ambiguity on Refresh (`Brain-Managed` vs. `Human-Active`)
In Phase 1 Step 4, the LLM was asked to inspect an existing `AGENTS.md` and infer which bullets were `Brain-Managed` (originated from `.brain/`) versus `Human-Active` (manual repo-specific guardrails that must be preserved).
- **Problem:** Once `mode: inline` rules are compiled and compressed into `AGENTS.md`, they look identical to manual bullets. Whenever a rule in `~/.brain/` is reworded, an LLM refreshing `AGENTS.md` can easily misclassify the old wording in `AGENTS.md` as a repo-specific `Human-Active` guardrail and keep both the old and new versions.
- **Additional Gap:** Phase 0 Step 2 attempted to hash "all applicable `.brain/*.md` files" *before* Phase 1 Step 2 detected which traits applied to the repository.

### 3. Multi-Turn Version-Bump Spirals in `workflow.md`
In `brain/workflow.md`, the versioning rule stated:
> *"Bump the alpha number by `+1` on every minimal change (e.g., `0.1.0-alpha.1` -> `0.1.0-alpha.2`) across all version manifests in the repo."*

- **Problem 1 (Multi-turn chat loops):** When iterating on a feature or UI tweak across 5–10 chat turns, an agent strictly obeying `## Done Criteria` bumps `alpha.1` → `alpha.2` → ... → `alpha.10` on every turn, dirtying lockfiles and triggering unnecessary rebuilds before a single commit is made.
- **Problem 2 (CI release trigger coupling):** Across `projectGloria` release workflows, changing the manifest version on `main` is the exact trigger that launches the GitHub Actions installer build and release pipeline.
- **Problem 3 (Missing version manifest list in template):** Although Phase 1 Step 5 asked the agent to identify all version files, the Phase 3 `AGENTS.md` template had no explicit line listing the repository's version manifests (`Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`, `gloria.toml`).

### 4. Mixing Mechanical Work (Hashing & File Sync) with Semantic LLM Work
`brain/init.md` relied on the LLM to compute a 12-character SHA-256 hex digest (`<!-- brain-sync: ... -->`), copy files from `~/.brain/` to `<project>/.brain/`, update `.git/info/exclude`, verify line counts, and keep four separate copies of the prompt (`brain/init.md` + three `SKILL.md` files) manually synchronized.
- **Problem:** LLMs cannot reliably compute SHA-256 hashes without spawning shell commands (or worse, hallucinating a 12-char hex string), and invoking an LLM just to run `/init-brain check` wastes tokens and latency on a purely deterministic check. Meanwhile, manually editing four prompt files invites drift.

### 5. Over-Broad Trait Scoping in `ui-preferences.md` & Missing Project Traits
`brain/ui-preferences.md` used `apply_when: desktop-gui, web-ui`, which acts as a logical OR for the entire file.
- **Problem:** Pure web projects (`web-ui`) matched `ui-preferences.md` and were instructed to disable native OS titlebars, build custom min/max/close window controls, and lock the viewport to `100vw x 100vh` (`overflow: hidden`).
- **Missing Traits:** The trait list did not include `browser-extension` or `android-app`, both of which exist across `projectGloria` repositories.

### 6. No Reverse-Flow Learning Mechanism (`learn`)
Information only flowed top-down (`~/.brain/*.md` → `AGENTS.md`).
- **Problem:** When correcting an agent during a coding session (e.g., *"never quote `DisplayIcon` in HKCU Uninstall"* or *"always use `C:\Users\you` in docs"*), there was no built-in command to persist that lesson either as a repo-specific guardrail in `AGENTS.md` or as a global rule in `~/.brain/`.

### 7. Incomplete Placeholder Brain Files, Stub-Detection Edge Case, & Skill Collision
- **Three Unfilled Brain Files:** `brain/commit-and-push.md`, `brain/building-the-project.md`, and `brain/github-actions.md` still contained `<...>` placeholders and were being skipped by the stub guard.
- **Stub-Guard False Positives:** A naive `<...>` check can misclassify valid filled rules that contain backticked CLI metavariables (such as `` `dist/<Name>_<version>_x64-setup.exe` ``) or arrows (`->`) as unfilled stubs.
- **Antigravity Skill Collision:** `C:\Users\you\.gemini\config\skills\agent-context-generator` had an overlapping trigger description with `init-brain` (*"generate, init, update, or refresh agent context, AGENTS.md, or CLAUDE.md"*), which can cause the agent to invoke the older skill instead of `init-brain`.
