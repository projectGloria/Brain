---
apply_when: desktop-gui, windows-app
mode: reference
trigger: Before packaging, compiling release binaries, or building an installer
---
# Building & Packaging (GloriaSetup)
- **Installer Builder:** Use **GloriaSetup** (`gloria.toml` + `gloria-pack`) for Windows desktop apps—never switch to Inno Setup, NSIS, WiX, or default framework bundlers unless explicitly told.
- **Manifest (`gloria.toml`):** Keep `id`, `name`, `version` (without a leading `v`), `main_exe`, `icon`, `accent`, `description`, and `data_folders` in `./gloria.toml`, keeping `version` synced in lockstep with all app manifests (`Cargo.toml`, `package.json`, `tauri.conf.json`).
- **Packing Command & Output:** Pack staged release binaries into `dist/<Name>_<version>_x64-setup.exe` using `gloria-pack --app <staged-app-dir> --stub <gloria-stub.exe> --manifest gloria.toml --version <version> --out dist` (or the repository's staging script such as `scripts/package.ps1` / `pack.ps1`).
- **Per-User & GloriaLauncher Contract:** Installers are strictly per-user (no admin/HKLM) and register under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\projectGloria.<id>` with `Publisher = projectGloria`, unquoted `InstallLocation`, and bare full path `DisplayIcon` (no quotes or `,0`). `data_folders` must start with `%APPDATA%\`, `%LOCALAPPDATA%\`, or `%USERPROFILE%\` plus a concrete app subfolder.
- **Icons & WebView2:** Always embed the app's own `.ico` (never leave the default `GloriaSetup.ico` on a real installer). When `needs_webview2 = true`, bundle a supplied `MicrosoftEdgeWebview2Setup.exe` via `--webview2-bootstrapper`—never download it at install time.
- **Dry-Run Verification:** Verify every built setup from PowerShell using `Start-Process -FilePath dist\<Name>_<version>_x64-setup.exe -ArgumentList '/S --dry-run' -Wait -PassThru` and confirm `ExitCode -eq 0` (never pass `/S` directly from Git Bash/MSYS, which rewrites `/S` into a path and exits 6).
