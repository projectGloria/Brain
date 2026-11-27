---
apply_when: has-github-remote
mode: reference
trigger: Before creating/modifying `.github/workflows/*.yml`, setting up release automation, or debugging a failed release
---
# GitHub Actions & Releases
- **One template for every app:** `.github/workflows/release.yml` is a copy of `GloriaSetup/templates/release.yml`. Edit only its `build` job; never hand-write a release pipeline, and remove any other workflow that publishes releases. `brain release` installs or migrates it (a custom file is kept as `release.yml.pre-gloria` and its build steps are reused).
- **Trigger:** `workflow_dispatch` only, with a boolean `dry_run` input (default `false`). Pushing to `main` never releases. Start a release only with `brain release <version>` (what `/gloria-github-release` runs) or `brain release --all --patch`.
- **Jobs:**
  1. `check` (shared, `projectGloria/GloriaSetup/.github/workflows/app-check.yml@v0`, about 1 min): fails fast when the version files disagree, when `v<version>` is already released, when `gloria.toml` `id`/`name` differ from the repo name, or when `gloria.toml` is invalid.
  2. `build` (this repo, `windows-latest`): build and test, then copy the files to install into `stage/` and upload them as artifact `app`. Installer apps: `stage/` is the app folder holding `main_exe`. Extensions / Android: `stage/` holds the finished `.zip` / `.apk`.
  3. `release` (shared, `app-release.yml@v0`, needs `contents: write`): installer apps are packed with `gloria-setup` and dry-run with `/S --dry-run`; other apps publish their files as-is. Draft -> upload + `SHA256SUMS.txt` -> verify -> publish `v<version>` (`--prerelease` when the version contains `-`). A published release is never overwritten.
- **Never pin GloriaSetup:** the shared jobs are referenced at `@v0`, a tag GloriaSetup moves only after its own CI passed, and pick the matching `gloria-setup` at run time. Never pin a `gloria-setup` version in an app.
- **Failed run:** `brain release` prints the diagnosis and the failing log lines, and fixes formatting, lockfiles and version drift itself. For a code failure, fix the code and run `brain release --retry` (same version; it was not published). Bump only if `check` says the version is already released.
- **Caches:** for slow native steps use `actions/cache/restore` before and `actions/cache/save` right after the slow step; for Rust, `Swatinem/rust-cache` with `cache-on-failure: true`.
- **Hardening & Shells:** pin third-party actions to full commit SHAs with a `# vX` comment. `permissions: contents: read` at the workflow level, `contents: write` only on `release`. Explicit `shell: pwsh` or `shell: bash` on every `run:` step.
