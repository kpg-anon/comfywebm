<#
.SYNOPSIS
    Sets the comfywebm version in every file that holds it.

.DESCRIPTION
    package.json, package-lock.json, src-tauri/tauri.conf.json, both Cargo.toml
    files, the two workspace packages in Cargo.lock, and the README (version
    badge and installer file name). Running it twice with the same version
    changes nothing.

.EXAMPLE
    ./scripts/bump-version.ps1 1.0.1
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# PowerShell 5.1's Get-Content/Set-Content use the ANSI code page; read and write UTF-8 (no BOM).
$utf8 = New-Object System.Text.UTF8Encoding($false)
function Read-Text($p) { [System.IO.File]::ReadAllText($p) }
function Write-Text($p, $s) { [System.IO.File]::WriteAllText($p, $s, $utf8) }

# First = $true: only the first match (the file's own version, not a dependency's).
$targets = @(
    @{ Path = 'package.json';              Pattern = '("version"\s*:\s*")\d+\.\d+\.\d+(")';                                    First = $true }
    @{ Path = 'src-tauri/tauri.conf.json'; Pattern = '("version"\s*:\s*")\d+\.\d+\.\d+(")';                                    First = $true }
    @{ Path = 'src-tauri/Cargo.toml';      Pattern = '(?m)^(version\s*=\s*")\d+\.\d+\.\d+(")';                                 First = $true }
    @{ Path = 'crates/engine/Cargo.toml';  Pattern = '(?m)^(version\s*=\s*")\d+\.\d+\.\d+(")';                                 First = $true }
    # package-lock.json has the project version twice, each right after "name": "comfywebm"
    @{ Path = 'package-lock.json';         Pattern = '("name"\s*:\s*"comfywebm"\s*,\s*"version"\s*:\s*")\d+\.\d+\.\d+(")'; First = $false }
    @{ Path = 'README.md';                 Pattern = '(badge/version-)\d+\.\d+\.\d+(-)';                                       First = $false }
    @{ Path = 'README.md';                 Pattern = '(alt="Version )\d+\.\d+\.\d+(")';                                        First = $false }
    @{ Path = 'README.md';                 Pattern = '(comfywebm_)\d+\.\d+\.\d+(_x64)';                                        First = $false }
)

foreach ($t in $targets) {
    $full = Join-Path $root $t.Path
    if (-not (Test-Path $full)) { Write-Warning "missing: $($t.Path)"; continue }
    $text = Read-Text $full
    $rx = [regex]$t.Pattern
    if (-not $rx.IsMatch($text)) { Write-Host "  no match  $($t.Path)  $($t.Pattern)" -ForegroundColor DarkGray; continue }
    $repl = '${1}' + $Version + '${2}'
    $new = if ($t.First) { $rx.Replace($text, $repl, 1) } else { $rx.Replace($text, $repl) }
    if ($new -ne $text) {
        Write-Text $full $new
        Write-Host "  set       $($t.Path)" -ForegroundColor Green
    } else {
        Write-Host "  unchanged $($t.Path)" -ForegroundColor DarkGray
    }
}

# Cargo.lock: the two workspace packages, by name, then their version line
$lock = Join-Path $root 'Cargo.lock'
$text = Read-Text $lock
$new = $text
foreach ($name in @('comfywebm', 'comfywebm-engine')) {
    $rx = [regex]"(?m)(^name = ""$([regex]::Escape($name))""\r?\nversion = "")\d+\.\d+\.\d+("")"
    $new = $rx.Replace($new, "`${1}$Version`${2}", 1)
}
if ($new -ne $text) { Write-Text $lock $new; Write-Host "  set       Cargo.lock" -ForegroundColor Green }
else { Write-Host "  unchanged Cargo.lock" -ForegroundColor DarkGray }

Write-Host "Version is $Version." -ForegroundColor Cyan
