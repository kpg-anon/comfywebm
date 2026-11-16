<#
.SYNOPSIS
    Publishes a comfywebm release that installed copies pick up as an in-app update.

.DESCRIPTION
    1. Checks: on branch main, git identity is the kpg-anon noreply address,
       the GitHub repo is public (the updater downloads without signing in),
       the updater key files exist, and the tag does not exist yet.
    2. Sets the version everywhere (bump-version.ps1).
    3. CHANGELOG.md: when there is no "## [x.y.z]" section yet, the
       "## [Unreleased]" section becomes "## [x.y.z] - <today>" and an empty
       [Unreleased] is left above it. That section is the release notes.
    4. Builds the NSIS and MSI installers, signed with the updater key
       (.tauri-updater.key + .tauri-updater.pass, both gitignored).
    5. Writes latest.json: version, notes, date, and the signed NSIS installer's
       URL and signature. The app reads it from
       releases/latest/download/latest.json.
    6. Commits only the files this script changed ("release: vX.Y.Z"), tags,
       pushes main and the tag, and creates the GitHub release with the
       installers and latest.json attached.

    -DryRun prints the notes and the steps and changes nothing.

.EXAMPLE
    ./scripts/release.ps1 1.0.0 -DryRun
    ./scripts/release.ps1 1.0.0
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

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Run([string]$exe, [string[]]$argv) {
    Write-Host "    $exe $($argv -join ' ')" -ForegroundColor DarkGray
    if ($DryRun) { return }
    & $exe @argv
    if ($LASTEXITCODE) { throw "failed ($LASTEXITCODE): $exe $($argv -join ' ')" }
}

# ---------------------------------------------------------------- 1. checks
Step 'Checks'
$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne 'main') { throw "On branch '$branch'; releases are made from main." }
$email = (git config user.email).Trim()
if ($email -ne '128227694+kpg-anon@users.noreply.github.com') {
    throw "git user.email is '$email', expected the kpg-anon noreply address."
}
$private = (gh api "repos/$repo" --jq '.private').Trim()
if ($private -ne 'false') {
    $msg = "The GitHub repo $repo is private. Installed copies download latest.json and the installer without signing in, so they would get 404 and never update. Make the repo public first."
    if ($DryRun) { Write-Warning $msg } else { throw $msg }
}
if (git tag --list $tag) { throw "Tag $tag exists already." }
$keyPath = Join-Path $root '.tauri-updater.key'
$passPath = Join-Path $root '.tauri-updater.pass'
if (-not (Test-Path $keyPath) -or -not (Test-Path $passPath)) {
    throw "Updater key files missing (.tauri-updater.key, .tauri-updater.pass). See scripts/README.md."
}
Write-Host '    OK' -ForegroundColor Green

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
# the section's text: from its heading to the next "## " heading
$lines = $cl -split "\r?\n"
$start = [array]::FindIndex($lines, [Predicate[string]] { param($l) $l.StartsWith($verHead) })
$body = @()
for ($i = $start + 1; $i -lt $lines.Count -and -not $lines[$i].StartsWith('## '); $i++) { $body += $lines[$i] }
$section = ($body -join "`n").Trim()
if (-not $section) { throw "The $verHead section of CHANGELOG.md is empty." }
$notes = "$section`n`n---`nFull changelog: https://github.com/$repo/blob/main/CHANGELOG.md"
Write-Host '    notes:' -ForegroundColor DarkGray
$notes -split "`n" | ForEach-Object { Write-Host "      $_" -ForegroundColor DarkGray }

# ---------------------------------------------------------------- 4. build
Step 'Build (npx tauri build, signed)'
# the passphrase must not be empty: PowerShell 5.1 drops empty environment
# variables for child processes, and the bundler would then ask for it
$env:TAURI_SIGNING_PRIVATE_KEY = [System.IO.File]::ReadAllText($keyPath)
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [System.IO.File]::ReadAllText($passPath).Trim()
Run 'npx' @('tauri', 'build')
$bundleDir = Join-Path $root 'target/release/bundle'
$setup = Get-ChildItem (Join-Path $bundleDir 'nsis') -Filter "comfywebm_${Version}_x64-setup.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
$msi = Get-ChildItem (Join-Path $bundleDir 'msi') -Filter "comfywebm_${Version}_x64_*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $DryRun) {
    if (-not $setup) { throw "No NSIS installer for $Version in $bundleDir/nsis." }
    if (-not (Test-Path "$($setup.FullName).sig")) { throw "No updater signature $($setup.FullName).sig (createUpdaterArtifacts and the key)." }
}

# ---------------------------------------------------------------- 5. latest.json
Step 'latest.json'
$latest = Join-Path $root 'target/release/latest.json'
if (-not $DryRun) {
    $manifest = [ordered]@{
        version   = $Version
        notes     = $notes
        pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        platforms = [ordered]@{
            'windows-x86_64' = [ordered]@{
                signature = ([System.IO.File]::ReadAllText("$($setup.FullName).sig")).Trim()
                url       = "https://github.com/$repo/releases/download/$tag/$($setup.Name)"
            }
        }
    }
    [System.IO.File]::WriteAllText($latest, ($manifest | ConvertTo-Json -Depth 6), $utf8)
    Write-Host "    $latest" -ForegroundColor Green
}

# ---------------------------------------------------------------- 6. publish
Step "Commit, tag $tag, push, GitHub release"
$files = @('package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml',
    'crates/engine/Cargo.toml', 'Cargo.lock', 'README.md', 'CHANGELOG.md')
Run 'git' (@('add', '--') + $files)
Run 'git' @('commit', '-m', "release: $tag")
Run 'git' @('tag', $tag)
Run 'git' @('push', 'origin', 'main')
Run 'git' @('push', 'origin', $tag)
$notesFile = Join-Path $env:TEMP "comfywebm-$tag-notes.md"
if (-not $DryRun) { [System.IO.File]::WriteAllText($notesFile, $notes, $utf8) }
$assets = @()
if ($setup) { $assets += $setup.FullName }
if ($msi) { $assets += $msi.FullName }
$assets += $latest
Run 'gh' (@('release', 'create', $tag) + $assets + @('--repo', $repo, '--title', "comfywebm $tag", '--notes-file', $notesFile))

Write-Host "`nReleased ${tag}: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Cyan
