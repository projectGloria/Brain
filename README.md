# agent-brain

Personal rules for AI coding agents, kept in one place and compiled into a short `AGENTS.md` for every project.

You write your preferences once (always work on `main`, how to version, UI style, how to package with GloriaSetup, how to release). A skill called `init-brain` reads those rules, scans the repo, and writes a verified, size-capped `AGENTS.md` that Claude Code, Codex, and Antigravity all load automatically.

## How it works

```
~/.brain/            master rules (you edit these, or run install-brain.bat)
    │  /init-brain
    ▼
<project>/.brain/    managed mirror of master rules (untracked via .git/info/exclude)
<project>/AGENTS.md  compiled: repo facts + your rules (max 120 lines)
<project>/CLAUDE.md  one line: @AGENTS.md
```

- **`inline` rules** are compiled straight into `AGENTS.md` under `## Personal Rules (Brain)`.
- **`reference` rules** stay in `.brain/<file>.md`, and `AGENTS.md` gets a one-line trigger pointer under `## Playbooks (Read Before Task)` saying when to read them.
- **Section ownership is deterministic:** on refresh, `## Personal Rules (Brain)` and `## Playbooks (Read Before Task)` are regenerated from scratch from `.brain/*.md`, while `## Repo Guardrails & Gotchas` preserves manual repo-specific rules (and overrides any `.brain/` rule with the same `**<Topic>:**` key).
- Nothing is written until you reply `apply`.

## Repo layout

```
install-brain.bat               one-command installer, skill compiler, and SHA-256 sync checker
brain/                          master rules + the tool-agnostic prompt
  workflow.md                   branching on main + working-tree alpha versioning
  ui-preferences.md             frameless desktop chrome, themed scrollbars, interactive states
  commit-and-push.md            single-sentence commit format, push policy, path privacy
  building-the-project.md       GloriaSetup packaging & dry-run verification playbook
  github-actions.md             3-stage CI release pipeline (version -> build -> publish)
  init.md                       canonical source of the init-brain skill prompt
skills/
  claude-code/init-brain/SKILL.md
  codex/init-brain/SKILL.md
  antigravity/init-brain/SKILL.md
```

## Install & CLI helper (`install-brain.bat`)

Run `install-brain.bat` from the repo root (or double-click it in Explorer). In one step it:
1. Compiles `brain/init.md` into all three tool `SKILL.md` files under `skills/` so they never drift out of sync.
2. Syncs `brain/*.md` (and `install-brain.bat`) into `%USERPROFILE%\.brain\`, pruning any deleted ghost files.
3. Installs the `init-brain` skill into Claude Code, Codex, and Antigravity:
   - Claude Code: `C:\Users\you\.claude\skills\init-brain\SKILL.md`
   - Codex: `C:\Users\you\.agents\skills\init-brain\SKILL.md`
   - Antigravity: `C:\Users\you\.gemini\config\skills\init-brain\SKILL.md`
4. Computes and verifies the master `<!-- brain-sync: <12-char-sha256> -->` fingerprint.

### `install-brain.bat` commands

| Command | What it does |
|---|---|
| `.\install-brain.bat` | Syncs the 3 `SKILL.md` files from `brain\init.md`, installs `~/.brain/`, and installs all 3 tool skills |
| `.\install-brain.bat check [dir]` | Deterministically checks whether a repo's `AGENTS.md` matches the master brain fingerprint and line budget |
| `.\install-brain.bat check-all [dir]` | Scans all sibling repositories (e.g. `projectGloria`) and prints a table of `AGENTS.md` sync status |
| `.\install-brain.bat sync [dir]` | Syncs `~/.brain/*.md` into `<dir>/.brain/`, prunes ghost files, and ensures `.brain/` is in `.git/info/exclude` |
| `.\install-brain.bat hash [dir]` | Prints the exact `<!-- brain-sync: <12-char-sha256> -->` marker |
| `.\install-brain.bat remove-legacy` | Removes the old `~/.gemini/config/skills/agent-context-generator` skill so it doesn't collide with `init-brain` |

## Agent Usage

| You type | What happens |
|---|---|
| `/init-brain` | Detects the mode, previews `AGENTS.md` plus a report, waits for `apply` |
| `/init-brain check` | Read-only: reports whether `AGENTS.md` is up to date, then stops |
| `/init-brain learn <rule>` | Persists a new repo guardrail into `AGENTS.md` or a global rule into `~/.brain/*.md`, previews, waits for `apply` |
| `/init-brain Tauri desktop app for X` | For an empty folder: gives the agent the stack so it doesn't have to ask |

Modes are chosen automatically:

- **Bootstrap:** empty folder. Writes a short `AGENTS.md` with your rules only, and stages `git init -b main`.
- **Initial generation:** existing code with no `AGENTS.md`.
- **Refresh:** existing `AGENTS.md`. Prunes stale entries, regenerates your `.brain/` sections, and preserves your manual repo guardrails.
- **Learn:** adds a single repo-specific or global rule without a full manual edit cycle.

**When to run it:** when you start a project, when you change a rule in `brain/` (after running `install-brain.bat`), or after a big refactor. In Codex and Antigravity, ask the agent to run `init-brain` (or pick it from the skills list).

## Brain file format

Every file in `brain/` starts with YAML frontmatter:

```yaml
---
apply_when: desktop-gui, windows-app   # when this file matters
mode: inline                           # or: reference
trigger: Before building an installer  # reference files only
---
```

Recognized `apply_when` traits:
`always`, `desktop-gui`, `windows-app`, `web-ui`, `browser-extension`, `android-app`, `cli-backend`, `has-github-remote`.

### Per-bullet trait filters
Inside a `mode: inline` file, individual bullets can restrict themselves to specific traits using a `[trait]` prefix:

```markdown
- [desktop-gui] **Window Chrome:** Disable native OS titlebars...
- **Scrollbars:** Ban raw OS scrollbars; use thin (4–6px) themed scrollbars...
```

When compiled into a `web-ui` or `browser-extension` repo, `[desktop-gui]` bullets are automatically skipped; when compiled into a `desktop-gui` repo, the `[desktop-gui]` tag is stripped and the rule is included.

### Project-only playbooks (`local-*.md`)
On sync, `<project>/.brain/` mirrors `~/.brain/` and deletes stale files that were removed from `~/.brain/`. If a project needs an untracked local playbook that doesn't exist in `~/.brain/`, name it `.brain/local-<name>.md` so sync preserves it (or keep tracked architecture docs in `docs/`).

## Guarantees

- `AGENTS.md` is capped at 120 lines (target 40–90) and ends with a deterministic `<!-- brain-sync: <12-char-sha256> -->` marker.
- Every command, path, and version manifest in `AGENTS.md` is verified against the workspace. Nothing is invented.
- Stays on `main`: never creates branches or worktrees, never runs `git checkout`.
- Never runs `git add`, `commit`, or `push`, and never bumps versions during context generation.
- `.brain/` is hidden from Git through `.git/info/exclude`, not `.gitignore`.
- No symlinks, so it works natively on Windows without elevation.
