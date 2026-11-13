[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$synchronizer = Join-Path $repositoryRoot "scripts/sync-official-ability-words.ps1"
$catalogPath = Join-Path $repositoryRoot "data/ability-catalog/ability-catalog-v1.json"
$snapshotPath = Join-Path $repositoryRoot "data/ability-catalog/official-ability-snapshot-v2.json"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-ability-words-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $temporaryRoot
try {
    $temporaryCatalog = Join-Path $temporaryRoot "catalog.json"
    Copy-Item -LiteralPath $catalogPath -Destination $temporaryCatalog
    & $synchronizer -CatalogPath $temporaryCatalog -SnapshotPath $snapshotPath
    $first = Get-Content -LiteralPath $temporaryCatalog -Raw -Encoding UTF8
    & $synchronizer -CatalogPath $temporaryCatalog -SnapshotPath $snapshotPath
    $second = Get-Content -LiteralPath $temporaryCatalog -Raw -Encoding UTF8
    if ($first -ne $second) { throw "Ability-word synchronization is not deterministic" }

    $catalog = $second | ConvertFrom-Json
    $snapshot = Get-Content -LiteralPath $snapshotPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $abilityWords = @($catalog.entries | Where-Object category -eq "abilityWord")
    if ($abilityWords.Count -ne 66) { throw "Expected all 66 official ability words" }

    $catalogLexemes = @{}
    foreach ($entry in $abilityWords) {
        foreach ($lexeme in @([string]$entry.name) + @($entry.aliases | ForEach-Object { [string]$_ })) {
            $catalogLexemes[$lexeme.ToLowerInvariant()] = $entry
        }
    }
    foreach ($officialWord in @($snapshot.abilityWords)) {
        if (-not $catalogLexemes.ContainsKey(([string]$officialWord).ToLowerInvariant())) {
            throw "Official ability word '$officialWord' is absent"
        }
    }

    $generated = @($abilityWords | Where-Object canonicalId -notin @("channel", "renewal", "revolt"))
    if ($generated.Count -ne 63) { throw "Expected 63 generated ability-word records" }
    foreach ($entry in $generated) {
        if (@($entry.aliases).Count -ne 0) { throw "Generated ability word '$($entry.canonicalId)' invented an alias" }
        if (@($entry.rulesReferences).Count -ne 1 -or $entry.rulesReferences[0].section -ne "207.2c") { throw "Generated ability word '$($entry.canonicalId)' has the wrong rules reference" }
        if (@($entry.semantic.primitiveDecomposition).Count -ne 0 -or @($entry.semantic.intrinsicHooks).Count -ne 0 -or $entry.semantic.namedActionPerformed.policy -ne "never") { throw "Generated ability word '$($entry.canonicalId)' gained semantics" }
        if ($entry.support.lexical -ne "supportedNative" -or $entry.support.parser -ne "unsupported" -or $entry.support.runtime -ne "unsupported") { throw "Generated ability word '$($entry.canonicalId)' overstates support" }
        if (@($entry.representativeCards).Count -ne 0) { throw "Generated ability word '$($entry.canonicalId)' invented card evidence" }
    }
    Write-Host "Ability-word synchronization tests passed: 66 official labels, 63 generated lexical-only markers."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
