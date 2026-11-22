@echo off
setlocal
set "BRAIN_SCRIPT_DIR=%~dp0"
set "BRAIN_SELF=%~f0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$lines = [System.IO.File]::ReadAllLines($env:BRAIN_SELF, [System.Text.Encoding]::UTF8); $idx = [Array]::IndexOf($lines, '# --- POWERSHELL_START ---'); $code = ($lines[($idx+1)..($lines.Length-1)] -join \"`n\"); & ([scriptblock]::Create($code)) @args" -- %* & exit /b
# --- POWERSHELL_START ---
$ErrorActionPreference = 'Stop'

$ScriptDir = $env:BRAIN_SCRIPT_DIR.TrimEnd('\', '/')
$UserHome  = $env:USERPROFILE
$MasterDir = Join-Path $UserHome '.brain'

function Write-Utf8NoBom([string]$Path, [string]$Content) {
    $dir = Split-Path -Parent $Path
    if ($dir -and -not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Content, $utf8NoBom)
}

function Read-Utf8([string]$Path) {
    return [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
}

function Get-BrainFiles([string]$BrainDir, [string]$ProjectBrainDir = '') {
    $map = @{}
    if ($BrainDir -and (Test-Path -LiteralPath $BrainDir)) {
        Get-ChildItem -LiteralPath $BrainDir -Filter '*.md' -File | Where-Object {
            $_.Name -notmatch '^(?i)init(-brain)?\.md$'
        } | ForEach-Object {
            $map[$_.Name.ToLowerInvariant()] = @{ Name = $_.Name; Path = $_.FullName }
        }
    }
    if ($ProjectBrainDir -and (Test-Path -LiteralPath $ProjectBrainDir)) {
        Get-ChildItem -LiteralPath $ProjectBrainDir -Filter '*.md' -File | Where-Object {
            $_.Name -notmatch '^(?i)init(-brain)?\.md$'
        } | ForEach-Object {
            $key = $_.Name.ToLowerInvariant()
            if (-not $map.ContainsKey($key) -and $_.Name -match '^(?i)local-') {
                $map[$key] = @{ Name = $_.Name; Path = $_.FullName }
            } elseif (-not (Test-Path -LiteralPath $BrainDir)) {
                $map[$key] = @{ Name = $_.Name; Path = $_.FullName }
            }
        }
    }
    return ($map.Keys | Sort-Object | ForEach-Object { $map[$_] })
}

function Get-BrainFingerprint([string]$BrainDir, [string]$ProjectBrainDir = '') {
    $files = Get-BrainFiles -BrainDir $BrainDir -ProjectBrainDir $ProjectBrainDir
    if (-not $files -or $files.Count -eq 0) { return '000000000000' }
    $sb = New-Object System.Text.StringBuilder
    foreach ($f in $files) {
        $raw = Read-Utf8 $f.Path
        $norm = ($raw -replace "`r`n", "`n" -replace "`r", "`n").TrimEnd("`n")
        [void]$sb.Append($f.Name.ToLowerInvariant()).Append("`n").Append($norm).Append("`n")
    }
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($sb.ToString())
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hashBytes = $sha.ComputeHash($bytes)
    } finally {
        $sha.Dispose()
    }
    $hex = -join ($hashBytes | ForEach-Object { $_.ToString('x2') })
    return $hex.Substring(0, 12)
}

function Sync-SkillFilesFromInit([string]$RepoRoot) {
    $initPath = Join-Path $RepoRoot 'brain\init.md'
    if (-not (Test-Path -LiteralPath $initPath)) { return $false }
    $raw = (Read-Utf8 $initPath) -replace "`r`n", "`n"
    $marker = "## Non-Negotiable Rules`n"
    $pos = $raw.IndexOf($marker)
    if ($pos -lt 0) { throw "Could not find '$marker' in $initPath" }
    $body = $raw.Substring($pos)

    $stdHeader = @"
---
name: init-brain
description: Generate, bootstrap, refresh, or update AGENTS.md by combining repo ground truth with personal rules from .brain/*.md (synced from the master ~/.brain). Use ONLY when the user explicitly asks to run init-brain, bootstrap or refresh project context, learn/persist a rule, or check whether AGENTS.md is stale. Do not use for ordinary coding tasks.
---
# init-brain — Project Context & Brain Compiler

Generate, bootstrap, refresh, or update ``AGENTS.md`` by combining **repository ground truth** with matching personal rules from ``.brain/*.md``, enforcing a strict 120-line budget and aggressive pruning on every run.

User arguments: any text the user typed when invoking this skill (e.g. ``check``, ``learn <rule>``, or a short project description such as "Tauri desktop app for X").

"@ -replace "`r`n", "`n"

    $claudeHeader = @"
---
name: init-brain
description: Generate, bootstrap, refresh, or update AGENTS.md by combining repo ground truth with personal rules from .brain/*.md (synced from the master ~/.brain). Run only when the user explicitly asks for init-brain, to bootstrap or refresh project context, to learn/persist a rule, or to check whether AGENTS.md is stale.
argument-hint: "[check] | [learn <rule>] | [short project description for bootstrap]"
disable-model-invocation: true
---
# /init-brain — Project Context & Brain Compiler

Generate, bootstrap, refresh, or update ``AGENTS.md`` by combining **repository ground truth** with matching personal rules from ``.brain/*.md``, enforcing a strict 120-line budget and aggressive pruning on every run.

User arguments: ```$ARGUMENTS``

"@ -replace "`r`n", "`n"

    $claudeBody = $body `
        -replace "If the user's invocation starts with", 'If `$ARGUMENTS` starts with' `
        -replace "If the user's invocation text is exactly", 'If `$ARGUMENTS` is exactly' `
        -replace "If neither the invocation text nor", 'If neither `$ARGUMENTS` nor'

    Write-Utf8NoBom (Join-Path $RepoRoot 'skills\antigravity\init-brain\SKILL.md') ($stdHeader + $body)
    Write-Utf8NoBom (Join-Path $RepoRoot 'skills\codex\init-brain\SKILL.md')       ($stdHeader + $body)
    Write-Utf8NoBom (Join-Path $RepoRoot 'skills\claude-code\init-brain\SKILL.md') ($claudeHeader + $claudeBody)
    return $true
}

function Invoke-Install {
    $sourceBrainDir = Join-Path $ScriptDir 'brain'
    $isRepoRoot = Test-Path -LiteralPath (Join-Path $sourceBrainDir 'init.md')

    if ($isRepoRoot) {
        [void](Sync-SkillFilesFromInit -RepoRoot $ScriptDir)
        Write-Host "[1/3] Synced 3 SKILL.md files from brain\init.md" -ForegroundColor Cyan
    } else {
        $sourceBrainDir = $ScriptDir
    }

    if (-not (Test-Path -LiteralPath $MasterDir)) {
        New-Item -ItemType Directory -Force -Path $MasterDir | Out-Null
    }

    $sourceMd = Get-ChildItem -LiteralPath $sourceBrainDir -Filter '*.md' -File
    $sourceNames = @{}
    foreach ($f in $sourceMd) {
        $sourceNames[$f.Name.ToLowerInvariant()] = $true
        $dest = Join-Path $MasterDir $f.Name
        if ($f.FullName -ne $dest) {
            Copy-Item -LiteralPath $f.FullName -Destination $dest -Force
        }
    }

    # Prune ghost files in ~/.brain (except local-*.md)
    $pruned = @()
    Get-ChildItem -LiteralPath $MasterDir -Filter '*.md' -File | ForEach-Object {
        $k = $_.Name.ToLowerInvariant()
        if (-not $sourceNames.ContainsKey($k) -and $_.Name -notmatch '^(?i)local-') {
            Remove-Item -LiteralPath $_.FullName -Force
            $pruned += $_.Name
        }
    }

    # Copy install-brain.bat itself into ~/.brain so agents can invoke "%USERPROFILE%\.brain\install-brain.bat"
    $selfInMaster = Join-Path $MasterDir 'install-brain.bat'
    if ($env:BRAIN_SELF -ne $selfInMaster) {
        Copy-Item -LiteralPath $env:BRAIN_SELF -Destination $selfInMaster -Force
    }

    Write-Host "[2/3] Installed master brain -> $MasterDir ($($sourceMd.Count) files)" -ForegroundColor Green
    if ($pruned.Count -gt 0) {
        Write-Host "      Pruned stale master files: $($pruned -join ', ')" -ForegroundColor Yellow
    }

    if ($isRepoRoot) {
        $targets = @(
            @{ Tool = 'Claude Code'; Src = 'skills\claude-code\init-brain\SKILL.md'; Dst = Join-Path $UserHome '.claude\skills\init-brain\SKILL.md' },
            @{ Tool = 'Codex';       Src = 'skills\codex\init-brain\SKILL.md';       Dst = Join-Path $UserHome '.agents\skills\init-brain\SKILL.md' },
            @{ Tool = 'Antigravity'; Src = 'skills\antigravity\init-brain\SKILL.md'; Dst = Join-Path $UserHome '.gemini\config\skills\init-brain\SKILL.md' }
        )
        Write-Host "[3/3] Installed init-brain skills:" -ForegroundColor Green
        foreach ($t in $targets) {
            $srcPath = Join-Path $ScriptDir $t.Src
            $dstDir  = Split-Path -Parent $t.Dst
            New-Item -ItemType Directory -Force -Path $dstDir | Out-Null
            Copy-Item -LiteralPath $srcPath -Destination $t.Dst -Force
            Write-Host ("      - {0,-12} -> {1}" -f $t.Tool, $t.Dst)
        }
    }

    $fp = Get-BrainFingerprint -BrainDir $MasterDir
    Write-Host ""
    Write-Host "Master brain-sync fingerprint: $fp" -ForegroundColor Cyan
    Write-Host "Marker: <!-- brain-sync: $fp -->" -ForegroundColor DarkCyan

    # Check for stub placeholders in master brain (un-backticked <...> template bullets)
    $stubs = @()
    foreach ($f in (Get-BrainFiles -BrainDir $MasterDir)) {
        $content = Read-Utf8 $f.Path
        if ($content -match '(?m)^\s*-\s+(?:\*\*[^*]+\*\*:\s*)?<[^`>\r\n]+>\s*$') {
            $stubs += $f.Name
        }
    }
    if ($stubs.Count -gt 0) {
        Write-Host "Incomplete brain files (contain <...> stubs): $($stubs -join ', ')" -ForegroundColor Yellow
    } else {
        Write-Host "All master brain rule files are complete (0 stub placeholders)." -ForegroundColor Green
    }

    $legacySkill = Join-Path $UserHome '.gemini\config\skills\agent-context-generator'
    if (Test-Path -LiteralPath $legacySkill) {
        Write-Host ""
        Write-Host "Note: Legacy skill found at $legacySkill" -ForegroundColor Yellow
        Write-Host "      Run 'install-brain.bat remove-legacy' to remove it so it does not collide with init-brain." -ForegroundColor Yellow
    }
}

function Invoke-SyncProject([string]$TargetDir) {
    $projRoot = (Resolve-Path -LiteralPath $TargetDir).Path
    $srcDir = if (Test-Path -LiteralPath $MasterDir) { $MasterDir } else { Join-Path $ScriptDir 'brain' }
    if (-not (Test-Path -LiteralPath $srcDir)) {
        throw "No master brain found at $MasterDir"
    }

    $projBrain = Join-Path $projRoot '.brain'
    New-Item -ItemType Directory -Force -Path $projBrain | Out-Null

    $masterFiles = Get-BrainFiles -BrainDir $srcDir
    $masterNames = @{}
    $copied = @()
    foreach ($f in $masterFiles) {
        $masterNames[$f.Name.ToLowerInvariant()] = $true
        Copy-Item -LiteralPath $f.Path -Destination (Join-Path $projBrain $f.Name) -Force
        $copied += $f.Name
    }

    $deleted = @()
    Get-ChildItem -LiteralPath $projBrain -Filter '*.md' -File | ForEach-Object {
        $k = $_.Name.ToLowerInvariant()
        if (-not $masterNames.ContainsKey($k) -and $_.Name -notmatch '^(?i)local-') {
            Remove-Item -LiteralPath $_.FullName -Force
            $deleted += $_.Name
        }
    }

    $gitDir = Join-Path $projRoot '.git'
    $excluded = $false
    if (Test-Path -LiteralPath $gitDir -PathType Container) {
        $excludePath = Join-Path $gitDir 'info\exclude'
        $infoDir = Split-Path -Parent $excludePath
        if (-not (Test-Path -LiteralPath $infoDir)) {
            New-Item -ItemType Directory -Force -Path $infoDir | Out-Null
        }
        $existing = if (Test-Path -LiteralPath $excludePath) { Read-Utf8 $excludePath } else { '' }
        if ($existing -notmatch '(?m)^\s*\.brain/?\s*$') {
            $prefix = if ($existing.Length -gt 0 -and -not $existing.EndsWith("`n")) { "`n" } else { '' }
            Write-Utf8NoBom $excludePath ($existing + $prefix + ".brain/`n")
        }
        $excluded = $true
    }

    $fp = Get-BrainFingerprint -BrainDir $srcDir -ProjectBrainDir $projBrain
    Write-Host "Synced .brain/ in $projRoot" -ForegroundColor Green
    Write-Host "  Copied/Updated : $($copied -join ', ')"
    if ($deleted.Count -gt 0) {
        Write-Host "  Pruned Ghosts  : $($deleted -join ', ')" -ForegroundColor Yellow
    }
    if ($excluded) {
        Write-Host "  Git Exclude    : .brain/ ensured in .git/info/exclude"
    }
    Write-Host "  Sync Marker    : <!-- brain-sync: $fp -->" -ForegroundColor Cyan
}

function Get-RepoCheckStatus([string]$RepoPath) {
    $agentsPath = Join-Path $RepoPath 'AGENTS.md'
    $srcDir = if (Test-Path -LiteralPath $MasterDir) { $MasterDir } else { Join-Path $ScriptDir 'brain' }
    $projBrain = Join-Path $RepoPath '.brain'
    $expectedFp = Get-BrainFingerprint -BrainDir $srcDir -ProjectBrainDir $projBrain

    if (-not (Test-Path -LiteralPath $agentsPath)) {
        return [PSCustomObject]@{
            Repo        = Split-Path -Leaf $RepoPath
            Status      = 'NO AGENTS.MD'
            Lines       = '-'
            SyncMarker  = '-'
            Expected    = $expectedFp
            Message     = 'AGENTS.md does not exist.'
        }
    }

    $lines = [System.IO.File]::ReadAllLines($agentsPath, [System.Text.Encoding]::UTF8)
    $lineCount = $lines.Count
    $lastNonEmpty = ($lines | Where-Object { $_.Trim() -ne '' } | Select-Object -Last 1)
    $markerFp = $null
    if ($lastNonEmpty -match '<!--\s*brain-sync:\s*([0-9a-fA-F]{12})\b') {
        $markerFp = $Matches[1].ToLowerInvariant()
    }

    # Check if any manifest or workflow is newer than AGENTS.md
    $agentsTime = (Get-Item -LiteralPath $agentsPath).LastWriteTimeUtc
    $watchFiles = @('package.json', 'pyproject.toml', 'Cargo.toml', 'src-tauri\Cargo.toml', 'src-tauri\tauri.conf.json', 'tauri.conf.json', 'gloria.toml', 'go.mod')
    $driftedFiles = @()
    foreach ($wf in $watchFiles) {
        $p = Join-Path $RepoPath $wf
        if (Test-Path -LiteralPath $p) {
            if ((Get-Item -LiteralPath $p).LastWriteTimeUtc -gt $agentsTime) {
                $driftedFiles += $wf
            }
        }
    }
    $wfDir = Join-Path $RepoPath '.github\workflows'
    if (Test-Path -LiteralPath $wfDir) {
        Get-ChildItem -LiteralPath $wfDir -Filter '*.yml' -File | ForEach-Object {
            if ($_.LastWriteTimeUtc -gt $agentsTime) {
                $driftedFiles += ".github/workflows/$($_.Name)"
            }
        }
    }

    $status = 'UP TO DATE'
    $msg = 'AGENTS.md is up to date.'
    if (-not $markerFp) {
        $status = 'NO MARKER'
        $msg = 'Refresh due: no sync marker.'
    } elseif ($markerFp -ne $expectedFp) {
        $status = 'BRAIN DRIFT'
        $msg = "Refresh due: brain rules changed ($markerFp != $expectedFp)."
    }

    if ($driftedFiles.Count -gt 0) {
        if ($status -eq 'UP TO DATE') { $status = 'REPO DRIFT' }
        $msg += " Refresh suggested: repo files changed since last sync ($($driftedFiles -join ', '))."
    }
    if ($lineCount -gt 120) {
        $status = 'OVER BUDGET'
        $msg += " Line budget exceeded ($lineCount / 120)."
    }

    return [PSCustomObject]@{
        Repo       = Split-Path -Leaf $RepoPath
        Status     = $status
        Lines      = "$lineCount/120"
        SyncMarker = if ($markerFp) { $markerFp } else { 'none' }
        Expected   = $expectedFp
        Message    = $msg
    }
}

$cmd = if ($args.Count -gt 0) { $args[0].ToLowerInvariant() } else { 'install' }
$arg1 = if ($args.Count -gt 1) { $args[1] } else { '' }

switch ($cmd) {
    'install' {
        Invoke-Install
    }
    'hash' {
        $target = if ($arg1) { (Resolve-Path -LiteralPath $arg1).Path } else { (Get-Location).Path }
        $srcDir = if (Test-Path -LiteralPath $MasterDir) { $MasterDir } else { Join-Path $ScriptDir 'brain' }
        $fp = Get-BrainFingerprint -BrainDir $srcDir -ProjectBrainDir (Join-Path $target '.brain')
        Write-Output "<!-- brain-sync: $fp -->"
    }
    'sync' {
        $target = if ($arg1) { $arg1 } else { '.' }
        Invoke-SyncProject -TargetDir $target
    }
    'check' {
        $target = if ($arg1) { (Resolve-Path -LiteralPath $arg1).Path } else { (Get-Location).Path }
        $res = Get-RepoCheckStatus -RepoPath $target
        Write-Host "$($res.Repo): $($res.Message) [Lines: $($res.Lines), Marker: $($res.SyncMarker), Master: $($res.Expected)]"
    }
    'check-all' {
        $parent = if ($arg1) { (Resolve-Path -LiteralPath $arg1).Path } else { Split-Path -Parent $ScriptDir }
        $dirs = Get-ChildItem -LiteralPath $parent -Directory | Sort-Object Name
        $results = foreach ($d in $dirs) {
            Get-RepoCheckStatus -RepoPath $d.FullName
        }
        $results | Format-Table -Property Repo, Status, Lines, SyncMarker, Expected -AutoSize
        $counts = $results | Group-Object Status | ForEach-Object { "$($_.Name): $($_.Count)" }
        Write-Host ("Summary ({0} folders): {1}" -f $results.Count, ($counts -join ' | ')) -ForegroundColor Cyan
    }
    'remove-legacy' {
        $legacySkill = Join-Path $UserHome '.gemini\config\skills\agent-context-generator'
        if (Test-Path -LiteralPath $legacySkill) {
            Remove-Item -LiteralPath $legacySkill -Recurse -Force
            Write-Host "Removed legacy skill: $legacySkill" -ForegroundColor Green
        } else {
            Write-Host "Legacy skill not present." -ForegroundColor DarkGray
        }
    }
    default {
        Write-Host "Usage:" -ForegroundColor Cyan
        Write-Host "  install-brain.bat                  Install/update ~/.brain and all 3 tool skills (Claude Code, Codex, Antigravity)"
        Write-Host "  install-brain.bat check [dir]      Run Phase 0 refresh & fingerprint check on a repository"
        Write-Host "  install-brain.bat check-all [dir]  Scan all sibling repositories and report AGENTS.md sync status"
        Write-Host "  install-brain.bat sync [dir]       Sync ~/.brain -> <dir>/.brain, prune ghosts, and update .git/info/exclude"
        Write-Host "  install-brain.bat hash [dir]       Print the exact <!-- brain-sync: <12-char-sha256> --> line"
        Write-Host "  install-brain.bat remove-legacy    Remove legacy ~/.gemini/config/skills/agent-context-generator skill"
    }
}
