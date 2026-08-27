$ErrorActionPreference = 'Stop'

$forbiddenNames = @(
    '.env',
    'cards-minimized.json',
    'deck-analytics.json'
)
$forbiddenExtensions = @('.pem', '.p12', '.pfx', '.sqlite', '.sqlite3')
$tracked = git ls-files

foreach ($path in $tracked) {
    $leaf = Split-Path -Leaf $path
    $extension = [System.IO.Path]::GetExtension($path).ToLowerInvariant()
    if ($forbiddenNames -contains $leaf -or $forbiddenExtensions -contains $extension) {
        throw "Forbidden public artifact is tracked: $path"
    }
}

$ErrorActionPreference = 'Continue'
$matches = git grep -n -I -E -- '-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----|AIza[0-9A-Za-z_-]{30,}|gh[pousr]_[0-9A-Za-z]{30,}' -- . ':!scripts/check-public-tree.ps1'
$grepExitCode = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($grepExitCode -eq 0) {
    $matches
    throw 'Potential credential material found in tracked files.'
}
if ($grepExitCode -ne 1) {
    throw 'Credential scan failed.'
}

Write-Host 'Public-tree audit passed.'
exit 0
