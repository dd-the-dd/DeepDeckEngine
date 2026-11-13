[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$synchronizer = Join-Path $repositoryRoot "scripts/sync-official-keyword-surfaces.ps1"
$catalogPath = Join-Path $repositoryRoot "data/ability-catalog/ability-catalog-v1.json"
$snapshotPath = Join-Path $repositoryRoot "data/ability-catalog/official-ability-snapshot-v2.json"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-keyword-surfaces-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $temporaryRoot
try {
    $temporaryCatalog = Join-Path $temporaryRoot "catalog.json"
    $staleCatalog = Get-Content -LiteralPath $catalogPath -Raw -Encoding UTF8 | ConvertFrom-Json
    ($staleCatalog.entries | Where-Object canonicalId -eq "scry").rulesReferences[0].section = "701.18"
    ($staleCatalog.entries | Where-Object canonicalId -eq "surveil").rulesReferences[0].section = "701.42"
    [IO.File]::WriteAllText($temporaryCatalog, (($staleCatalog | ConvertTo-Json -Depth 40) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))

    & $synchronizer -CatalogPath $temporaryCatalog -SnapshotPath $snapshotPath
    $first = Get-Content -LiteralPath $temporaryCatalog -Raw -Encoding UTF8
    & $synchronizer -CatalogPath $temporaryCatalog -SnapshotPath $snapshotPath
    $second = Get-Content -LiteralPath $temporaryCatalog -Raw -Encoding UTF8
    if ($first -ne $second) { throw "Keyword surface synchronization is not deterministic" }

    $catalog = $second | ConvertFrom-Json
    $snapshot = Get-Content -LiteralPath $snapshotPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($catalog.catalogVersion -ne "2026.11.10-c13" -or @($catalog.entries).Count -ne 340) { throw "Unexpected synchronized catalog" }
    $documentary = @($catalog.entries | Where-Object {
        $_.category -in @("keywordAction", "keywordAbility") -and
        $_.support.lexical -eq "unsupported" -and $_.support.parser -eq "unsupported" -and $_.support.runtime -eq "unsupported"
    })
    if ($documentary.Count -ne 163) { throw "Expected 163 documentary-only keyword surfaces" }
    foreach ($entry in $documentary) {
        if (@($entry.aliases).Count -ne 0 -or @($entry.representativeCards).Count -ne 0) { throw "Documentary entry '$($entry.canonicalId)' invented lexical/card evidence" }
        if (@($entry.semantic.primitiveDecomposition).Count -ne 0 -or @($entry.semantic.intrinsicHooks).Count -ne 0 -or $entry.semantic.namedActionPerformed.policy -ne "never") { throw "Documentary entry '$($entry.canonicalId)' invented semantics" }
    }

    $official = @(
        @($snapshot.keywordActions | ForEach-Object { [pscustomobject]@{ category = "keywordAction"; surface = $_ } }) +
        @($snapshot.keywordAbilities | ForEach-Object { [pscustomobject]@{ category = "keywordAbility"; surface = $_ } })
    )
    foreach ($record in $official) {
        $matches = @($catalog.entries | Where-Object { $_.category -eq $record.category -and $_.name -eq $record.surface.name })
        if ($matches.Count -ne 1) { throw "Official identity '$($record.surface.name)' maps to $($matches.Count) catalog entries" }
        if (@($matches[0].rulesReferences | Where-Object section -eq $record.surface.section).Count -ne 1) { throw "Official section mismatch for '$($record.surface.name)'" }
        $officialTexts = @($record.surface.clauses | ForEach-Object { [string]$_.text })
        $catalogTexts = @($matches[0].rulesText.clauses | ForEach-Object { [string]$_.text })
        if (($officialTexts -join "`n") -ne ($catalogTexts -join "`n")) { throw "Official rules text drift for '$($record.surface.name)'" }
    }
    $scry = $catalog.entries | Where-Object canonicalId -eq "scry"
    $surveil = $catalog.entries | Where-Object canonicalId -eq "surveil"
    if ($scry.rulesReferences[0].section -ne "701.22" -or $surveil.rulesReferences[0].section -ne "701.25") { throw "Scry/Surveil realignment failed" }
    if (@($scry.semantic.primitiveDecomposition).Count -ne 4 -or $scry.semantic.namedActionPerformed.actionId -ne "scry") { throw "Scry enrichment was not preserved" }
    if (@($surveil.semantic.primitiveDecomposition).Count -ne 4 -or $surveil.semantic.namedActionPerformed.actionId -ne "surveil") { throw "Surveil enrichment was not preserved" }
    if (@($catalog.entries | Where-Object { $_.canonicalId -in @("forestwalk", "islandwalk", "mountainwalk", "plainswalk", "swampwalk", "typecycling", "basic-landcycling") }).Count -ne 7) { throw "Enriched parameterized variants were discarded" }
    Write-Host "Keyword synchronization tests passed: 266 official identities, 163 documentary entries, 103 enrichments preserved."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
