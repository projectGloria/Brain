---
apply_when: always
mode: inline
---
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until explicitly told otherwise.
- **Versioning:** Never bump versions during normal work. The user picks the version at release time and `/gloria-github-release` sets it in every version file at once; keep version files in sync with each other otherwise.
