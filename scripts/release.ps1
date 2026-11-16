<#
.SYNOPSIS
    Starts a comfywebm release: version, changelog section, commit, tag, push.
    GitHub Actions (.github/workflows/release.yml) then builds Windows, Linux
    and macOS and puts everything on a draft release.

.DESCRIPTION
    1. Checks: on branch main with nothing uncommitted in the files it changes,
       git identity is the kpg-anon noreply address, the GitHub repo is public
       (the updater downloads without signing in), the updater signing secrets
       exist on GitHub, and the tag does not exist yet.
    2. Sets the version everywhere (bump-version.ps1).
    3. CHANGELOG.md: when there is no "## [x.y.z]" section yet, the
       "## [Unreleased]" section becomes "## [x.y.z] - <today>" and an empty
       [Unreleased] is left above it. That section is the release notes.
    4. Commits only the files it changed ("release: vX.Y.Z"), tags vX.Y.Z and
       pushes main and the tag.

    Then: Actions > release builds for about 15 minutes and creates a draft
    release with the installers and latest.json. Look at it and press Publish;
    only a published release is offered to installed copies.

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
$files = @('package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml',
    'crates/engine/Cargo.toml', 'Cargo.lock', 'README.md', 'USAGE.md', 'CHANGELOG.md')

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
$private = (gh api "repos/$repo" --jq '.private').Trim()
if ($private -ne 'false') {
    Problem "The GitHub repo $repo is private. Installed copies download latest.json and the installers without signing in, so they would get 404 and never update. Make the repo public first."
}
$secrets = (gh secret list --repo $repo) -join "`n"
foreach ($s in @('TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')) {
    if ($secrets -notmatch "(?m)^$s\b") { Problem "The repo secret $s is missing; the build could not sign the updates. See scripts/README.md." }
}
if (git tag --list $tag) { Problem "Tag $tag exists already." }
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
if (-not $section) { throw "The $verHead section of CHANGELOG.md is empty." }
Write-Host '    notes (the release and the in-app update notice show this):' -ForegroundColor DarkGray
$section -split "`n" | ForEach-Object { Write-Host "      $_" -ForegroundColor DarkGray }

# ---------------------------------------------------------------- 4. publish the tag
Step "Commit, tag $tag, push"
Run 'git' (@('add', '--') + $files)
Run 'git' @('commit', '-m', "release: $tag")
Run 'git' @('tag', $tag)
Run 'git' @('push', 'origin', 'main')
Run 'git' @('push', 'origin', $tag)

Write-Host "`nTagged $tag. GitHub Actions now builds it:" -ForegroundColor Cyan
Write-Host "  https://github.com/$repo/actions/workflows/release.yml"
Write-Host "When it is done, check the draft and press Publish:"
Write-Host "  https://github.com/$repo/releases"
