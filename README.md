# agent-brain

Personal rules for AI coding agents, kept in one place and compiled into a short `AGENTS.md` for every project.

You write your preferences once (always work on `main`, how to version, UI style, how to build). A skill called `init-brain` reads those rules, scans the repo, and writes a verified, size-capped `AGENTS.md` that Claude Code, Codex, and Antigravity all load automatically.

## How it works

```
~/.brain/            master rules (you edit these)
    │  /init-brain
    ▼
<project>/.brain/    synced copy of the master
<project>/AGENTS.md  compiled: repo facts + your rules (max 120 lines)
<project>/CLAUDE.md  one line: @AGENTS.md
```

- **`inline` rules** are copied straight into `AGENTS.md`.
- **`reference` rules** stay in `.brain/<file>.md`, and `AGENTS.md` gets a one-line pointer saying when to read them.
- Nothing is written until you reply `apply`.

## Repo layout

```
brain/                        master rules + the tool-agnostic prompt
  workflow.md                 branching + versioning
  ui-preferences.md           window chrome, scrollbars, native feel
  commit-and-push.md          commit format + push policy   (TODO: fill in)
  building-the-project.md     GloriaSetup build rules       (TODO: fill in)
  github-actions.md           CI + release rules            (TODO: fill in)
  init.md                     plain-markdown copy of the skill
skills/
  claude-code/init-brain/SKILL.md
  codex/init-brain/SKILL.md
  antigravity/init-brain/SKILL.md
```

## Install

### 1. Master brain

Copy the contents of `brain/` into `~/.brain/` (Windows: `C:\Users\<you>\.brain\`). The `.md` files go directly inside it, with no extra nested folder.

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.brain" | Out-Null
Copy-Item .\brain\*.md "$env:USERPROFILE\.brain\"
```

### 2. The skill (one per tool you use)

| Tool | Copy `SKILL.md` into |
|---|---|
| Claude Code | `~/.claude/skills/init-brain/` |
| Codex | `~/.agents/skills/init-brain/` |
| Antigravity | `~/.gemini/config/skills/init-brain/` |

The folder must be named `init-brain` and the file must be named `SKILL.md`. Restart the tool or open a new chat afterward.

```powershell
# Claude Code example
New-Item -ItemType Directory -Force "$env:USERPROFILE\.claude\skills\init-brain" | Out-Null
Copy-Item .\skills\claude-code\init-brain\SKILL.md "$env:USERPROFILE\.claude\skills\init-brain\"
```

If Antigravity doesn't pick the skill up, try `~/.gemini/antigravity/skills/init-brain/`, the path older docs used.

## Usage

| You type | What happens |
|---|---|
| `/init-brain` | Detects the mode, previews `AGENTS.md` plus a report, waits for `apply` |
| `/init-brain check` | Read-only: reports whether `AGENTS.md` is up to date, then stops |
| `/init-brain Tauri desktop app for X` | For an empty folder: gives the agent the stack so it doesn't have to ask |

Modes are chosen automatically:

- **Bootstrap:** empty folder. Writes a short `AGENTS.md` with your rules only, and stages `git init -b main`.
- **Initial generation:** existing code with no `AGENTS.md`.
- **Refresh:** existing `AGENTS.md`. Prunes stale entries, re-syncs your rules, and keeps your manual guardrails.

**When to run it:** when you start a project, when you change a rule in `~/.brain/`, or after a big refactor. You don't need it every session, because the agent loads `AGENTS.md` by itself. Use `check` to see if a refresh is due.

In Codex and Antigravity, ask the agent to run `init-brain` (or pick it from the skills list) instead of typing a slash command.

## Brain file format

Every file in `brain/` starts with YAML frontmatter:

```yaml
---
apply_when: desktop-gui, windows-app   # when this file matters
mode: inline                           # or: reference
trigger: Before building an installer  # reference files only
---
```

`apply_when` traits: `always`, `desktop-gui`, `windows-app`, `web-ui`, `cli-backend`, `has-github-remote`. The skill detects these from the repo.

Placeholders like `<Your exact commit format>` are never copied into `AGENTS.md`. They are skipped and listed under **Incomplete Brain Files**. A `reference` file with no filled-in content gets no pointer at all.

## Guarantees

- `AGENTS.md` is capped at 120 lines (target 40-90) and ends with a `brain-sync` marker used by `check`.
- Every command and path in it is verified against the workspace. Nothing is invented.
- Stays on `main`: never creates branches or worktrees, never runs `git checkout`.
- Never runs `git add`, `commit`, or `push`, and never bumps versions.
- `.brain/` is hidden from Git through `.git/info/exclude`, not `.gitignore`.
- No symlinks, so it works on Windows.

## Known gaps

- `commit-and-push.md`, `building-the-project.md`, and `github-actions.md` still contain placeholders. Fill them in before relying on them.
- "Bump the alpha number on every minimal change" in `workflow.md` doesn't define what counts as one change (per commit, per task, per turn).
- The `100vw x 100vh` shell lock in `ui-preferences.md` suits desktop apps but would break an ordinary website. Consider scoping it to `desktop-gui`.
- Because `.brain/` is excluded from Git, `reference` playbooks won't exist in other clones or on CI. The preview warns you about this when the repo has a GitHub remote.
- The Codex and Antigravity skills have not been tested end to end. Their install paths come from each tool's docs, which change.

## Keeping the three skills in sync

The three `SKILL.md` files share the same body. The Claude Code one differs only in its frontmatter (`argument-hint`, `disable-model-invocation: true`) and in using `$ARGUMENTS`. When you change a rule or phase, edit one file and copy the change into the other two.
