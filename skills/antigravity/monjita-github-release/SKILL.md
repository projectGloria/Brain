---
name: monjita-github-release
description: Releases the current projectGloria repo end to end - shows the current version and asks for the new one, bumps every version file, validates gloria.toml locally, makes sure .github/workflows/release.yml is the shared GloriaSetup template, commits and pushes to main, starts the release workflow, watches it, fixes and retries failures, and reports only when the GitHub Release is live. Trigger when the user invokes monjita-github-release or asks to cut/publish a new release.
---

# Release a projectGloria repo (`monjita-github-release`)

The user changed something (anything, even one string) and wants it released so GloriaLauncher can install or update it. Running this skill **is** the user's explicit request to commit, push, and release; no other confirmation is needed after they give the version.

How releases work (do not redesign this):
- `.github/workflows/release.yml` in every app is a copy of `GloriaSetup/templates/release.yml`. It runs **only** via `workflow_dispatch`; pushing to `main` never releases.
- Its `check` and `release` jobs are shared, from `projectGloria/GloriaSetup/.github/workflows/app-{check,release}.yml@main`. They check that the version files agree and that the version isn't already released. They use the **newest** `gloria-setup` on npm (never pinned), validate `gloria.toml`, pack, dry-run, and publish `v<version>` with `SHA256SUMS.txt`.
- Only the `build` job belongs to the repo: build, test, then copy the files to install into `stage/` and upload them as the artifact `app`.
- Repos without `gloria.toml` (browser extensions, Android) use the same template. Their `build` job puts the finished `.zip` / `.apk` in `stage/`, and those files are published as they are.

## Rules
1. **Work on `main`**, never create branches or tags (the release job creates the tag).
2. **Never edit the shared jobs** (`check`, `release`) in an app's `release.yml`, and never pin `gloria-setup` anywhere. If a shared job itself is broken, stop and tell the user (the fix belongs in GloriaSetup).
3. **Source truth only:** never guess executable names, icon paths, or build commands. Read the repo's manifests and scripts.
4. **Don't stop at "pushed".** The task is done when the GitHub Release is live, or after 3 failed attempts, with the reason reported.
5. **Path privacy:** use `C:\Users\you` in anything you write.

## Phase 1: Show the current version and ask
1. Read every version file present: `gloria.toml`, `package.json`, `src-tauri/tauri.conf.json`, `tauri.conf.json`, `Cargo.toml` / `src-tauri/Cargo.toml` (`[package]`), `manifest.json` (extensions), `android/app/build.gradle(.kts)` (`versionName`), `pyproject.toml`, `pubspec.yaml`.
2. Check the latest release: `gh release list --limit 3`.
3. Check the setup:
   - Is `release.yml` the template? It is if it references `GloriaSetup/.github/workflows/app-check.yml@main`.
   - Does `gloria.toml` pass `npx --yes gloria-setup@latest --check --manifest gloria.toml`?
4. Tell the user briefly:
   - **Current version:** `<version>` (list any file that disagrees).
   - **Last release:** `<tag>` or none.
   - **Release setup:** ready / will be migrated to the shared template / gloria.toml needs a fix: `<error>`.
   - Ask: `What should the new version be? (e.g. <suggested next>)`. For alpha versions suggest `-alpha.N+1`.
5. **Stop and wait.** If the user already gave the version with the command, go straight to Phase 2.

## Phase 2: Bump, validate, set up
Strip any leading `v` from the version the user gives (`1.2.0`, `1.2.0-alpha.3`).

1. **Bump every version file to the new version.**
   - If the repo has its own script (`scripts/version.mjs`, `npm run set:version`), use it.
   - Lockfiles:
     - `package-lock.json`: `npm version <v> --no-git-tag-version --allow-same-version` (or `npm install --package-lock-only`).
     - `Cargo.lock`: `cargo update --workspace --offline`.
   - Extensions: Chrome `manifest.json` `version` allows only numbers. Use the numeric part (`1.2.0`) and leave the full version in `package.json`.
2. **Validate locally (seconds, not CI minutes):**
   - `npx --yes gloria-setup@latest --check --manifest gloria.toml --version <v>`.
     - A TOML escape error means a backslash path is in double quotes. Use single quotes (`'%APPDATA%\MyApp'`).
     - `id`/`name` must match the GitHub repo name, so GloriaLauncher can match the install.
   - Run the repo's quick checks (typecheck, unit tests) from its `AGENTS.md` Commands/Done Criteria.
3. **Workflow:** if `release.yml` is not the template, copy the template:
   - Source: the sibling `../GloriaSetup/templates/release.yml`, or `gh api repos/projectGloria/GloriaSetup/contents/templates/release.yml --jq .content | base64 -d`.
   - Replace only its `Build` step with the repo's real build, taken from the old workflow and the repo's scripts:
     - **Tauri:** `npm ci`, then `npx tauri build --no-bundle`; copy `src-tauri/target/release/<exe>` (plus any resources) to `stage/`.
     - **Electron:** `npx electron-builder --win --x64 --dir --publish never`; copy `dist/win-unpacked/*` to `stage/`.
     - **Rust:** `cargo build --release --locked`; copy the exe to `stage/`.
     - **Python:** PyInstaller `--onedir --noconsole`; copy `dist/<App>/*` to `stage/`.
     - **Flutter:** `flutter build windows --release`; copy `build/windows/x64/runner/Release/*` to `stage/`.
     - **Extension:** build, then zip into `stage/<Name>-<version>.zip`.
     - **Android:** build the release APK into `stage/<Name>-<version>.apk`.
   - Keep any caches the old workflow had. Use explicit `actions/cache/restore` + `actions/cache/save` right after slow steps, so a later failure doesn't lose the cache.
   - Pin actions by full commit SHA with a `# vX` comment, and put an explicit `shell:` on every `run:`.
   - Delete other workflows that also build/publish releases.
4. If the repo has no `gloria.toml` but is a Windows desktop app, create one from real values: `id`, `name` (= repo name), `version`, `main_exe`, `icon` (a real `.ico`), `accent`, `description`, `data_folders` (single-quoted, `%APPDATA%\`/`%LOCALAPPDATA%\`/`%USERPROFILE%\` + app folder), and `needs_webview2 = true` for Tauri/WebView2 apps.

## Phase 3: Commit, push, start the release
1. `git status --short`; make sure you're on `main`. Stage the user's pending changes along with the bump. The user ran this skill to release what they changed.
2. Commit with one plain sentence (no `feat:`/`fix:` prefixes), e.g. `Release <v>: <what changed, in a few words>.` Then `git push origin main`.
3. Start the release:
   - `gh workflow run release.yml --ref main`.
   - Wait a few seconds.
   - Get the run id: `gh run list --workflow release.yml --limit 1 --json databaseId,headSha,status`. Confirm `headSha` matches `git rev-parse HEAD`.
4. Watch it with `gh run watch <id> --exit-status --interval 30`. Builds can take 5–40 minutes, so run it in the background if your tool supports that, and say so to the user.

## Phase 4: If it fails, fix and retry (max 3 attempts)
1. `gh run view <id> --log-failed | tail -n 80` to find the failing job and step.
2. Fix the cause in the repo:
   - `check` failed: version files disagree, an invalid `gloria.toml`, or the version is already released. For the last one, ask the user for a new version.
   - `build` failed: a build or test error.
   - `release` failed: a staged folder missing `main_exe`, or a bad icon.
   - Run the local validation again.
3. Commit (`Fix <what> so the <v> release builds.`), push, and start the workflow again (Phase 3, step 3). Keep the same version: it isn't published yet, and a leftover draft is reused.
4. After 3 failed attempts, or if the failure is in a shared job, stop and report the error, the run URL, and what you tried.

## Phase 5: Report
Only when the run succeeded:
- `<old>` → `<new>`, and the release URL (`gh release view v<new> --json url,assets`), with the asset names.
- The GloriaSetup version used (it's in the release notes).
- GloriaLauncher shows the update after its catalog refresh: up to about an hour on the web edition, or after the 15-minute cache on desktop. Use Refresh in the launcher to see it sooner.

## Releasing GloriaSetup itself
When the repo is GloriaSetup (it has `crates/gloria-pack/` and `npm/package.json` named `gloria-setup`), skip Phase 2's gloria.toml and workflow steps, and do this instead:
1. Set the new version in exactly these places (they must match, or the workflow refuses to publish):
   - `Cargo.toml` `[workspace.package].version`;
   - `npm/package.json` `"version"`;
   - the `projectGloria/GloriaSetup@v<version>` example in `README.md`;
   - `Cargo.lock`, via `cargo update --workspace --offline`.
2. Run its Done Criteria (`cargo build --workspace`, `cargo test --workspace`, the release build).
3. Commit, push, `gh workflow run release.yml --ref main`, and watch it. Do Phases 3–5 the same way. Its workflow publishes the GitHub Release and then `gloria-setup@<version>` to npm.
4. Report when npm shows the new version (`npm view gloria-setup version`). From then on, every app release uses it automatically.
