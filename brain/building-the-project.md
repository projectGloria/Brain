---
apply_when: desktop-gui, windows-app
mode: reference
trigger: Before packaging, compiling release binaries, or building an installer
---
# Building & Packaging (GloriaSetup)
- Use **GloriaSetup** as the installer builder—never switch to Inno Setup, NSIS, WiX, or default framework bundlers unless explicitly told.
- <Exact GloriaSetup CLI command / config file path / output directory>
- <Version sync rule: keep GloriaSetup installer version synced with app manifest>
- <Any code-signing, icon, or asset bundling requirements>