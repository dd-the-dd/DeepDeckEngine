[CmdletBinding()]
param(
    [string]$CatalogPath = "data/ability-catalog/ability-catalog-v1.json",
    [string]$SnapshotPath = "data/ability-catalog/official-ability-snapshot-v2.json",
    [string]$EmitInventoryPath,
    [switch]$PrintInventory
)

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Resolve-RepositoryPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

function ConvertTo-NormalizedIdentity {
    param([Parameter(Mandatory = $true)][string]$Value)
    return (($Value.Normalize([Text.NormalizationForm]::FormKC).Trim().ToLowerInvariant()) -replace '\s+', ' ')
}

function ConvertTo-InventoryId {
    param([Parameter(Mandatory = $true)][string]$Name)
    $normalized = $Name.Normalize([Text.NormalizationForm]::FormKC).ToLowerInvariant().Replace("'", "").Replace([string][char]0x2019, "")
    return (($normalized -replace '[^a-z0-9]+', '-').Trim('-'))
}

function New-Frequency {
    param([Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string]$FixtureCorpus)
    $pattern = '(?i)(?<![A-Za-z])' + [regex]::Escape($Name) + '(?![A-Za-z])'
    return [ordered]@{
        fixtureOracleMentions = [regex]::Matches($FixtureCorpus, $pattern).Count
        basis = "case-insensitive lexical mentions in fixtures/**/*.json; not card or play frequency"
    }
}

$catalog = Get-Content -LiteralPath (Resolve-RepositoryPath $CatalogPath) -Raw -Encoding UTF8 | ConvertFrom-Json
$snapshot = Get-Content -LiteralPath (Resolve-RepositoryPath $SnapshotPath) -Raw -Encoding UTF8 | ConvertFrom-Json
if ($snapshot.snapshotSchemaVersion -ne "deepdeck-official-ability-snapshot/v2") { throw "Ability inventory requires official snapshot v2" }
if ($snapshot.source.effectiveDate -ne $catalog.rulesVersion.effectiveDate) { throw "Catalog and snapshot rules dates differ" }

$fixtureFiles = @(Get-ChildItem -LiteralPath (Join-Path $repositoryRoot "fixtures") -Recurse -File -Filter "*.json")
$fixtureCorpus = ($fixtureFiles | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 }) -join [Environment]::NewLine

$owners = @{}
foreach ($entry in @($catalog.entries)) {
    foreach ($lexeme in @([string]$entry.name) + @($entry.aliases | ForEach-Object { [string]$_ })) {
        $key = "$($entry.category)|$(ConvertTo-NormalizedIdentity $lexeme)"
        if (-not $owners.ContainsKey($key)) { $owners[$key] = [System.Collections.Generic.List[object]]::new() }
        if (-not @($owners[$key] | Where-Object canonicalId -eq $entry.canonicalId).Count) { $owners[$key].Add($entry) }
    }
}

$entries = [System.Collections.Generic.List[object]]::new()
foreach ($categoryRecord in @(
    [pscustomobject]@{ category = "keywordAction"; surfaces = @($snapshot.keywordActions) },
    [pscustomobject]@{ category = "keywordAbility"; surfaces = @($snapshot.keywordAbilities) }
)) {
    foreach ($official in $categoryRecord.surfaces) {
        $key = "$($categoryRecord.category)|$(ConvertTo-NormalizedIdentity ([string]$official.name))"
        [object[]]$candidates = if ($owners.ContainsKey($key)) { @($owners[$key]) } else { @() }
        if ($candidates.Count -gt 1) { throw "Ambiguous catalog identity for official $($categoryRecord.category) '$($official.name)'" }
        $owner = if ($candidates.Count -eq 1) { $candidates[0] } else { $null }
        if ($null -ne $owner) {
            $sections = @($owner.rulesReferences | ForEach-Object { [string]$_.section })
            if ($sections -notcontains [string]$official.section) {
                throw "Section mismatch for official $($categoryRecord.category) '$($official.name)': snapshot=$($official.section), catalog=$($sections -join ',')"
            }
        }
        $documentaryUnsupported = $null -ne $owner -and
            $owner.support.lexical -eq "unsupported" -and
            $owner.support.parser -eq "unsupported" -and
            $owner.support.runtime -eq "unsupported" -and
            @($owner.semantic.primitiveDecomposition).Count -eq 0 -and
            @($owner.semantic.intrinsicHooks).Count -eq 0 -and
            $owner.semantic.namedActionPerformed.policy -eq "never"
        $entries.Add([pscustomobject][ordered]@{
            inventoryId = ConvertTo-InventoryId ([string]$official.name)
            name = [string]$official.name
            category = [string]$categoryRecord.category
            rulesSection = [string]$official.section
            catalogStatus = if ($null -eq $owner) { "uncatalogued" } else { "catalogued" }
            catalogIds = if ($null -eq $owner) { @() } else { @([string]$owner.canonicalId) }
            integrationStatus = if ($null -eq $owner) { "uncatalogued" } elseif ($documentaryUnsupported) { "documentaryUnsupported" } else { "enriched" }
            support = if ($null -eq $owner) { $null } else { $owner.support }
            localFrequency = New-Frequency ([string]$official.name) $fixtureCorpus
        })
    }
}

foreach ($officialWord in @($snapshot.abilityWords)) {
    $key = "abilityWord|$(ConvertTo-NormalizedIdentity ([string]$officialWord))"
    [object[]]$candidates = if ($owners.ContainsKey($key)) { @($owners[$key]) } else { @() }
    if ($candidates.Count -gt 1) { throw "Ambiguous catalog identity for official ability word '$officialWord'" }
    $owner = if ($candidates.Count -eq 1) { $candidates[0] } else { $null }
    $entries.Add([pscustomobject][ordered]@{
        inventoryId = ConvertTo-InventoryId ([string]$officialWord)
        name = [string]$officialWord
        category = "abilityWord"
        rulesSection = "207.2c"
        catalogStatus = if ($null -eq $owner) { "uncatalogued" } else { "catalogued" }
        catalogIds = if ($null -eq $owner) { @() } else { @([string]$owner.canonicalId) }
        integrationStatus = "markerOnly"
        support = if ($null -eq $owner) { $null } else { $owner.support }
        localFrequency = New-Frequency ([string]$officialWord) $fixtureCorpus
    })
}

$supplementalRulesSurfaces = @(
    $catalog.entries |
        Where-Object { $_.category -in @("marker", "staticOrCostMechanic") } |
        Sort-Object { [string]$_.canonicalId } |
        ForEach-Object {
            $entry = $_
            $documentaryUnsupported = $entry.support.lexical -eq "unsupported" -and
                $entry.support.parser -eq "unsupported" -and
                $entry.support.runtime -eq "unsupported" -and
                @($entry.semantic.primitiveDecomposition).Count -eq 0 -and
                @($entry.semantic.intrinsicHooks).Count -eq 0 -and
                $entry.semantic.namedActionPerformed.policy -eq "never"
            [pscustomobject][ordered]@{
                inventoryId = ConvertTo-InventoryId ([string]$entry.name)
                name = [string]$entry.name
                category = [string]$entry.category
                rulesSections = @($entry.rulesReferences | ForEach-Object { [string]$_.section })
                catalogId = [string]$entry.canonicalId
                integrationStatus = if ($documentaryUnsupported) { "documentaryUnsupported" } else { "enriched" }
                support = $entry.support
                localFrequency = New-Frequency ([string]$entry.name) $fixtureCorpus
            }
        }
)
$supplementalIntegrationQueue = [System.Collections.Generic.List[object]]::new()
$queuedSupplementalSurfaces = @(
    $supplementalRulesSurfaces |
        Where-Object integrationStatus -eq "documentaryUnsupported" |
        Sort-Object @{ Expression = { $_.localFrequency.fixtureOracleMentions }; Descending = $true }, category, catalogId
)
for ($index = 0; $index -lt $queuedSupplementalSurfaces.Count; $index += 1) {
    $entry = $queuedSupplementalSurfaces[$index]
    $supplementalIntegrationQueue.Add([pscustomobject][ordered]@{
        priority = $index + 1
        inventoryId = $entry.inventoryId
        name = $entry.name
        category = $entry.category
        rulesSections = @($entry.rulesSections)
        catalogId = $entry.catalogId
        fixtureOracleMentions = $entry.localFrequency.fixtureOracleMentions
        reason = "supplemental rules documentation exists; lexical recognition, parser lowering, contract, and runtime behavior remain unproved"
    })
}

$allEntries = @($entries)
$uncatalogued = @($allEntries | Where-Object catalogStatus -eq "uncatalogued")
$catalogued = @($allEntries | Where-Object catalogStatus -eq "catalogued")
$keywordEntries = @($allEntries | Where-Object category -in @("keywordAction", "keywordAbility"))
$documentaryQueue = @($keywordEntries | Where-Object integrationStatus -eq "documentaryUnsupported" | Sort-Object @{ Expression = { $_.localFrequency.fixtureOracleMentions }; Descending = $true }, category, rulesSection)
$integrationQueue = [System.Collections.Generic.List[object]]::new()
for ($index = 0; $index -lt $documentaryQueue.Count; $index += 1) {
    $entry = $documentaryQueue[$index]
    $integrationQueue.Add([pscustomobject][ordered]@{
        priority = $index + 1
        inventoryId = $entry.inventoryId
        name = $entry.name
        category = $entry.category
        rulesSection = $entry.rulesSection
        catalogId = @($entry.catalogIds)[0]
        fixtureOracleMentions = $entry.localFrequency.fixtureOracleMentions
        reason = "documentation exists; parser, contract, primitive IR, and runtime behavior remain unproved"
    })
}

$inventory = [ordered]@{
    inventorySchemaVersion = "deepdeck-ability-inventory/v2"
    inventoryVersion = [string]$catalog.catalogVersion
    generatedAgainstCatalogVersion = [string]$catalog.catalogVersion
    rulesEffectiveDate = [string]$snapshot.source.effectiveDate
    authoritativeSnapshot = [ordered]@{
        path = "data/ability-catalog/official-ability-snapshot-v2.json"
        sourceUrl = [string]$snapshot.source.url
        sha256 = [string]$snapshot.source.sha256
    }
    sources = @(
        [ordered]@{ kind = "officialRulesSnapshot"; path = "data/ability-catalog/official-ability-snapshot-v2.json" },
        [ordered]@{ kind = "abilityCatalog"; path = "data/ability-catalog/ability-catalog-v1.json" },
        [ordered]@{ kind = "localOracleFrequency"; path = "fixtures/**/*.json"; fileCount = $fixtureFiles.Count }
    )
    coverage = [ordered]@{
        authoritativeTotal = $allEntries.Count
        catalogued = $catalogued.Count
        uncatalogued = $uncatalogued.Count
        catalogCoveragePercent = [math]::Round(100 * $catalogued.Count / $allEntries.Count, 2)
        supplementalRulesSurfaceTotal = $supplementalRulesSurfaces.Count
        byCategory = [ordered]@{
            keywordAction = [ordered]@{ total = @($allEntries | Where-Object category -eq "keywordAction").Count; missing = @($uncatalogued | Where-Object category -eq "keywordAction").Count }
            keywordAbility = [ordered]@{ total = @($allEntries | Where-Object category -eq "keywordAbility").Count; missing = @($uncatalogued | Where-Object category -eq "keywordAbility").Count }
            abilityWord = [ordered]@{ total = @($allEntries | Where-Object category -eq "abilityWord").Count; missing = @($uncatalogued | Where-Object category -eq "abilityWord").Count }
        }
    }
    integration = [ordered]@{
        keywordSurfaceTotal = $keywordEntries.Count
        enriched = @($keywordEntries | Where-Object integrationStatus -eq "enriched").Count
        documentaryUnsupported = $documentaryQueue.Count
        supplementalRulesSurfaceTotal = $supplementalRulesSurfaces.Count
        supplementalDocumentaryUnsupported = $queuedSupplementalSurfaces.Count
    }
    missingQueue = @($uncatalogued | Select-Object inventoryId, name, category, rulesSection)
    integrationQueue = @($integrationQueue)
    supplementalIntegrationQueue = @($supplementalIntegrationQueue)
    supplementalRulesSurfaces = @($supplementalRulesSurfaces)
    entries = $allEntries
}

$json = $inventory | ConvertTo-Json -Depth 30
if (-not [string]::IsNullOrWhiteSpace($EmitInventoryPath)) {
    $resolvedOutput = Resolve-RepositoryPath $EmitInventoryPath
    $parent = Split-Path -Parent $resolvedOutput
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) { $null = New-Item -ItemType Directory -Path $parent -Force }
    [IO.File]::WriteAllText($resolvedOutput, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    Write-Host "Generated ability inventory: $resolvedOutput"
}
if ($PrintInventory) { Write-Output $json }
else { Write-Host "Ability inventory: $($allEntries.Count) official surfaces, $($catalogued.Count) catalogued, $($documentaryQueue.Count) documentary keyword surfaces awaiting integration." }
