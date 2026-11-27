---
name: gloria-create-tools
description:  Generates tailored Windows-first dev scripts in gloriaTools/ (free-port dev runner, pre-flight check, clean & unlock, local release pack, setup/doctor, process stopper, status, release preview, dependency audit, interactive menu) with Brain's `brain tools` command. Trigger when the user invokes gloria-create-tools or asks to create/generate project dev tools or scripts.
---

# Generate gloriaTools/ for this repo

Brain detects the stack, ports, binaries and commands from the repo and writes the scripts itself. `brain` = `"%USERPROFILE%\.brain\brain.cmd"`.

1. **Preview (read-only):** run `brain tools --plan` in the repo root. It lists every tool as `NEW`, `IN_SYNC`, `OUTDATED` or `CUSTOM` (custom user scripts are never touched). Show the list and ask which to write; end with `Reply "apply" to write these tools, or tell me which to skip.`
2. **Apply:** after `apply`, run `brain tools` (all) or `brain tools --only dev,check,clean` (a subset), and print what it wrote and kept.
3. **Repo-specific extras:** only if the user asks for a tool Brain does not generate, write it by hand as `gloriaTools/<name>.bat` in the same hybrid `.bat` + PowerShell format as the generated files (copy one as the template, keep its `# Description:` line so `menu.bat` lists it). Use real commands from the manifests only, and make `stop`/`clean`-style tools target only this repo's processes.

Never commit, push, tag or bump versions. Use `C:\Users\you` in any example path.
