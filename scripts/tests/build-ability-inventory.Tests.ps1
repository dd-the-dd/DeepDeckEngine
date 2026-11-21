[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$builder = Join-Path $repositoryRoot "scripts/build-ability-inventory.ps1"
$catalogPath = Join-Path $repositoryRoot "data/ability-catalog/ability-catalog-v1.json"
$snapshotPath = Join-Path $repositoryRoot "data/ability-catalog/official-ability-snapshot-v2.json"
$checkedInventoryPath = Join-Path $repositoryRoot "data/ability-catalog/ability-inventory-v2.json"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-ability-inventory-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $temporaryRoot
try {
    $firstJson = & $builder -CatalogPath $catalogPath -SnapshotPath $snapshotPath -PrintInventory | Out-String
    $secondJson = & $builder -CatalogPath $catalogPath -SnapshotPath $snapshotPath -PrintInventory | Out-String
    if ($firstJson.Trim() -ne $secondJson.Trim()) { throw "Ability inventory generation is not deterministic" }
    $inventory = $firstJson | ConvertFrom-Json

    if ($inventory.inventorySchemaVersion -ne "deepdeck-ability-inventory/v2") { throw "Unexpected inventory schema version" }
    if (@($inventory.entries).Count -ne 332 -or $inventory.coverage.authoritativeTotal -ne 332) { throw "Official inventory denominator must remain 332 for CR 2026-11-06" }
    if ($inventory.coverage.catalogued -ne 332 -or $inventory.coverage.uncatalogued -ne 0) { throw "Every official surface must have a documentary catalog entry" }
    if ($inventory.coverage.byCategory.keywordAction.missing -ne 0 -or $inventory.coverage.byCategory.keywordAbility.missing -ne 0 -or $inventory.coverage.byCategory.abilityWord.missing -ne 0) { throw "Official documentary map is incomplete" }
    if ($inventory.integration.keywordSurfaceTotal -ne 266 -or $inventory.integration.enriched -ne 104 -or $inventory.integration.documentaryUnsupported -ne 162) { throw "Unexpected keyword integration split" }
    if (@($inventory.missingQueue).Count -ne 0 -or @($inventory.integrationQueue).Count -ne 162) { throw "Documentary coverage and integration debt must remain separate" }
    if ($inventory.coverage.supplementalRulesSurfaceTotal -ne 1 -or $inventory.integration.supplementalRulesSurfaceTotal -ne 1 -or $inventory.integration.supplementalDocumentaryUnsupported -ne 0) { throw "Unexpected supplemental rules-surface coverage" }
    if (@($inventory.supplementalRulesSurfaces).Count -ne 1 -or @($inventory.supplementalIntegrationQueue).Count -ne 0) { throw "Certified supplemental rules surface must leave the integration queue" }
    $chapterSymbol = $inventory.supplementalRulesSurfaces | Where-Object catalogId -eq "chapter-symbol"
    if ($null -eq $chapterSymbol -or $chapterSymbol.category -ne "marker" -or $chapterSymbol.integrationStatus -ne "enriched") { throw "Chapter Symbol supplemental inventory promotion missing" }
    if ($chapterSymbol.support.lexical -ne "supportedNative" -or $chapterSymbol.support.parser -ne "supportedNative" -or $chapterSymbol.support.runtime -ne "supportedNative") { throw "Chapter Symbol support evidence is incomplete" }
    if (($chapterSymbol.rulesSections -join ",") -ne "107.15,715.2") { throw "Chapter Symbol supplemental rules references changed" }
    if (@($inventory.entries | Where-Object inventoryId -eq "chapter-symbol").Count -ne 0) { throw "Supplemental Chapter Symbol changed the official 701/702 denominator" }

    $scry = $inventory.entries | Where-Object { $_.category -eq "keywordAction" -and $_.name -eq "Scry" }
    $surveil = $inventory.entries | Where-Object { $_.category -eq "keywordAction" -and $_.name -eq "Surveil" }
    if ($scry.rulesSection -ne "701.22" -or @($scry.catalogIds)[0] -ne "scry") { throw "Scry identity/section mismatch" }
    if ($surveil.rulesSection -ne "701.25" -or @($surveil.catalogIds)[0] -ne "surveil") { throw "Surveil identity/section mismatch" }
    foreach ($identity in @(
        [pscustomobject]@{ name = "Play"; id = "play"; section = "701.18" },
        [pscustomobject]@{ name = "Meld"; id = "meld"; section = "701.42" },
        [pscustomobject]@{ name = "Landwalk"; id = "landwalk"; section = "702.14" }
    )) {
        $entry = $inventory.entries | Where-Object { $_.name -eq $identity.name -and $_.rulesSection -eq $identity.section }
        if (@($entry.catalogIds)[0] -ne $identity.id -or $entry.integrationStatus -ne "documentaryUnsupported") { throw "Official identity '$($identity.name)' was not kept distinct" }
    }
    foreach ($newRule in @("Empower Jace", "Assimilate", "Beam me up")) {
        $entry = $inventory.entries | Where-Object name -eq $newRule
        if ($null -eq $entry -or $entry.integrationStatus -ne "documentaryUnsupported") { throw "Current official surface '$newRule' must remain visible as integration debt" }
    }
    if ($inventory.integrationQueue[0].name -ne "Cast" -or $inventory.integrationQueue[0].fixtureOracleMentions -lt 1) { throw "Integration queue is not frequency-prioritized" }

    $checked = Get-Content -LiteralPath $checkedInventoryPath -Raw -Encoding UTF8 | ConvertFrom-Json | ConvertTo-Json -Depth 30 -Compress
    $generated = $inventory | ConvertTo-Json -Depth 30 -Compress
    if ($checked -ne $generated) { throw "Checked inventory v2 is stale" }

    $mismatchedCatalogPath = Join-Path $temporaryRoot "section-mismatch.json"
    $mismatchedCatalog = Get-Content -LiteralPath $catalogPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $mismatchedScry = $mismatchedCatalog.entries | Where-Object canonicalId -eq "scry"
    ($mismatchedScry.rulesReferences | Where-Object { $_.section -match '^701\.' } | Select-Object -First 1).section = "701.18"
    [IO.File]::WriteAllText($mismatchedCatalogPath, (($mismatchedCatalog | ConvertTo-Json -Depth 40) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    $mismatchFailed = $false
    try {
        $null = & $builder -CatalogPath $mismatchedCatalogPath -SnapshotPath $snapshotPath -PrintInventory
    } catch {
        $mismatchFailed = $_.Exception.Message -match "Section mismatch.*Scry"
    }
    if (-not $mismatchFailed) { throw "Inventory did not reject a stale section identity" }

    Write-Host "Ability inventory tests passed: 332 official surfaces plus one supplemental Chapter Symbol marker."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
