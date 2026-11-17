[CmdletBinding()]
param(
    [string]$CatalogPath = "data/ability-catalog/ability-catalog-v1.json",
    [string]$LexiconOutputPath = "crates/deepdeck-oracle/generated/ability-lexicon-index-v2.json",
    [string]$RegistryOutputPath = "crates/deepdeck-oracle/generated/ability-registry-index-v2.json",
    [string]$DocumentationOutputPath = "crates/deepdeck-oracle/generated/ability-documentation-index-v2.json"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Resolve-RepositoryPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

function Write-DeterministicJson {
    param($Value, [Parameter(Mandatory = $true)][string]$Path)
    $resolved = Resolve-RepositoryPath $Path
    $parent = Split-Path -Parent $resolved
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) { $null = New-Item -ItemType Directory -Path $parent -Force }
    [IO.File]::WriteAllText($resolved, (($Value | ConvertTo-Json -Depth 30) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
}

$resolvedCatalog = Resolve-RepositoryPath $CatalogPath
& (Join-Path $PSScriptRoot "validate-ability-catalog.ps1") -CatalogPath $resolvedCatalog
$catalog = Get-Content -LiteralPath $resolvedCatalog -Raw -Encoding UTF8 | ConvertFrom-Json
$sortedEntries = @($catalog.entries | Sort-Object { [string]$_.canonicalId })
$runtimeEntries = @($sortedEntries | Where-Object {
    $_.category -ne "marker" -and
    -not ($_.support.lexical -eq "unsupported" -and $_.support.parser -eq "unsupported" -and $_.support.runtime -eq "unsupported")
})

$lexicon = [ordered]@{
    lexiconSchemaVersion = "deepdeck-ability-lexicon/v2"
    catalogVersion = [string]$catalog.catalogVersion
    rulesEffectiveDate = [string]$catalog.rulesVersion.effectiveDate
    entries = @($runtimeEntries | ForEach-Object {
        [ordered]@{
            canonicalId = [string]$_.canonicalId
            category = [string]$_.category
            lexemes = @([string]$_.name) + @($_.aliases | ForEach-Object { [string]$_ })
        }
    })
}

$registry = [ordered]@{
    registrySchemaVersion = "deepdeck-ability-registry/v2"
    catalogVersion = [string]$catalog.catalogVersion
    rulesEffectiveDate = [string]$catalog.rulesVersion.effectiveDate
    entries = @($runtimeEntries | ForEach-Object {
        $entry = $_
        $semanticKind = if ($entry.category -eq "abilityWord" -or $entry.category -eq "marker") {
            "markerOnly"
        } elseif (@($entry.semantic.primitiveDecomposition).Count -gt 0) {
            "primitiveExpansion"
        } else {
            "intrinsicRule"
        }
        $registryEntry = [ordered]@{
            canonicalId = [string]$entry.canonicalId
            category = [string]$entry.category
            rulesReferences = @($entry.rulesReferences)
            rulesText = $entry.rulesText
            grammar = @(
                @($entry.grammarTemplates | ForEach-Object { [ordered]@{ templateId = [string]$_.templateId; role = "primary" } }) +
                @($entry.reminderGrammarTemplates | ForEach-Object { [ordered]@{ templateId = [string]$_.templateId; role = "reminder" } })
            )
            semantics = [ordered]@{
                kind = $semanticKind
                primitiveSteps = @($entry.semantic.primitiveDecomposition)
                intrinsicHooks = @($entry.semantic.intrinsicHooks)
                signal = $entry.semantic.namedActionPerformed
            }
            support = $entry.support
        }
        if ($null -ne $entry.compiledIntegration) {
            $registryEntry.compiledIntegration = $entry.compiledIntegration
        }
        $registryEntry
    })
}

$documentation = [ordered]@{
    documentationSchemaVersion = "deepdeck-ability-documentation/v2"
    catalogVersion = [string]$catalog.catalogVersion
    rulesEffectiveDate = [string]$catalog.rulesVersion.effectiveDate
    entries = @($sortedEntries | ForEach-Object {
        [ordered]@{
            canonicalId = [string]$_.canonicalId
            name = [string]$_.name
            aliases = @($_.aliases)
            category = [string]$_.category
            rulesReferences = @($_.rulesReferences)
            rulesText = $_.rulesText
            grammarTemplateIds = @(
                @($_.grammarTemplates | ForEach-Object { [string]$_.templateId }) +
                @($_.reminderGrammarTemplates | ForEach-Object { [string]$_.templateId })
            )
            support = $_.support
        }
    })
}

Write-DeterministicJson $lexicon $LexiconOutputPath
Write-DeterministicJson $registry $RegistryOutputPath
Write-DeterministicJson $documentation $DocumentationOutputPath
Write-Host "Generated ability indexes: $($lexicon.entries.Count) runtime lexemes, $($registry.entries.Count) semantic entries, and $($documentation.entries.Count) documentary entries."
