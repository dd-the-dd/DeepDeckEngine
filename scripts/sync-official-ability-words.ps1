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

function ConvertTo-CanonicalId {
    param([Parameter(Mandatory = $true)][string]$AbilityWord)
    $withoutApostrophes = $AbilityWord.Trim().ToLowerInvariant().Replace("'", "").Replace([string][char]0x2019, "")
    return (($withoutApostrophes -replace '[^a-z0-9]+', '-').Trim('-'))
}

function ConvertTo-GrammarId {
    param([Parameter(Mandatory = $true)][string]$CanonicalId)
    $parts = @($CanonicalId.Split('-') | Where-Object { $_ })
    if ($parts.Count -eq 0) { throw "Cannot derive grammar identity from '$CanonicalId'" }
    $tail = @($parts | Select-Object -Skip 1 | ForEach-Object {
        if ($_.Length -eq 1) { $_.ToUpperInvariant() }
        else { $_.Substring(0, 1).ToUpperInvariant() + $_.Substring(1) }
    }) -join ""
    return $parts[0] + $tail
}

$resolvedCatalog = Resolve-RepositoryPath $CatalogPath
$resolvedSnapshot = Resolve-RepositoryPath $SnapshotPath
$catalog = Get-Content -LiteralPath $resolvedCatalog -Raw -Encoding UTF8 | ConvertFrom-Json
$snapshot = Get-Content -LiteralPath $resolvedSnapshot -Raw -Encoding UTF8 | ConvertFrom-Json
if ($snapshot.snapshotSchemaVersion -notin @("deepdeck-official-ability-snapshot/v1", "deepdeck-official-ability-snapshot/v2")) {
    throw "Unsupported official ability snapshot schema: $($snapshot.snapshotSchemaVersion)"
}
if ($snapshot.source.effectiveDate -ne $catalog.rulesVersion.effectiveDate) {
    throw "Catalog and official snapshot effective dates differ"
}

# These three entries predate the generated official inventory and retain their
# richer local evidence. Every other CR 207.2c word is rebuilt from the snapshot.
$preservedCanonicalIds = @("channel", "renewal", "revolt")
$preservedOfficialWords = @{}
foreach ($entry in @($catalog.entries | Where-Object { $preservedCanonicalIds -contains [string]$_.canonicalId })) {
    foreach ($lexeme in @([string]$entry.name) + @($entry.aliases | ForEach-Object { [string]$_ })) {
        $preservedOfficialWords[$lexeme.Trim().ToLowerInvariant()] = [string]$entry.canonicalId
    }
}

$generatedWords = @($snapshot.abilityWords | Where-Object {
    -not $preservedOfficialWords.ContainsKey(([string]$_).Trim().ToLowerInvariant())
})
if ($generatedWords.Count -ne 63) {
    throw "Expected 63 generated ability words after preserving Channel, Renew, and Revolt; got $($generatedWords.Count)"
}

$generatedEntries = @($generatedWords | ForEach-Object {
    $officialWord = [string]$_
    $canonicalId = ConvertTo-CanonicalId $officialWord
    $grammarId = ConvertTo-GrammarId $canonicalId
    [ordered]@{
        canonicalId = $canonicalId
        name = $officialWord
        aliases = @()
        category = "abilityWord"
        classificationNotes = "Official CR 207.2c ability-word marker. It labels, but does not define, the following ability."
        rulesReferences = @([ordered]@{
            document = "ComprehensiveRules"
            version = [string]$snapshot.source.effectiveDate
            section = "207.2c"
            sourceUrl = "https://magic.wizards.com/en/rules"
        })
        rulesText = [ordered]@{
            version = [string]$snapshot.source.effectiveDate
            clauses = @([ordered]@{
                clauseId = "$canonicalId.label"
                text = "The ability word '$officialWord' has no intrinsic rules meaning; the ability text after the em dash defines the complete behavior."
                sourceSection = "207.2c"
                usage = "documentationOnly"
            })
        }
        grammarTemplates = @([ordered]@{
            templateId = "$canonicalId.ability-word"
            tokenGrammar = "ABILITY_WORD($grammarId) EM_DASH"
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
            lexical = "supportedNative"
            parser = "unsupported"
            runtime = "unsupported"
            notes = "The generic tokenizer recognizes this exact official label only at ability start immediately before an em dash. The following Oracle text must be parsed independently; this marker supplies no parser or runtime behavior."
        }
        representativeCards = @()
        provenance = @(
            [ordered]@{
                kind = "officialRules"
                locator = "CR 207.2c via https://magic.wizards.com/en/rules"
                asOf = [string]$snapshot.source.effectiveDate
            },
            [ordered]@{
                kind = "localOracleCorpus"
                locator = "src/oracle/syntax/tokenizer.rs generic ability-word start-plus-em-dash recognition"
                asOf = [string]$snapshot.source.effectiveDate
                notes = "Lexical implementation evidence only; no following ability text is claimed supported."
            }
        )
    }
})

$generatedIds = @{} 
foreach ($entry in $generatedEntries) {
    if ($generatedIds.ContainsKey([string]$entry.canonicalId)) {
        throw "Generated canonical identity collision: $($entry.canonicalId)"
    }
    $generatedIds[[string]$entry.canonicalId] = $true
}
$retainedEntries = @($catalog.entries | Where-Object {
    $id = [string]$_.canonicalId
    $isGeneratedAbilityWord = $_.category -eq "abilityWord" -and
        $preservedCanonicalIds -notcontains $id -and
        @($_.rulesReferences | Where-Object { $_.section -eq "207.2c" }).Count -gt 0
    -not $isGeneratedAbilityWord
})
foreach ($entry in $retainedEntries) {
    if ($generatedIds.ContainsKey([string]$entry.canonicalId)) {
        throw "Generated ability word collides with retained entry '$($entry.canonicalId)'"
    }
}

$catalog.catalogVersion = $CatalogVersion
$catalog.entries = @($retainedEntries) + @($generatedEntries)
[IO.File]::WriteAllText(
    $resolvedCatalog,
    (($catalog | ConvertTo-Json -Depth 32) + [Environment]::NewLine),
    [Text.UTF8Encoding]::new($false)
)
Write-Host "Synchronized 63 official ability words; catalog now contains $($catalog.entries.Count) entries at $CatalogVersion."
