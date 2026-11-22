---
apply_when: has-github-remote
mode: reference
trigger: Before creating or modifying `.github/workflows/*.yml` or release automation
---
# GitHub Actions & Releases
- **Triggers:** Run `.github/workflows/release.yml` on `push: branches: [main]` and `workflow_dispatch` with a boolean `dry_run` input (default `true`).
- **Three-Stage Pipeline (`version` -> `build` -> `publish`):**
  1. `version` (`ubuntu-latest`): Read the current manifest version and compare it against `github.event.before` on `main`; emit `build=true` only when the version changed or on manual `workflow_dispatch`.
  2. `build` (`windows-latest`, `if: needs.version.outputs.build == 'true'`): Run release tests, stage the app binaries, pack `dist/<Name>_<version>_x64-setup.exe` via `projectGloria/GloriaSetup` (`upload: 'false'`), verify with `Start-Process ... -ArgumentList '/S --dry-run' -Wait -PassThru`, and upload the artifact with `compression-level: 0`.
  3. `publish` (`ubuntu-latest`, `needs: [version, build]`, `if: github.ref == 'refs/heads/main' && (github.event_name == 'push' || !inputs.dry_run)`): Download the installer artifact, generate `SHA256SUMS.txt`, create a draft GitHub Release `v<version>` (adding `--prerelease` whenever `<version>` contains `-`, e.g., `-alpha.1`), upload the installer and `SHA256SUMS.txt`, re-download both to verify `sha256sum --check SHA256SUMS.txt`, and undraft (`gh release edit v<version> --draft=false`).
- **Hardening & Shells:** Pin all actions (`actions/checkout`, `actions/upload-artifact`, `actions/download-artifact`, `projectGloria/GloriaSetup`) to full commit SHAs with a `# vX` comment. Set `permissions: contents: read` at the workflow level and `contents: write` only on `publish`. Always declare explicit `shell: pwsh` or `shell: bash` on every `run:` step.
