---
apply_when: has-github-remote
mode: reference
trigger: Before creating/modifying `.github/workflows/*.yml`, setting up release automation, or debugging a failed release
---
# GitHub Actions & Releases
- **One template for every app:** `.github/workflows/release.yml` is a copy of `GloriaSetup/templates/release.yml` (`gh api repos/projectGloria/GloriaSetup/contents/templates/release.yml --jq .content | base64 -d`). Edit only its `build` job; never hand-write a release pipeline, and remove any other workflow that publishes releases.
- **Trigger:** `workflow_dispatch` only, with a boolean `dry_run` input (default `false`). Pushing to `main` never releases. A release is started by `/gloria-github-release` (`gh workflow run release.yml --ref main`).
- **Jobs:**
  1. `check` (shared, `projectGloria/GloriaSetup/.github/workflows/app-check.yml@main`, about 1 min): fails fast when the version files (`gloria.toml`, `package.json`, `tauri.conf.json`, `Cargo.toml`) disagree, when `v<version>` is already released, or when `gloria-pack --check` rejects `gloria.toml`. Picks the newest `gloria-setup` on npm.
  2. `build` (this repo, `windows-latest`): build and test, then copy the files to install into `stage/` and upload them as artifact `app`. Installer apps: `stage/` is the app folder holding `main_exe`. Extensions / Android: `stage/` holds the finished `.zip` / `.apk`.
  3. `release` (shared, `app-release.yml@main`, needs `contents: write`): installer apps are packed with that `gloria-setup` (WebView2 bootstrapper bundled when `needs_webview2 = true`) and dry-run with `/S --dry-run`; other apps publish their files as-is. Then draft -> upload + `SHA256SUMS.txt` -> verify -> undraft `v<version>` (`--prerelease` when the version contains `-`). A draft from a failed run is reused; a published release is never overwritten.
- **Never pin GloriaSetup:** the shared jobs use the newest `gloria-setup` at run time and are referenced at `@main`, so improvements reach every app without editing it. The version used is written in the release notes.
- **Caches:** For slow native steps (vcpkg, big toolchains) use `actions/cache/restore` before and `actions/cache/save` right after the slow step, so a later failure still keeps the cache. For Rust, `Swatinem/rust-cache` with `cache-on-failure: true`.
- **Debugging a failed run:** `gh run view <id> --log-failed`. Retry the same version after fixing (it isn't published yet); bump only if `check` says it's already released.
- **Hardening & Shells:** Pin all third-party actions to full commit SHAs with a `# vX` comment. `permissions: contents: read` at the workflow level, `contents: write` only on the `release` job. Explicit `shell: pwsh` or `shell: bash` on every `run:` step.
