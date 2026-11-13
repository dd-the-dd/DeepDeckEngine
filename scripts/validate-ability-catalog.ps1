[CmdletBinding()]
param(
    [string]$CatalogPath = "data/ability-catalog/ability-catalog-v1.json",
    [string]$SchemaPath = "data/ability-catalog/ability-catalog-v1.schema.json",
    [string]$EmitIndexPath,
    [switch]$PrintGeneratedIndex
)

$ErrorActionPreference = "Stop"

function Resolve-RepositoryPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([IO.Path]::IsPathRooted($Path)) {
        return [IO.Path]::GetFullPath($Path)
    }
    return [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../$Path"))
}

function Add-ValidationError {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][System.Collections.Generic.List[string]]$Errors,
        [Parameter(Mandatory = $true)][string]$Message
    )

    $Errors.Add($Message)
}

function Normalize-Lexeme {
    param([Parameter(Mandatory = $true)][string]$Value)

    return $Value.Trim().ToLowerInvariant()
}

function Normalize-AbilityIdentity {
    param([Parameter(Mandatory = $true)][string]$Value)

    return ((Normalize-Lexeme $Value) -replace '[^a-z0-9]+', '-').Trim('-')
}

$resolvedCatalogPath = Resolve-RepositoryPath $CatalogPath
$resolvedSchemaPath = Resolve-RepositoryPath $SchemaPath
if (-not (Test-Path -LiteralPath $resolvedCatalogPath -PathType Leaf)) {
    throw "Ability catalog not found: $resolvedCatalogPath"
}
if (-not (Test-Path -LiteralPath $resolvedSchemaPath -PathType Leaf)) {
    throw "Ability catalog schema not found: $resolvedSchemaPath"
}

# Parsing the schema is intentional even on Windows PowerShell, where Test-Json
# is unavailable. Structural constraints that matter to generation are checked
# below so the validator behaves identically in CI and developer shells.
$null = Get-Content -LiteralPath $resolvedSchemaPath -Raw -Encoding UTF8 | ConvertFrom-Json
$catalog = Get-Content -LiteralPath $resolvedCatalogPath -Raw -Encoding UTF8 | ConvertFrom-Json
$validationErrors = [System.Collections.Generic.List[string]]::new()

if ($catalog.catalogSchemaVersion -ne "deepdeck-ability-catalog/v1") {
    Add-ValidationError $validationErrors "catalogSchemaVersion must be deepdeck-ability-catalog/v1"
}
if (@($catalog.entries).Count -eq 0) {
    Add-ValidationError $validationErrors "catalog must contain at least one entry"
}

$canonicalIds = @{}
$lexemeOwners = @{}
$identityOwners = @{}
$templateOwners = @{}
$allowedCategories = @("keywordAbility", "keywordAction", "abilityWord", "marker", "staticOrCostMechanic")
$allowedSupport = @("supportedNative", "supportedLegacy", "partial", "unsupported")
$allowedSignalPolicies = @("never", "afterActionCompletes")
$compiledIntegrationOwners = @{}
$compiledIntegrationContracts = @{
    "keyword.mobilize.v1" = [pscustomobject]@{
        canonicalId = "mobilize"
        loweringAdapterId = "lower.keyword.mobilize.v1"
        bindings = @(
            "quantity:positiveInteger:recognizerOutput",
            "mobilizeTokens:objectSet:loweringOutput"
        )
        emittedPrimitives = @(
            "createTokens",
            "installDelayedStepTrigger",
            "sacrificePermanents"
        )
    }
}

foreach ($entry in @($catalog.entries)) {
    $id = [string]$entry.canonicalId
    if ($id -notmatch '^[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*$') {
        Add-ValidationError $validationErrors "invalid canonicalId '$id'"
    }
    if ($canonicalIds.ContainsKey($id)) {
        Add-ValidationError $validationErrors "duplicate canonicalId '$id'"
    } else {
        $canonicalIds[$id] = $true
    }
    $normalizedId = Normalize-AbilityIdentity $id
    if ($identityOwners.ContainsKey($normalizedId) -and $identityOwners[$normalizedId] -ne $id) {
        Add-ValidationError $validationErrors "ability identity collision '$id' with '$($identityOwners[$normalizedId])'"
    } else {
        $identityOwners[$normalizedId] = $id
    }

    if ($allowedCategories -notcontains [string]$entry.category) {
        Add-ValidationError $validationErrors "entry '$id' has invalid category '$($entry.category)'"
    }

    foreach ($lexeme in @([string]$entry.name) + @($entry.aliases | ForEach-Object { [string]$_ })) {
        if ([string]::IsNullOrWhiteSpace($lexeme)) {
            Add-ValidationError $validationErrors "entry '$id' contains an empty name or alias"
            continue
        }
        $normalized = Normalize-Lexeme $lexeme
        if ($lexemeOwners.ContainsKey($normalized)) {
            if ($lexemeOwners[$normalized] -ne $id) {
                Add-ValidationError $validationErrors "lexeme collision '$lexeme' between '$id' and '$($lexemeOwners[$normalized])'"
            } else {
                Add-ValidationError $validationErrors "duplicate normalized lexeme '$lexeme' in '$id'"
            }
        } else {
            $lexemeOwners[$normalized] = $id
        }
        $normalizedIdentity = Normalize-AbilityIdentity $lexeme
        if ($identityOwners.ContainsKey($normalizedIdentity) -and $identityOwners[$normalizedIdentity] -ne $id) {
            Add-ValidationError $validationErrors "ability identity collision '$lexeme' between '$id' and '$($identityOwners[$normalizedIdentity])'"
        } else {
            $identityOwners[$normalizedIdentity] = $id
        }
    }

    if (@($entry.rulesReferences).Count -eq 0) {
        Add-ValidationError $validationErrors "entry '$id' has no rules reference"
    }
    foreach ($reference in @($entry.rulesReferences)) {
        if ([string]$reference.section -notmatch '^\d+(?:\.\d+)+(?:[a-z])?$') {
            Add-ValidationError $validationErrors "entry '$id' has invalid rules section '$($reference.section)'"
        }
        if ([string]$reference.version -notmatch '^\d{4}-\d{2}-\d{2}$') {
            Add-ValidationError $validationErrors "entry '$id' has invalid rules version '$($reference.version)'"
        }
        if ([string]$reference.sourceUrl -notmatch '^https://(?:www\.)?magic\.wizards\.com/') {
            Add-ValidationError $validationErrors "entry '$id' rules source must be an official magic.wizards.com URL"
        }
    }

    $referenceSections = @($entry.rulesReferences | ForEach-Object { [string]$_.section })
    if ([string]$entry.rulesText.version -notmatch '^\d{4}-\d{2}-\d{2}$') {
        Add-ValidationError $validationErrors "entry '$id' has invalid rulesText version '$($entry.rulesText.version)'"
    }
    if (@($entry.rulesText.clauses).Count -eq 0) {
        Add-ValidationError $validationErrors "entry '$id' has no normalized rulesText clauses"
    }
    foreach ($clause in @($entry.rulesText.clauses)) {
        if ([string]::IsNullOrWhiteSpace([string]$clause.text)) {
            Add-ValidationError $validationErrors "entry '$id' has an empty rulesText clause '$($clause.clauseId)'"
        }
        if ($clause.usage -ne "documentationOnly") {
            Add-ValidationError $validationErrors "entry '$id' rulesText clause '$($clause.clauseId)' must be documentationOnly"
        }
        $sourceSection = [string]$clause.sourceSection
        $sourceFound = @($referenceSections | Where-Object {
            $sourceSection -eq $_ -or $sourceSection -match ('^' + [regex]::Escape($_) + '(?:[a-z]|\.)')
        }).Count -gt 0
        if (-not $sourceFound) {
            Add-ValidationError $validationErrors "entry '$id' rulesText sourceSection '$sourceSection' is absent from rulesReferences"
        }
    }

    if (@($entry.grammarTemplates).Count -eq 0) {
        Add-ValidationError $validationErrors "entry '$id' has no token grammar template"
    }
    $grammarAtom = '[A-Z][A-Z0-9_]*(?:\([A-Za-z][A-Za-z0-9]*\))?'
    foreach ($template in @($entry.grammarTemplates)) {
        $templateId = [string]$template.templateId
        if ($templateOwners.ContainsKey($templateId)) {
            Add-ValidationError $validationErrors "duplicate templateId '$templateId' in '$id' and '$($templateOwners[$templateId])'"
        } else {
            $templateOwners[$templateId] = $id
        }
        $grammar = [string]$template.tokenGrammar
        if ([string]::IsNullOrWhiteSpace($grammar)) {
            Add-ValidationError $validationErrors "entry '$id' has an empty token grammar"
        }
        if ($grammar -notmatch "^(?:$grammarAtom|\[$grammarAtom\])(?: (?:$grammarAtom|\[$grammarAtom\]))*$") {
            Add-ValidationError $validationErrors "entry '$id' token grammar must contain only symbolic token atoms and optional symbolic atoms"
        }
    }
    $reminderTemplates = @($entry.reminderGrammarTemplates)
    if (@($entry.grammarTemplates | Where-Object { $_.reminderTextPolicy -ne "none" }).Count -gt 0 -and $reminderTemplates.Count -eq 0) {
        Add-ValidationError $validationErrors "entry '$id' declares reminder text but has no reminderGrammarTemplates"
    }
    foreach ($template in $reminderTemplates) {
        $templateId = [string]$template.templateId
        if ($templateOwners.ContainsKey($templateId)) {
            Add-ValidationError $validationErrors "duplicate templateId '$templateId' in '$id' and '$($templateOwners[$templateId])'"
        } else {
            $templateOwners[$templateId] = $id
        }
        $grammar = [string]$template.tokenGrammar
        if ($grammar -notmatch "^(?:$grammarAtom|\[$grammarAtom\])(?: (?:$grammarAtom|\[$grammarAtom\]))*$") {
            Add-ValidationError $validationErrors "entry '$id' reminder grammar must contain only symbolic token atoms"
        }
    }

    if ($null -ne $entry.compiledIntegration) {
        $integration = $entry.compiledIntegration
        $recognizerId = [string]$integration.recognizerId
        if (-not $compiledIntegrationContracts.ContainsKey($recognizerId)) {
            Add-ValidationError $validationErrors "entry '$id' references unknown compiled recognizerId '$recognizerId'"
        } else {
            $contract = $compiledIntegrationContracts[$recognizerId]
            if ($compiledIntegrationOwners.ContainsKey($recognizerId)) {
                Add-ValidationError $validationErrors "compiled recognizerId '$recognizerId' is shared by '$id' and '$($compiledIntegrationOwners[$recognizerId])'"
            } else {
                $compiledIntegrationOwners[$recognizerId] = $id
            }
            if ($id -ne $contract.canonicalId) {
                Add-ValidationError $validationErrors "compiled recognizerId '$recognizerId' belongs to '$($contract.canonicalId)', not '$id'"
            }
            if ([string]$integration.loweringAdapterId -ne $contract.loweringAdapterId) {
                Add-ValidationError $validationErrors "entry '$id' references unknown loweringAdapterId '$($integration.loweringAdapterId)' for '$recognizerId'"
            }
            $bindings = @($integration.bindings | ForEach-Object { "$($_.name):$($_.type):$($_.source)" })
            if (($bindings -join ",") -ne (@($contract.bindings) -join ",")) {
                Add-ValidationError $validationErrors "entry '$id' compiled bindings do not match the closed '$recognizerId' contract"
            }
            $emittedPrimitives = @($integration.emittedPrimitives | ForEach-Object { [string]$_ })
            if (($emittedPrimitives -join ",") -ne (@($contract.emittedPrimitives) -join ",")) {
                Add-ValidationError $validationErrors "entry '$id' emitted primitives do not match the closed '$recognizerId' contract"
            }
            $documentedPrimitives = @($entry.semantic.primitiveDecomposition | ForEach-Object { [string]$_.primitive })
            if (($emittedPrimitives -join ",") -ne ($documentedPrimitives -join ",")) {
                Add-ValidationError $validationErrors "entry '$id' compiled primitives differ from its documentary primitive decomposition"
            }
        }
        if ($integration.unknownPolicy -ne "structuredFailure") {
            Add-ValidationError $validationErrors "entry '$id' compiled integration must use structuredFailure for UNKNOWN"
        }
        if ($entry.support.parser -ne "supportedNative" -or $entry.support.runtime -ne "supportedNative") {
            Add-ValidationError $validationErrors "entry '$id' compiled integration requires certified native parser and runtime support"
        }
    }

    $steps = @($entry.semantic.primitiveDecomposition)
    $expectedOrder = 1
    foreach ($step in $steps | Sort-Object { [int]$_.order }) {
        if ([int]$step.order -ne $expectedOrder) {
            Add-ValidationError $validationErrors "entry '$id' primitive orders must be contiguous from 1"
            break
        }
        $expectedOrder += 1
        if ([string]$step.primitive -notmatch '^[a-z][A-Za-z0-9]*$') {
            Add-ValidationError $validationErrors "entry '$id' has invalid primitive '$($step.primitive)'"
        }
    }

    $signal = $entry.semantic.namedActionPerformed
    if ($allowedSignalPolicies -notcontains [string]$signal.policy) {
        Add-ValidationError $validationErrors "entry '$id' has invalid signal policy '$($signal.policy)'"
    } elseif ($signal.policy -eq "afterActionCompletes") {
        if ([string]::IsNullOrWhiteSpace([string]$signal.actionId)) {
            Add-ValidationError $validationErrors "entry '$id' completion signal requires actionId"
        }
        if ($signal.cardinality -ne "oncePerAction") {
            Add-ValidationError $validationErrors "entry '$id' completion signal must be oncePerAction"
        }
    } elseif ($null -ne $signal.actionId -or $signal.cardinality -ne "none") {
        Add-ValidationError $validationErrors "entry '$id' with signal policy never cannot expose an actionId/cardinality"
    }

    if ($entry.category -eq "abilityWord") {
        if ($steps.Count -ne 0 -or @($entry.semantic.intrinsicHooks).Count -ne 0 -or $signal.policy -ne "never") {
            Add-ValidationError $validationErrors "ability word '$id' cannot own semantic primitives, hooks, or a performed signal"
        }
    }
    if ($entry.category -eq "marker") {
        $ownsSemantics = $steps.Count -ne 0 -or @($entry.semantic.intrinsicHooks).Count -ne 0
        if ($signal.policy -ne "never") {
            Add-ValidationError $validationErrors "marker '$id' cannot emit a performed signal"
        }
        if ($ownsSemantics -and $id -ne "chapter-symbol") {
            Add-ValidationError $validationErrors "marker '$id' cannot own semantic primitives or hooks"
        }
        $nonNativeMarkerLayers = @($entry.support.lexical, $entry.support.parser, $entry.support.runtime) |
            Where-Object { $_ -ne "supportedNative" }
        if ($ownsSemantics -and @($nonNativeMarkerLayers).Count -gt 0) {
            Add-ValidationError $validationErrors "semantic marker '$id' must be supportedNative across all layers"
        }
    }
    if ($id -eq "teamwork" -and $entry.category -ne "keywordAbility") {
        Add-ValidationError $validationErrors "teamwork is a keyword ability under CR 702.194, not an ability word"
    }
    if ($id -eq "teamwork" -and $signal.policy -ne "never") {
        Add-ValidationError $validationErrors "teamwork is a casting-cost ability and cannot emit namedActionPerformed"
    }

    foreach ($layer in @("lexical", "parser", "runtime")) {
        if ($allowedSupport -notcontains [string]$entry.support.$layer) {
            Add-ValidationError $validationErrors "entry '$id' has invalid $layer support '$($entry.support.$layer)'"
        }
    }
    $isLexicalOnlyAbilityWord = $entry.category -eq "abilityWord" -and
        $entry.support.lexical -eq "supportedNative" -and
        $entry.support.parser -eq "unsupported" -and
        $entry.support.runtime -eq "unsupported"
    $isDocumentaryUnsupportedKeyword = $entry.category -in @("keywordAction", "keywordAbility") -and
        $entry.support.lexical -eq "unsupported" -and
        $entry.support.parser -eq "unsupported" -and
        $entry.support.runtime -eq "unsupported" -and
        $steps.Count -eq 0 -and
        @($entry.semantic.intrinsicHooks).Count -eq 0 -and
        $signal.policy -eq "never"
    if (@($entry.representativeCards).Count -lt 2 -and -not $isLexicalOnlyAbilityWord -and -not $isDocumentaryUnsupportedKeyword) {
        Add-ValidationError $validationErrors "entry '$id' needs at least two representative cards"
    }
    if (@($entry.provenance | Where-Object { $_.kind -eq "officialRules" }).Count -eq 0) {
        Add-ValidationError $validationErrors "entry '$id' lacks officialRules provenance"
    }
    if (@($entry.provenance | Where-Object { $_.kind -eq "localOracleCorpus" }).Count -eq 0 -and -not $isDocumentaryUnsupportedKeyword) {
        Add-ValidationError $validationErrors "entry '$id' lacks localOracleCorpus provenance"
    }
}

if ($validationErrors.Count -gt 0) {
    foreach ($message in $validationErrors) {
        Write-Error $message -ErrorAction Continue
    }
    throw "Ability catalog validation failed with $($validationErrors.Count) error(s)."
}

$generatedEntries = @(
    $catalog.entries |
        Sort-Object { [string]$_.canonicalId } |
        ForEach-Object {
            $entry = $_
            [ordered]@{
                canonicalId = [string]$entry.canonicalId
                category = [string]$entry.category
                lexemes = @([string]$entry.name) + @($entry.aliases | ForEach-Object { [string]$_ })
                ruleSections = @($entry.rulesReferences | ForEach-Object { [string]$_.section })
                grammarTemplateIds = @($entry.grammarTemplates | ForEach-Object { [string]$_.templateId })
                reminderGrammarTemplateIds = @($entry.reminderGrammarTemplates | ForEach-Object { [string]$_.templateId })
                primitives = @($entry.semantic.primitiveDecomposition | ForEach-Object { [string]$_.primitive })
                namedActionId = if ($entry.semantic.namedActionPerformed.policy -eq "afterActionCompletes") {
                    [string]$entry.semantic.namedActionPerformed.actionId
                } else {
                    $null
                }
                support = [ordered]@{
                    lexical = [string]$entry.support.lexical
                    parser = [string]$entry.support.parser
                    runtime = [string]$entry.support.runtime
                }
            }
        }
)

$generatedIndex = [ordered]@{
    generatedFrom = [IO.Path]::GetFileName($resolvedCatalogPath)
    catalogSchemaVersion = [string]$catalog.catalogSchemaVersion
    catalogVersion = [string]$catalog.catalogVersion
    rulesEffectiveDate = [string]$catalog.rulesVersion.effectiveDate
    entries = $generatedEntries
}
$generatedJson = $generatedIndex | ConvertTo-Json -Depth 12

if (-not [string]::IsNullOrWhiteSpace($EmitIndexPath)) {
    $resolvedEmitPath = Resolve-RepositoryPath $EmitIndexPath
    $parent = Split-Path -Parent $resolvedEmitPath
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        $null = New-Item -ItemType Directory -Path $parent -Force
    }
    [IO.File]::WriteAllText($resolvedEmitPath, $generatedJson + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    Write-Host "Generated ability catalog index: $resolvedEmitPath"
}

if ($PrintGeneratedIndex) {
    Write-Output $generatedJson
} else {
    Write-Host "Ability catalog valid: $(@($catalog.entries).Count) entries, $($templateOwners.Count) token grammar templates, $($lexemeOwners.Count) unique lexemes."
}
