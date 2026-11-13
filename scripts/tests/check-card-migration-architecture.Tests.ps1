$ErrorActionPreference = 'Stop'

$scriptPath = Join-Path $PSScriptRoot '..\check-card-migration-architecture.ps1'
$source = Get-Content -LiteralPath $scriptPath -Raw

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) {
        throw $Message
    }
}

$cardLoopStart = $source.IndexOf('foreach ($card in $manifestCards)')
$proofStart = $source.IndexOf('$requiredProofs = @()', $cardLoopStart)
$combinedScanStart = $source.IndexOf('$oracleChecks = @(', $proofStart)
$combinedFileLoop = $source.IndexOf('foreach ($file in $productionFiles)', $combinedScanStart)
$emitLoopStart = $source.IndexOf('foreach ($check in $identityChecks)', $combinedFileLoop)

Assert-True ($cardLoopStart -ge 0 -and $proofStart -gt $cardLoopStart) `
    'Expected the affected-card loop and proof section.'
Assert-True ($combinedScanStart -gt $proofStart -and $combinedFileLoop -gt $combinedScanStart) `
    'Expected one combined oracle-identity scan after manifest processing.'
Assert-True ($emitLoopStart -gt $combinedFileLoop) `
    'Expected deterministic finding emission after the combined scan.'

$cardLoopPrefix = $source.Substring($cardLoopStart, $proofStart - $cardLoopStart)
$combinedScan = $source.Substring($combinedScanStart, $emitLoopStart - $combinedScanStart)
$emitSection = $source.Substring($emitLoopStart)
$collectorBlockStart = $cardLoopPrefix.IndexOf('$collectorPattern = $null')
$collectorBlockEnd = $cardLoopPrefix.IndexOf('$identityChecks.Add', $collectorBlockStart)
$collectorBlock = $cardLoopPrefix.Substring($collectorBlockStart, $collectorBlockEnd - $collectorBlockStart)

Assert-True ($cardLoopPrefix -match '\$collectorPatternsByValue\.ContainsKey') `
    'Collector identity regexes must be cached by unique collector value.'
Assert-True ($cardLoopPrefix -match '\$collectorPatternsByValue\[\$collectorKey\]\s*=\s*\[regex\]::new') `
    'Each unique collector identity regex must be constructed only once.'
Assert-True ($cardLoopPrefix -match 'RegexOptions\]::Compiled') `
    'Collector identity regex must be compiled before scanning production files.'
Assert-True ($collectorBlock -notmatch '\$oracleId') `
    'Collector identities must still be checked when an Oracle ID is also available.'
Assert-True ($cardLoopPrefix -match '\$identityChecks\.Add') `
    'Manifest processing must defer source scans through identity checks.'
Assert-True ($cardLoopPrefix -notmatch 'foreach \(\$file in \$productionFiles\)') `
    'Manifest processing must not rescan every production file per card.'
Assert-True ($combinedScan -match '\$oracleUuidPattern\.Matches\(\$content\)') `
    'Each production file must be traversed once by the UUID oracle pattern.'
Assert-True ($cardLoopPrefix -notmatch '\.IndexOf\(\$oracleId') `
    'The old card-by-file oracle-id search must not return to manifest processing.'
Assert-True ($emitSection -match '\$check\.CollectorPattern\.IsMatch') `
    'Collector checks must keep using the cached compiled regex.'
Assert-True ($emitSection -match '\$collectorMatchesByValue\.ContainsKey') `
    'Collector source matches must be cached instead of rescanning files per card.'
Assert-True ($source -match 'deepdeck-card-migration-architecture-cache/v1' -and
    $source -match 'Get-FileHash -LiteralPath \$file\.FullName -Algorithm SHA256' -and
    $source -match 'require-parser-proof=' -and
    $source -match 'require-all-proofs=') `
    'Architecture cache keys must hash the full Rust inventory and proof requirements.'
Assert-True ($source -match '\$architectureFingerprintAfter -ne \$architectureFingerprintBefore' -and
    $source -match 'Architecture inputs changed during the scan') `
    'Architecture scans must reject source drift before publishing a result.'
Assert-True ($source -match '\$violations\.Count -eq 0' -and
    $source -match 'Move-Item -LiteralPath \$temporary -Destination \$Path -Force') `
    'Only successful architecture results may be published atomically.'
Assert-True ($source -match 'DeepDeckArchitectureCache-' -and
    $source -match '\$mutex\.WaitOne') `
    'Architecture cache reads and publication must use a named cross-process mutex.'

Write-Host 'card-migration architecture loop tests passed.'
