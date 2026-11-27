# INIT.md — Project Context (tool-agnostic)

> Plain-markdown copy of the `gloria-init-brain` skill. Brain does the deterministic part (`brain sync`); you write only the repo knowledge.

Create or refresh `AGENTS.md` for the current repo. User arguments: `check`, `learn <rule>`, or a short project description.

`brain` below means `"%USERPROFILE%\.brain\brain.cmd"` (installed by Brain).

## Non-Negotiable Rules
1. **Brain owns three things; never hand-write them:** `## Personal Rules (Brain)`, `## Playbooks (Read Before Task)`, and the final `<!-- brain-sync: ... -->` line. `brain sync` compiles them from `~/.brain/*.md` (traits, `apply_when`, stubs, topic overrides), mirrors `.brain/`, writes `CLAUDE.md` = `@AGENTS.md`, and hides `.brain/` via `.git/info/exclude`.
2. **You own the rest:** the one-line description, `## Commands`, `## Architecture`, `## Repo Guardrails & Gotchas`, `## Done Criteria`. Preserve valid human guardrails; a guardrail with the same `**Topic:**` as a Brain rule overrides it.
3. **Budget:** target about 60 lines, hard cap 120 (Brain's sections included).
4. **Source truth only:** every path, command and script must exist in the repo now. No generic advice, no `N/A`, no directory trees or dependency dumps, no scaffold stubs.
5. **Stay on `main`:** never create branches or worktrees; on a folder without `.git/`, `git init -b main` at apply time.
6. **Never commit, push, tag, or bump versions.**

## Steps
1. **Check:** run `brain check .` and print its line. For `check`, stop here.
   - `BRAIN DRIFT` alone: run `brain sync`, show the result, and stop. No AI work needed.
   - `REPO DRIFT`, `NO MARKER`, `NO AGENTS.MD`, `OVER BUDGET`: continue.
2. **Learn (`learn <rule>`):** decide global (`~/.brain/<file>.md` and the Brain repo's `brain/<file>.md`) or repo-specific (`## Repo Guardrails & Gotchas`). Ask one question if unclear. Stage the edit, then go to step 5.
3. **Inspect (read-only):**
   - Manifests up to 2 levels deep (`package.json`, `Cargo.toml`, `src-tauri/tauri.conf.json`, `gloria.toml`, `pyproject.toml`, `*.csproj`, `go.mod`, `Makefile`) and `.github/workflows/*.yml`: exact run/test/lint/build commands and every version file.
   - 4–8 key entry points and layer boundaries.
   - `git branch --show-current` and `git log -n 25 --oneline` for recurring traps (Windows paths, UTF-8, threading, process cleanup).
   - Existing `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.cursorrules`, `.github/copilot-instructions.md`: keep verified facts and valid guardrails, mark guardrails that point at deleted files as stale.
   - Empty folder (bootstrap): if the purpose, stack and target are unknown, ask at most 3 short questions first.
4. **Draft** (omit empty sections; leave the Brain sections as they are or absent, `brain sync` fills them):

```markdown
# AGENTS.md — <Project Name>

<One line: what it does, stack, target OS/type.>

## Commands
- Run / Dev: `<exact command>`
- Test: `<exact command>` (single test: `<exact command>`)
- Lint / Typecheck: `<exact command>`
- Build / Package: `<exact command>`

## Architecture
- `<entry-point>` — <role>
- `<layer/>` — <what lives here and what must not>
- **Version Manifests:** `<file>`, `<file>` (always the same version).

## Repo Guardrails & Gotchas
- **<Topic>:** <repo-specific trap or rule>

## Done Criteria
- <exact checks before reporting done, including that all version manifests agree>
```

5. **Preview:** print the mode, the draft with its estimated line count, stale guardrails to confirm, and the staged writes. End with: `Reply "apply" to write these changes, or tell me what to change.` Write nothing before `apply`.
6. **Apply:** `git init -b main` if needed, write `AGENTS.md`, then run `brain sync --accept-repo` and print its output. If it reports warnings (stub rules, over budget), fix what is yours and run it again. Report files written and the final line count.
