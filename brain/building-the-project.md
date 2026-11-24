---
apply_when: desktop-gui, windows-app
mode: reference
trigger: Before packaging, compiling release binaries, or building an installer
---
# Building & Packaging (GloriaSetup)
- **Installer Builder:** Use **GloriaSetup** (`gloria.toml` + `gloria-pack`) for Windows desktop apps—never switch to Inno Setup, NSIS, WiX, or default framework bundlers unless explicitly told.
- **Getting GloriaSetup (npm):** GloriaSetup is published on npm as `gloria-setup` (`https://www.npmjs.com/package/gloria-setup`) and ships both `gloria-pack.exe` and `gloria-stub.exe` (Windows x64, Node.js 18+). Always use the npm package to build installers instead of building GloriaSetup from source or hunting for binaries:
  - One-off / CI: `npx --yes gloria-setup@<version> <gloria-pack args>` (the `gloria-setup` bin aliases `gloria-pack`; `--yes` avoids prompts in CI).
  - Installed: `npm i -D gloria-setup` (or `npm i -g gloria-setup`), then call `npx gloria-pack` (or `gloria-pack`) directly.
  - Never pin a GloriaSetup version in a workflow. CI uses the shared release jobs (see `.brain/github-actions.md`), which pick the newest `gloria-setup` on npm at run time. Locally, use `npx --yes gloria-setup@latest`.
  - Validate the manifest in seconds before any long build: `npx --yes gloria-setup@latest --check --manifest gloria.toml` (add `--app <staged-dir>` to also check the staged app).
  - Only fall back to a local GloriaSetup build (`cargo build --release` in the GloriaSetup repo) when developing GloriaSetup itself or testing an unpublished change.
- **Manifest (`gloria.toml`):** Keep `id`, `name`, `version` (without a leading `v`), `main_exe`, `icon`, `accent`, `description`, and `data_folders` in `./gloria.toml`, keeping `version` synced in lockstep with all app manifests (`Cargo.toml`, `package.json`, `tauri.conf.json`). `id` and `name` must match the GitHub repo name so GloriaLauncher can match the installed app. Write Windows paths in single-quoted TOML strings (`data_folders = ['%APPDATA%\MyApp']`): in double quotes a backslash starts an escape, and `\d`, `\M` etc. fail to parse.
- **Packing Command & Output:** Pack staged release binaries into `dist/<Name>_<version>_x64-setup.exe` using `gloria-pack --app <staged-app-dir> --manifest gloria.toml --version <version> --out dist` (or the repository's staging script such as `scripts/package.ps1` / `pack.ps1`). `--stub` defaults to the `gloria-stub.exe` next to `gloria-pack.exe`, which is correct for the npm package, so omit it. Pass `--stub <gloria-stub.exe>` only when using a custom stub.
- **Per-User & GloriaLauncher Contract:** Installers are strictly per-user (no admin/HKLM) and register under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\projectGloria.<id>` with `Publisher = projectGloria`, unquoted `InstallLocation`, and bare full path `DisplayIcon` (no quotes or `,0`). `data_folders` must start with `%APPDATA%\`, `%LOCALAPPDATA%\`, or `%USERPROFILE%\` plus a concrete app subfolder.
- **Icons & WebView2:** Always embed the app's own `.ico` (never leave the default `GloriaSetup.ico` on a real installer). When `needs_webview2 = true`, bundle a supplied `MicrosoftEdgeWebview2Setup.exe` via `--webview2-bootstrapper`—never download it at install time.
- **Dry-Run Verification:** Verify every built setup from PowerShell using `Start-Process -FilePath dist\<Name>_<version>_x64-setup.exe -ArgumentList '/S --dry-run' -Wait -PassThru` and confirm `ExitCode -eq 0` (never pass `/S` directly from Git Bash/MSYS, which rewrites `/S` into a path and exits 6).
