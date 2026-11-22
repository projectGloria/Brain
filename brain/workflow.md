---
apply_when: always
mode: inline
---
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until I explicitly say otherwise.
- **Versioning (Minimal Changes):** Bump the alpha number by `+1` on every minimal change (e.g., `0.1.0-alpha.1` -> `0.1.0-alpha.2`) across all version manifests in the repo.
- **Versioning (Major Changes):** Before changing minor/major versions on breaking or major feature work, stop and ask me for the exact target version.