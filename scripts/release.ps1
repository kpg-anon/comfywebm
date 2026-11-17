<#
.SYNOPSIS
    Releases comfywebm from this PC: Windows is built here and published at
    once; GitHub Actions adds macOS and Linux to the same release afterwards.

.DESCRIPTION
    1. Checks: on main, the files it commits have no other changes, git
       identity is the kpg-anon noreply address, gh is signed in, the repo is
       public, the signing secrets exist on GitHub (for the macOS and Linux
       builds), the local key files exist, the tag is new.
    2. Sets the version everywhere (bump-version.ps1).
    3. CHANGELOG.md: "## [Unreleased]" becomes "## [x.y.z] - <today>" (an
       empty [Unreleased] stays above it). That section is the release notes.
    4. Deletes old bundles of this version, then builds the signed NSIS
       installer (npx tauri build) with the MSVC toolchain and checks that it
       is new. Microsoft Defender flagged the GNU (MinGW) builds as
       Trojan:Win32/Bearfoos.A!ml; the same source built with MSVC was clean.
       Then Defender scans the installer and the exe; a detection stops the
       release before anything is committed or published.
    5. The portable zip: comfywebm.exe and the "portable" file (an MSVC build
       has the WebView2 loader inside the exe).
    6. latest.json for Windows (the release workflow rewrites it with every
       platform when its builds are done).
    7. Commits only the files it changed ("release: vX.Y.Z") and pushes main.
    8. gh release create: makes the tag on that commit and publishes the
       release with the installer, its .sig, the portable zip and latest.json.
       Windows copies can update from now on.
    9. Publishing starts .github/workflows/release.yml, which builds macOS and
       Linux into the release (about 15 minutes) and rewrites latest.json.

    -DryRun prints the notes and the steps and changes nothing.
    See docs/SHIP.md (local) or scripts/README.md.

.EXAMPLE
    ./scripts/release.ps1 1.0.1 -DryRun
    ./scripts/release.ps1 1.0.1
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$repo = 'kpg-anon/comfywebm'
$tag = "v$Version"
$utf8 = New-Object System.Text.UTF8Encoding($false)
$files = @('package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml',
    'crates/engine/Cargo.toml', 'Cargo.lock', 'README.md', 'USAGE.md', 'CHANGELOG.md')
$keyPath = Join-Path $root '.tauri-updater.key'
$passPath = Join-Path $root '.tauri-updater.pass'
$release = Join-Path $root 'target/release'
$nsis = Join-Path $release 'bundle/nsis'
$setupName = "comfywebm_${Version}_x64-setup.exe"
$portableName = "comfywebm_${Version}_x64-portable.zip"
# the toolchain the release is built with, whatever the default on this PC is
$toolchain = 'stable-x86_64-pc-windows-msvc'
$mpcmd = Join-Path $env:ProgramFiles 'Windows Defender\MpCmdRun.exe'

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Run([string]$exe, [string[]]$argv) {
    Write-Host "    $exe $($argv -join ' ')" -ForegroundColor DarkGray
    if ($DryRun) { return }
    & $exe @argv
    if ($LASTEXITCODE) { throw "failed ($LASTEXITCODE): $exe $($argv -join ' ')" }
}
function Problem($msg) { if ($DryRun) { Write-Warning $msg } else { throw $msg } }

# ---------------------------------------------------------------- 1. checks
Step 'Checks'
$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne 'main') { Problem "On branch '$branch'; releases are made from main." }
$dirty = git status --porcelain -- @files
if ($dirty) { Problem "Uncommitted changes in files the release commit would include:`n$dirty" }
$email = (git config user.email).Trim()
if ($email -ne '128227694+kpg-anon@users.noreply.github.com') { Problem "git user.email is '$email', expected the kpg-anon noreply address." }
gh auth status *> $null
if ($LASTEXITCODE) { Problem 'gh is not signed in (gh auth login).' }
$private = (gh api "repos/$repo" --jq '.private').Trim()
if ($private -ne 'false') { Problem "The GitHub repo $repo is private; installed copies could not download the update." }
$secrets = (gh secret list --repo $repo) -join "`n"
foreach ($s in @('TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')) {
    if ($secrets -notmatch "(?m)^$s\b") { Problem "The repo secret $s is missing; the macOS and Linux builds could not sign their updates." }
}
if (-not (Test-Path $keyPath) -or -not (Test-Path $passPath)) { Problem 'The updater key files (.tauri-updater.key, .tauri-updater.pass) are missing.' }
git fetch -q --tags origin
if ($LASTEXITCODE) { Problem 'git fetch failed (network?); cannot check the tags on GitHub.' }
if (git tag --list $tag) { Problem "Tag $tag exists already." }
# MSVC: the toolchain is installed and finds Microsoft's linker (Visual Studio Build Tools, C++ workload)
$hostLine = (& rustup run $toolchain rustc -vV 2>$null) | Where-Object { $_ -like 'host:*' }
if ($hostLine -notlike '*windows-msvc*') { Problem "The Rust toolchain $toolchain is not installed (rustup toolchain install $toolchain)." }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) {
    Problem 'Visual Studio Build Tools (C++ workload) are not installed; the MSVC toolchain needs its linker.'
} elseif (-not (& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)) {
    Problem 'Visual Studio is installed without the C++ build tools (workload "Desktop development with C++" / VCTools).'
}
if (-not (Test-Path $mpcmd)) { Problem "Microsoft Defender's command line ($mpcmd) was not found; the release scan needs it." }
Write-Host '    done' -ForegroundColor Green

# ---------------------------------------------------------------- 2. version
Step "Version $Version"
if ($DryRun) { Write-Host '    (dry run) scripts/bump-version.ps1' -ForegroundColor DarkGray }
else { & (Join-Path $PSScriptRoot 'bump-version.ps1') $Version }

# ---------------------------------------------------------------- 3. changelog
Step 'Changelog section'
$clPath = Join-Path $root 'CHANGELOG.md'
$cl = [System.IO.File]::ReadAllText($clPath)
$nl = if ($cl.Contains("`r`n")) { "`r`n" } else { "`n" }
$verHead = "## [$Version]"
if (-not $cl.Contains($verHead)) {
    $un = [regex]::Match($cl, '(?m)^## \[Unreleased\][ \t]*\r?\n')
    if (-not $un.Success) { throw "CHANGELOG.md has neither '$verHead' nor '## [Unreleased]'." }
    $date = (Get-Date).ToString('yyyy-MM-dd')
    $cl = $cl.Substring(0, $un.Index) + "## [Unreleased]$nl$nl$verHead - $date$nl" + $cl.Substring($un.Index + $un.Length)
    if (-not $DryRun) { [System.IO.File]::WriteAllText($clPath, $cl, $utf8) }
    Write-Host "    [Unreleased] becomes '$verHead - $date'" -ForegroundColor Green
}
$lines = $cl -split "\r?\n"
$start = [array]::FindIndex($lines, [Predicate[string]] { param($l) $l.StartsWith($verHead) })
$body = @()
for ($i = $start + 1; $i -lt $lines.Count -and -not $lines[$i].StartsWith('## '); $i++) { $body += $lines[$i] }
$section = ($body -join "`n").Trim()
if (-not $section) { throw "The $verHead section of CHANGELOG.md is empty: write what changed under [Unreleased] first." }
$notes = "$section`n`n---`nFull changelog: https://github.com/$repo/blob/main/CHANGELOG.md"
Write-Host '    notes (the release and the in-app update notice show this):' -ForegroundColor DarkGray
$section -split "`n" | ForEach-Object { Write-Host "      $_" -ForegroundColor DarkGray }

# ---------------------------------------------------------------- 4. build Windows
Step "Build Windows (npx tauri build, signed, $toolchain)"
if (-not $DryRun) {
    # an installer of this version left from an earlier build (a test, a failed run) must not be published
    Get-ChildItem $nsis -Filter "*_${Version}_*" -ErrorAction SilentlyContinue | Remove-Item -Force
    Get-ChildItem (Join-Path $release 'bundle/portable') -Filter "*_${Version}_*" -ErrorAction SilentlyContinue | Remove-Item -Force
}
$buildStart = Get-Date
# the passphrase must not be empty: PowerShell 5.1 drops empty environment
# variables for child processes, and the bundler would then ask for it
if (-not $DryRun) {
    $env:TAURI_SIGNING_PRIVATE_KEY = [System.IO.File]::ReadAllText($keyPath)
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [System.IO.File]::ReadAllText($passPath).Trim()
}
# MSVC for this build only (rustup reads RUSTUP_TOOLCHAIN); the PC's default stays as it is
$env:RUSTUP_TOOLCHAIN = $toolchain
try { Run 'npx' @('tauri', 'build') } finally { Remove-Item Env:RUSTUP_TOOLCHAIN -ErrorAction SilentlyContinue }
$setup = Join-Path $nsis $setupName
$exe = Join-Path $release 'comfywebm.exe'
if (-not $DryRun) {
    foreach ($f in @($setup, "$setup.sig", $exe)) {
        if (-not (Test-Path $f)) { throw "$f was not built." }
        if ((Get-Item $f).LastWriteTime -lt $buildStart) { throw "$f is older than this build." }
    }
    Write-Host "    $setupName, $((Get-Item $setup).Length) bytes" -ForegroundColor Green
}

# ---------------------------------------------------------------- 4b. Defender
Step 'Microsoft Defender scan (report only)'
# Exit code 2 = a threat was found. -DisableRemediation reports without deleting the file.
foreach ($f in @($setup, $exe)) {
    Write-Host "    $f" -ForegroundColor DarkGray
    if ($DryRun) { continue }
    $scan = & $mpcmd -Scan -ScanType 3 -File $f -DisableRemediation 2>&1 | Out-String
    if ($LASTEXITCODE -eq 2) {
        throw "Microsoft Defender flags $f; nothing was committed or published.`n$($scan.Trim())"
    } elseif ($LASTEXITCODE -ne 0) {
        throw "The Defender scan of $f did not finish (exit $LASTEXITCODE).`n$($scan.Trim())"
    }
}
if (-not $DryRun) { Write-Host '    no threat found' -ForegroundColor Green }

# ---------------------------------------------------------------- 5. portable zip
Step 'Portable zip'
$portable = Join-Path $release "bundle/portable/$portableName"
if (-not $DryRun) {
    $dir = Join-Path $release 'portable'
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    New-Item -ItemType Directory -Force $dir, (Split-Path $portable) | Out-Null
    # an MSVC build has the WebView2 loader inside the exe (a WebView2Loader.dll in target/release is from an old GNU build)
    Copy-Item $exe $dir
    [System.IO.File]::WriteAllText((Join-Path $dir 'portable'),
        "This file makes comfywebm portable: while it is here, settings, presets, the queue,`r`nthe cache and the remembered options are kept in the `"data`" folder beside it.`r`n", $utf8)
    Compress-Archive -Path (Join-Path $dir '*') -DestinationPath $portable -Force
    Write-Host "    $portableName, $((Get-Item $portable).Length) bytes" -ForegroundColor Green
}

# ---------------------------------------------------------------- 6. latest.json (Windows)
Step 'latest.json (Windows; the workflow adds macOS and Linux)'
$latest = Join-Path $release 'latest.json'
if (-not $DryRun) {
    $manifest = [ordered]@{
        version   = $Version
        notes     = $notes
        pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        platforms = [ordered]@{
            'windows-x86_64' = [ordered]@{
                signature = ([System.IO.File]::ReadAllText("$setup.sig")).Trim()
                url       = "https://github.com/$repo/releases/download/$tag/$setupName"
            }
        }
    }
    [System.IO.File]::WriteAllText($latest, ($manifest | ConvertTo-Json -Depth 6), $utf8)
}

# ---------------------------------------------------------------- 7. commit, push
Step 'Commit and push main'
Run 'git' (@('add', '--') + $files)
Run 'git' @('commit', '-m', "release: $tag")
Run 'git' @('push', 'origin', 'main')
$sha = if ($DryRun) { '<commit>' } else { (git rev-parse HEAD).Trim() }

# ---------------------------------------------------------------- 8. publish
Step "Publish $tag"
$notesFile = Join-Path $env:TEMP "comfywebm-$tag-notes.md"
if (-not $DryRun) { [System.IO.File]::WriteAllText($notesFile, $notes, $utf8) }
Run 'gh' @('release', 'create', $tag, $setup, "$setup.sig", $portable, $latest,
    '--repo', $repo, '--target', $sha, '--title', "comfywebm $tag", '--notes-file', $notesFile, '--latest')
Run 'git' @('fetch', '-q', '--tags', 'origin')

Write-Host "`nPublished ${tag}: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Cyan
Write-Host "Windows copies can update now. macOS and Linux are being built (about 15 minutes):"
Write-Host "  https://github.com/$repo/actions/workflows/release.yml"
Write-Host "Until that finishes, latest.json lists Windows only; Mac and Linux copies find no update yet."
