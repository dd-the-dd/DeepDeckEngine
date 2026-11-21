[CmdletBinding()]
param(
    [string]$CatalogPath = "data/ability-catalog/ability-catalog-v1.json",
    [string]$SnapshotPath = "data/ability-catalog/official-ability-snapshot-v2.json",
    [string]$CatalogVersion = "2026.11.10-c13"
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

function ConvertTo-CanonicalId {
    param([Parameter(Mandatory = $true)][string]$OfficialName)
    $source = $OfficialName
    if ($source -match '^\P{IsBasicLatin}+\s*\(([^)]+)\)$') { $source = [string]$Matches[1] }
    $source = $source.Normalize([Text.NormalizationForm]::FormD).Replace("'", "").Replace([string][char]0x2019, "")
    $builder = [Text.StringBuilder]::new()
    foreach ($character in $source.ToCharArray()) {
        $category = [Globalization.CharUnicodeInfo]::GetUnicodeCategory($character)
        if ($category -eq [Globalization.UnicodeCategory]::NonSpacingMark) { continue }
        $null = $builder.Append($character)
    }
    $ascii = $builder.ToString().ToLowerInvariant()
    $id = (($ascii -replace '[^a-z0-9]+', '-').Trim('-'))
    if ($id -notmatch '^[a-z]') { throw "Cannot derive canonical identity from official name '$OfficialName'" }
    return $id
}

function ConvertTo-GrammarId {
    param([Parameter(Mandatory = $true)][string]$CanonicalId)
    $parts = @($CanonicalId.Split('-') | Where-Object { $_ })
    $tail = @($parts | Select-Object -Skip 1 | ForEach-Object {
        if ($_.Length -eq 1) { $_.ToUpperInvariant() }
        else { $_.Substring(0, 1).ToUpperInvariant() + $_.Substring(1) }
    }) -join ""
    return $parts[0] + $tail
}

function New-RulesText {
    param($Official, [Parameter(Mandatory = $true)][string]$CanonicalId, [Parameter(Mandatory = $true)][string]$Version)
    return [ordered]@{
        version = $Version
        clauses = @($Official.clauses | ForEach-Object {
            $suffix = ([string]$_.section -replace '[^a-zA-Z0-9]+', '-').Trim('-').ToLowerInvariant()
            [ordered]@{
                clauseId = "$CanonicalId.cr-$suffix"
                text = [string]$_.text
                sourceSection = [string]$_.section
                usage = "documentationOnly"
            }
        })
    }
}

function New-OfficialReference {
    param($Official, [Parameter(Mandatory = $true)][string]$Version)
    return [ordered]@{
        document = "ComprehensiveRules"
        version = $Version
        section = [string]$Official.section
        sourceUrl = "https://magic.wizards.com/en/rules"
    }
}

function Test-DocumentaryUnsupportedEntry {
    param($Entry)
    return @($Entry.representativeCards).Count -eq 0 -and
        @($Entry.semantic.primitiveDecomposition).Count -eq 0 -and
        @($Entry.semantic.intrinsicHooks).Count -eq 0 -and
        $Entry.semantic.namedActionPerformed.policy -eq "never" -and
        $Entry.support.lexical -eq "unsupported" -and
        $Entry.support.parser -eq "unsupported" -and
        $Entry.support.runtime -eq "unsupported"
}

function New-DocumentaryEntry {
    param($Official, [Parameter(Mandatory = $true)][string]$Category, [Parameter(Mandatory = $true)][string]$Version)
    $canonicalId = ConvertTo-CanonicalId ([string]$Official.name)
    $grammarId = ConvertTo-GrammarId $canonicalId
    $grammarKind = if ($Category -eq "keywordAction") { "KEYWORD_ACTION" } else { "KEYWORD_ABILITY" }
    $templateKind = if ($Category -eq "keywordAction") { "keyword-action" } else { "keyword-ability" }
    return [pscustomobject][ordered]@{
        canonicalId = $canonicalId
        name = [string]$Official.name
        aliases = @()
        category = $Category
        classificationNotes = "Official $Category surface synchronized for documentation and future integration. No parser or runtime behavior is implied."
        rulesReferences = @(New-OfficialReference $Official $Version)
        rulesText = New-RulesText $Official $canonicalId $Version
        grammarTemplates = @([ordered]@{
            templateId = "$canonicalId.$templateKind"
            tokenGrammar = "$grammarKind($grammarId)"
            bindings = @()
            reminderTextPolicy = "none"
        })
        reminderGrammarTemplates = @()
        semantic = [ordered]@{
            primitiveDecomposition = @()
            namedActionPerformed = [ordered]@{
                policy = "never"
                cardinality = "none"
                zeroQuantity = "notApplicable"
                resultBindings = @()
            }
            intrinsicHooks = @()
        }
        support = [ordered]@{
            lexical = "unsupported"
            parser = "unsupported"
            runtime = "unsupported"
            notes = "Documentary registry entry only. Token recognition, primitive lowering, contract executability, and runtime behavior require independent proof."
        }
        representativeCards = @()
        provenance = @([ordered]@{
            kind = "officialRules"
            locator = "CR $($Official.section) via the checked-in official ability snapshot v2"
            asOf = $Version
        })
    }
}

$resolvedCatalog = Resolve-RepositoryPath $CatalogPath
$catalog = Get-Content -LiteralPath $resolvedCatalog -Raw -Encoding UTF8 | ConvertFrom-Json
$snapshot = Get-Content -LiteralPath (Resolve-RepositoryPath $SnapshotPath) -Raw -Encoding UTF8 | ConvertFrom-Json
if ($snapshot.snapshotSchemaVersion -ne "deepdeck-official-ability-snapshot/v2") { throw "Official keyword synchronization requires snapshot v2" }
$version = [string]$snapshot.source.effectiveDate
if ($catalog.rulesVersion.effectiveDate -ne $version) { throw "Catalog and snapshot rules dates differ" }

$officialSurfaces = @(
    @($snapshot.keywordActions | ForEach-Object { [pscustomobject]@{ category = "keywordAction"; surface = $_ } }) +
    @($snapshot.keywordAbilities | ForEach-Object { [pscustomobject]@{ category = "keywordAbility"; surface = $_ } })
)
$officialByIdentity = @{}
foreach ($record in $officialSurfaces) {
    $key = "$($record.category)|$(ConvertTo-NormalizedIdentity ([string]$record.surface.name))"
    if ($officialByIdentity.ContainsKey($key)) { throw "Duplicate official keyword identity '$key'" }
    $officialByIdentity[$key] = $record
}

$matchedOfficial = @{}
$resultEntries = [System.Collections.Generic.List[object]]::new()
$realignments = [System.Collections.Generic.List[string]]::new()
foreach ($entry in @($catalog.entries)) {
    if ($entry.category -notin @("keywordAction", "keywordAbility")) {
        $resultEntries.Add($entry)
        continue
    }
    $matches = [System.Collections.Generic.List[object]]::new()
    foreach ($lexeme in @([string]$entry.name) + @($entry.aliases | ForEach-Object { [string]$_ })) {
        $key = "$($entry.category)|$(ConvertTo-NormalizedIdentity $lexeme)"
        if ($officialByIdentity.ContainsKey($key) -and -not @($matches | Where-Object { $_.surface.section -eq $officialByIdentity[$key].surface.section }).Count) {
            $matches.Add($officialByIdentity[$key])
        }
    }
    if ($matches.Count -gt 1) { throw "Catalog entry '$($entry.canonicalId)' maps to multiple official keyword identities" }
    if ($matches.Count -eq 0) {
        # Parameterized variants such as Forestwalk and Typecycling remain
        # separate enriched entries and do not claim the official umbrella row.
        $resultEntries.Add($entry)
        continue
    }

    $record = $matches[0]
    $official = $record.surface
    $officialKey = "$($record.category)|$(ConvertTo-NormalizedIdentity ([string]$official.name))"
    if ($matchedOfficial.ContainsKey($officialKey)) { throw "Multiple catalog entries claim official identity '$officialKey'" }
    $matchedOfficial[$officialKey] = [string]$entry.canonicalId

    $oldKeywordSections = @($entry.rulesReferences | Where-Object { $_.section -match '^(701|702)\.' } | ForEach-Object { [string]$_.section })
    if ($oldKeywordSections.Count -gt 0 -and $oldKeywordSections -notcontains [string]$official.section) {
        $realignments.Add("$($entry.canonicalId):$($oldKeywordSections -join ',')->$($official.section)")
    }

    if (Test-DocumentaryUnsupportedEntry $entry) {
        $replacement = New-DocumentaryEntry $official $record.category $version
        $replacement.canonicalId = [string]$entry.canonicalId
        $resultEntries.Add($replacement)
        continue
    }

    $nonKeywordReferences = @($entry.rulesReferences | Where-Object { $_.section -notmatch '^(701|702)\.' })
    $entry.name = [string]$official.name
    $entry.rulesReferences = @(New-OfficialReference $official $version) + $nonKeywordReferences
    $entry.rulesText = New-RulesText $official ([string]$entry.canonicalId) $version
    $resultEntries.Add($entry)
}

$added = 0
foreach ($record in $officialSurfaces) {
    $officialKey = "$($record.category)|$(ConvertTo-NormalizedIdentity ([string]$record.surface.name))"
    if ($matchedOfficial.ContainsKey($officialKey)) { continue }
    $candidate = New-DocumentaryEntry $record.surface $record.category $version
    if (@($resultEntries | Where-Object { $_.canonicalId -eq $candidate.canonicalId }).Count -gt 0) {
        throw "Generated canonical identity '$($candidate.canonicalId)' collides with a retained catalog entry"
    }
    $resultEntries.Add($candidate)
    $matchedOfficial[$officialKey] = [string]$candidate.canonicalId
    $added += 1
}

if ($matchedOfficial.Count -ne 266) { throw "Expected 266 mapped official keyword surfaces; got $($matchedOfficial.Count)" }
$catalog.catalogVersion = $CatalogVersion
$catalog.entries = @($resultEntries)
[IO.File]::WriteAllText($resolvedCatalog, (($catalog | ConvertTo-Json -Depth 40) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
Write-Host "Synchronized 266 official keyword surfaces: $added documentary entries added, $($realignments.Count) enriched entries realigned, $($catalog.entries.Count) total catalog entries."
if ($realignments.Count -gt 0) { Write-Host "Realignments: $($realignments -join '; ')" }
