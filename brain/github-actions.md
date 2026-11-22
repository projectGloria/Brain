---
apply_when: has-github-remote
mode: reference
trigger: Before creating/modifying `.github/workflows/*.yml`, setting up release automation, or preparing a first release push
---
# GitHub Actions & Releases
- **Triggers:** Run `.github/workflows/release.yml` on `push: branches: [main]` and `workflow_dispatch` with a boolean `dry_run` input (default `true`). Pushing a version-bumped commit to `main` automatically builds the installer with `gloria-setup` and publishes it to GitHub Releases.
- **Three-Stage Pipeline (`version` -> `build` -> `publish`):**
  1. `version` (`ubuntu-latest`): Read the current manifest version and compare it against `github.event.before` on `main`; emit `build=true` only when the version changed or on manual `workflow_dispatch`.
  2. `build` (`windows-latest`, `if: needs.version.outputs.build == 'true'`): Set up Node.js 18+ (`actions/setup-node`), run release tests, build & stage the app binaries, pack `dist/<Name>_<version>_x64-setup.exe` using the pinned `gloria-setup` npm package (`https://www.npmjs.com/package/gloria-setup`)—either `npx --yes gloria-setup@0.1.1 --app <staged-dir> --manifest gloria.toml --version <version> --out dist` or `npx gloria-pack ...` when pinned in `devDependencies` (omit `--stub`, as the npm package bundles `gloria-stub.exe`)—verify with `Start-Process ... -ArgumentList '/S --dry-run' -Wait -PassThru`, and upload the installer artifact with `compression-level: 0`.
  3. `publish` (`ubuntu-latest`, `needs: [version, build]`, `if: github.ref == 'refs/heads/main' && (github.event_name == 'push' || !inputs.dry_run)`): Download the installer artifact, generate `SHA256SUMS.txt`, create a draft GitHub Release `v<version>` (adding `--prerelease` whenever `<version>` contains `-`, e.g., `-alpha.1`), upload the installer and `SHA256SUMS.txt`, re-download both to verify `sha256sum --check SHA256SUMS.txt`, and undraft (`gh release edit v<version> --draft=false`).
- **Hardening & Shells:** Pin all actions (`actions/checkout`, `actions/setup-node`, `actions/upload-artifact`, `actions/download-artifact`) to full commit SHAs with a `# vX` comment. Set `permissions: contents: read` at the workflow level and `contents: write` only on `publish`. Always declare explicit `shell: pwsh` or `shell: bash` on every `run:` step.
