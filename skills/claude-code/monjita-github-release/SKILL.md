---
name: monjita-github-release
description: Prompts the user for a new release version (showing the current version), syncs all project version manifests and gloria.toml, generates or updates .github/workflows/release.yml to build a Windows installer with GloriaSetup (gloria-setup) and publish it to GitHub Releases, and commits and pushes to main. Trigger when the user invokes monjita-github-release or asks to cut/publish a new GitHub release with GloriaSetup.
---

# GitHub Release with GloriaSetup (`monjita-github-release`)

Cut a new release in a single prompt turn: show the current version and ask for the new version number, then once the user replies with the target version, update all manifests, ensure `gloria.toml` and `.github/workflows/release.yml` build the app with **GloriaSetup** and publish it to the repository's **Releases** tab, and commit and push to `origin main`.

## Non-Negotiable Rules
1. **Two-turn interactive contract:**
   - **Turn 1 (Prompt):** Inspect the repository's version manifests (`gloria.toml`, `package.json`, `Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `tauri.conf.json`, `pyproject.toml`, `pubspec.yaml`), tell the user the current version, and ask for the new version number. Do not modify files, commit, or push before the user provides the version. (If the user already supplied the exact target version in their command, proceed directly to Turn 2.)
   - **Turn 2 (Execute):** Once the user gives the new version number, do everything end-to-end without asking for intermediate confirmation: bump all manifests, create/update `gloria.toml` and `.github/workflows/release.yml`, verify local checks, commit on `main`, and push to `origin main`.
2. **GloriaSetup only:** Package Windows installers exclusively with **GloriaSetup** (`gloria.toml` + the `gloria-setup` npm package / `gloria-pack`). Never use Inno Setup, NSIS, WiX, or default framework bundlers.
3. **Source truth only:** Never guess executable names, build scripts, icon paths, or staging directories. Inspect the repo's actual manifests and scripts before writing `gloria.toml` or `.github/workflows/release.yml`.
4. **Main branch push trigger (no manual git tags):** Always work directly on `main`. Never create branches or run `git tag` manually—pushing the version-bumped commit to `origin main` triggers `.github/workflows/release.yml`, which builds the installer and publishes the `v<version>` release to GitHub Releases.
5. **Path privacy:** Always use `C:\Users\you` in any comments, docs, or examples—never hardcode the local machine's real username.

---

## Workflow

Execute in strict order inside the repository root.

### Phase 1: Inspect Current Version & Ask User
1. Locate all version manifests present in the repository:
   - `gloria.toml` (`version = "..."`)
   - `package.json` (`"version": "..."`)
   - `Cargo.toml` / `src-tauri/Cargo.toml` (`version = "..."` under `[package]`)
   - `tauri.conf.json` / `src-tauri/tauri.conf.json` (`"version": "..."`)
   - `pyproject.toml` (`version = "..."`)
   - `pubspec.yaml` (`version: ...`)
2. Also check whether `gloria.toml` and `.github/workflows/release.yml` exist and whether the latest git tag/release (`git tag --sort=-v:refname | head -n 5` or `gh release list --limit 3`) matches.
3. Output a short prompt telling the user the current version and asking for the new one:
   - **Current Version:** `<current-version>` (note any mismatch across manifests if found)
   - **Release Setup:** `<gloria.toml + .github/workflows/release.yml status: ready / will be created / will be updated to GloriaSetup>`
   - Ask: `What is the new version number you want to release? (e.g., <suggested-next-version>)`
4. **Stop and wait** for the user's reply.

---

### Phase 2: Sync All Version Manifests & `gloria.toml`
As soon as the user replies with `<new_version>` (strip any accidental leading `v` so manifest versions stay bare `MAJOR.MINOR.PATCH` or `MAJOR.MINOR.PATCH-alpha.N`):

1. **Update every manifest in lockstep:**
   - Update `version` in all present manifests (`gloria.toml`, `package.json`, `Cargo.toml`, `src-tauri/Cargo.toml`, `tauri.conf.json`, `src-tauri/tauri.conf.json`, `pyproject.toml`, `pubspec.yaml`, and lockfiles if updated by `npm version --no-git-tag-version` or `cargo check`).
2. **Create or validate `gloria.toml`:**
   - If `gloria.toml` is missing, create it in the repo root using real values discovered from the codebase:
     ```toml
     id = "<AppId>"
     name = "<AppName>"
     version = "<new_version>"
     main_exe = "<main_exe_name>.exe"
     icon = "<relative/path/to/app_icon.ico>"
     accent = "<#hex_accent_color>"
     description = "<Concise 1-line description>"
     data_folders = ['%APPDATA%\<AppFolder>']
     ```
   - Ensure `icon` points to a real `.ico` file in the repo (convert the app's `.png`/`.svg` icon to `.ico` if no `.ico` exists yet—never leave the default `GloriaSetup.ico`).
   - Ensure `data_folders` entries start with `%APPDATA%\`, `%LOCALAPPDATA%\`, or `%USERPROFILE%\` plus a concrete app subfolder.
   - If `needs_webview2 = true`, ensure `--webview2-bootstrapper` is supplied when packing.

---

### Phase 3: Create or Update `.github/workflows/release.yml`
Inspect `.github/workflows/release.yml`. Create it if missing, or update it if it does not follow the three-stage GloriaSetup release pipeline (`version` -> `build` -> `publish`):

1. **Triggers & Permissions:**
   - Trigger on `push: branches: [main]` and `workflow_dispatch` with a boolean `dry_run` input (`default: true`).
   - Declare `permissions: contents: read` at the workflow root, and `permissions: contents: write` only on the `publish` job.
   - Pin all GitHub Actions (`actions/checkout`, `actions/setup-node`, `actions/upload-artifact`, `actions/download-artifact`, etc.) to full 40-char commit SHAs with a `# vX` comment.
   - Declare explicit `shell: bash` or `shell: pwsh` on every `run:` step.

2. **Job 1 — `version` (`ubuntu-latest`):**
   - Check out with `fetch-depth: 0`.
   - Read the version from `gloria.toml` and verify it matches the primary project manifest (`package.json`, `Cargo.toml`, `pyproject.toml`, `pubspec.yaml`, etc.).
   - Compare against `github.event.before` on `main`; output `version=<ver>` and `build=true` when the version changed (or on `workflow_dispatch`), otherwise `build=false`.

3. **Job 2 — `build` (`windows-latest`, `needs: version`, `if: needs.version.outputs.build == 'true'`):**
   - Set up the project toolchain (plus Node.js 18+ via `actions/setup-node` so `npx` is available).
   - Run release tests and build/stage the unpacked Windows x64 application payload into a staging folder (e.g., `app-dist`, `release/win-unpacked`, `target/release` staged dir, or via the repo's `./scripts/package.ps1 -StageOnly`):
     - **Electron:** `npm ci`, `npm run build`, `npx electron-builder --win --x64 --dir --publish never` (stage `dist/win-unpacked` or `release/win-unpacked`).
     - **Rust / Tauri:** `cargo build --release --locked` (stage the release binary and required runtime assets into a clean staging directory).
     - **Python:** Build standalone directory payload via PyInstaller (`--onedir --noconsole`) into `dist/<AppName>`.
     - **Flutter:** `flutter build windows --release` (`build/windows/x64/runner/Release`).
   - Pack the installer with the pinned `gloria-setup` npm package:
     ```powershell
     npx --yes gloria-setup@0.2.0 --app <staged-app-dir> --manifest gloria.toml --version ${{ needs.version.outputs.version }} --out dist
     ```
     *(Omit `--stub`, as `gloria-setup` bundles `gloria-stub.exe` alongside `gloria-pack.exe`.)*
   - Verify the built installer (`dist\<Name>_<version>_x64-setup.exe`) with a silent dry-run in PowerShell:
     ```powershell
     $setup = "dist\<Name>_${{ needs.version.outputs.version }}_x64-setup.exe"
     if (-not (Test-Path $setup)) { throw "Expected installer not found: $setup" }
     $proc = Start-Process -FilePath $setup -ArgumentList '/S --dry-run' -Wait -PassThru
     if ($proc.ExitCode -ne 0) { throw "GloriaSetup dry-run failed with exit code $($proc.ExitCode)" }
     ```
   - Upload `dist/<Name>_${{ needs.version.outputs.version }}_x64-setup.exe` via `actions/upload-artifact` with `compression-level: 0` and `if-no-files-found: error`.

4. **Job 3 — `publish` (`ubuntu-latest`, `needs: [version, build]`, `if: github.ref == 'refs/heads/main' && (github.event_name == 'push' || !inputs.dry_run)`):**
   - Download the installer artifact into `release-assets/`.
   - Generate `SHA256SUMS.txt`:
     ```bash
     cd release-assets
     sha256sum "*_x64-setup.exe" > SHA256SUMS.txt
     ```
   - Create a draft GitHub Release `v<version>` with `gh release create` (passing `--prerelease` automatically whenever `<version>` contains `-`, e.g. `-alpha.1`), upload `<Name>_<version>_x64-setup.exe` and `SHA256SUMS.txt`, re-download both into a temporary folder to verify `sha256sum --check SHA256SUMS.txt`, and undraft the release (`gh release edit "v${VERSION}" --draft=false`) so it appears live in the repository's **Releases** tab.

---

### Phase 4: Commit & Push to Trigger the Release
1. Verify git status (`git status --short`) and ensure you are on `main`.
2. Stage the updated version manifests, `gloria.toml`, and `.github/workflows/release.yml`.
3. Commit with a single plain sentence (no `feat:`/`fix:`/`chore:` prefixes), for example:
   `Bump version to <new_version> and configure GloriaSetup GitHub Release workflow.` (or `Bump version to <new_version> for release.` when the workflow already existed).
4. Push directly to `origin main`:
   ```bash
   git push origin main
   ```
5. Report concisely to the user:
   - Previous version -> new version (`<old>` -> `<new_version>`).
   - Files updated/created (manifests, `gloria.toml`, `.github/workflows/release.yml`).
   - Confirmation that the commit was pushed to `origin main` and the GitHub Actions release workflow is now building `<Name>_<new_version>_x64-setup.exe` for the **Releases** tab.
