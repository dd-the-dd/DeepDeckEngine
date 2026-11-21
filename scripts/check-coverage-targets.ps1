$ErrorActionPreference = "Stop"

# Keeps the explicit coverage-target quarantine and its tracked list in sync.
# Every `ignore` in Rust sources must be a coverage-target quarantine
#   #[cfg_attr(not(feature = "coverage-targets"), ignore = "coverage target: ...")]
# and every quarantined test must have exactly one row in docs/coverage-targets.md
# (first cell `path::test_fn`), and vice versa.

$repoRoot = Split-Path -Parent $PSScriptRoot
$listPath = Join-Path $repoRoot "docs/coverage-targets.md"

function Get-RelativePath([string]$Path) {
    return [IO.Path]::GetFullPath($Path).Substring([IO.Path]::GetFullPath($repoRoot).Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
}

$quarantinePattern = [regex]'cfg_attr\(\s*not\(feature\s*=\s*"coverage-targets"\)\s*,\s*ignore\s*=\s*"coverage target:[^"]*"\s*\)\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([A-Za-z_][A-Za-z0-9_]*)'
$anyIgnorePattern = [regex]'(?m)(#\[ignore\b|\bignore\s*=\s*")'

$sourceRoots = @("src", "crates", "tests") | ForEach-Object { Join-Path $repoRoot $_ } | Where-Object { Test-Path -LiteralPath $_ }
$files = @($sourceRoots | ForEach-Object { Get-ChildItem -LiteralPath $_ -Recurse -File -Filter "*.rs" })

$quarantined = @{}
$errors = @()
foreach ($file in $files) {
    $text = [IO.File]::ReadAllText($file.FullName)
    $ignoreCount = $anyIgnorePattern.Matches($text).Count
    if ($ignoreCount -eq 0) { continue }
    $relative = Get-RelativePath $file.FullName
    $found = $quarantinePattern.Matches($text)
    if ($found.Count -ne $ignoreCount) {
        $errors += "${relative}: $ignoreCount ignore attribute(s) but $($found.Count) coverage-target quarantine(s); every ignore must be a documented coverage target"
    }
    foreach ($match in $found) {
        $key = "${relative}::$($match.Groups[1].Value)"
        if ($quarantined.ContainsKey($key)) { $errors += "duplicate quarantined test $key" }
        $quarantined[$key] = $true
    }
}

if (-not (Test-Path -LiteralPath $listPath)) { throw "missing docs/coverage-targets.md" }
$listed = @{}
foreach ($line in [IO.File]::ReadAllLines($listPath)) {
    if ($line -notmatch '^\|\s*`([^`]+::[A-Za-z_][A-Za-z0-9_]*)`\s*\|') { continue }
    $key = $Matches[1]
    if ($listed.ContainsKey($key)) { $errors += "docs/coverage-targets.md lists $key twice" }
    $listed[$key] = $true
}

foreach ($key in $quarantined.Keys | Sort-Object) {
    if (-not $listed.ContainsKey($key)) { $errors += "quarantined but not listed in docs/coverage-targets.md: $key" }
}
foreach ($key in $listed.Keys | Sort-Object) {
    if (-not $quarantined.ContainsKey($key)) { $errors += "listed in docs/coverage-targets.md but not quarantined (remove the row, or restore the attribute): $key" }
}
if ($errors.Count -ne 0) {
    throw ($errors -join [Environment]::NewLine)
}
Write-Host "Coverage targets verified: $($quarantined.Count) quarantined tests listed in docs/coverage-targets.md."
