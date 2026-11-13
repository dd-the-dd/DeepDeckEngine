[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$builder = Join-Path $repositoryRoot "scripts/build-ability-indexes.ps1"
$catalog = Join-Path $repositoryRoot "data/ability-catalog/ability-catalog-v1.json"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-ability-indexes-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $temporaryRoot
try {
    $lexiconOne = Join-Path $temporaryRoot "lexicon-one.json"
    $registryOne = Join-Path $temporaryRoot "registry-one.json"
    $documentationOne = Join-Path $temporaryRoot "documentation-one.json"
    $lexiconTwo = Join-Path $temporaryRoot "lexicon-two.json"
    $registryTwo = Join-Path $temporaryRoot "registry-two.json"
    $documentationTwo = Join-Path $temporaryRoot "documentation-two.json"
    & $builder -CatalogPath $catalog -LexiconOutputPath $lexiconOne -RegistryOutputPath $registryOne -DocumentationOutputPath $documentationOne
    & $builder -CatalogPath $catalog -LexiconOutputPath $lexiconTwo -RegistryOutputPath $registryTwo -DocumentationOutputPath $documentationTwo
    if ((Get-Content -Raw $lexiconOne) -ne (Get-Content -Raw $lexiconTwo)) { throw "Lexicon index is not deterministic" }
    if ((Get-Content -Raw $registryOne) -ne (Get-Content -Raw $registryTwo)) { throw "Registry index is not deterministic" }
    if ((Get-Content -Raw $documentationOne) -ne (Get-Content -Raw $documentationTwo)) { throw "Documentation index is not deterministic" }

    $lexicon = Get-Content -Raw -Encoding UTF8 $lexiconOne | ConvertFrom-Json
    $registry = Get-Content -Raw -Encoding UTF8 $registryOne | ConvertFrom-Json
    $documentation = Get-Content -Raw -Encoding UTF8 $documentationOne | ConvertFrom-Json
    if ($lexicon.lexiconSchemaVersion -ne "deepdeck-ability-lexicon/v2" -or @($lexicon.entries).Count -ne 177) { throw "Unexpected lexicon v2" }
    if ($registry.registrySchemaVersion -ne "deepdeck-ability-registry/v2" -or @($registry.entries).Count -ne 177) { throw "Unexpected registry v2" }
    if ($documentation.documentationSchemaVersion -ne "deepdeck-ability-documentation/v2" -or @($documentation.entries).Count -ne 340) { throw "Unexpected documentation index v2" }
    if (@($documentation.entries | Where-Object { $null -ne $_.semantics }).Count -ne 0) { throw "Documentation index leaked executable semantics" }
    if (@($registry.entries | Where-Object { $_.support.lexical -eq "unsupported" -and $_.support.parser -eq "unsupported" -and $_.support.runtime -eq "unsupported" }).Count -ne 0) { throw "Documentary-only entries leaked into the semantic registry" }
    if ($null -ne $lexicon.entries[0].rulesText -or $null -ne $lexicon.entries[0].grammar) { throw "Minimal lexicon leaked registry documentation" }
    if (@($lexicon.entries | Where-Object { $null -ne $_.compiledIntegration }).Count -ne 0 -or @($documentation.entries | Where-Object { $null -ne $_.compiledIntegration }).Count -ne 0) { throw "Compiled integration leaked outside the typed semantic registry" }

    $compiledIntegrations = @($registry.entries | Where-Object { $null -ne $_.compiledIntegration })
    if ($compiledIntegrations.Count -ne 1 -or $compiledIntegrations[0].canonicalId -ne "mobilize") { throw "Expected one certified Mobilize compiled integration" }
    $compiledMobilize = $compiledIntegrations[0].compiledIntegration
    if ($compiledMobilize.recognizerId -ne "keyword.mobilize.v1" -or $compiledMobilize.loweringAdapterId -ne "lower.keyword.mobilize.v1") { throw "Mobilize compiled integration IDs changed" }
    if ((@($compiledMobilize.bindings | ForEach-Object { "$($_.name):$($_.type):$($_.source)" }) -join ",") -ne "quantity:positiveInteger:recognizerOutput,mobilizeTokens:objectSet:loweringOutput") { throw "Mobilize compiled bindings changed" }
    if ((@($compiledMobilize.emittedPrimitives) -join ",") -ne "createTokens,installDelayedStepTrigger,sacrificePermanents" -or $compiledMobilize.unknownPolicy -ne "structuredFailure") { throw "Mobilize compiled primitive/UNKNOWN contract changed" }

    $surveil = $registry.entries | Where-Object canonicalId -eq "surveil"
    if ($surveil.semantics.kind -ne "primitiveExpansion" -or $surveil.semantics.signal.actionId -ne "surveil") { throw "Surveil registry semantics missing" }
    if (@($surveil.rulesText.clauses).Count -eq 0 -or @($surveil.grammar | Where-Object role -eq "primary").Count -eq 0) { throw "Surveil registry documentation/grammar IDs missing" }
    $search = $registry.entries | Where-Object canonicalId -eq "search"
    $shuffle = $registry.entries | Where-Object canonicalId -eq "shuffle"
    if (($search.semantics.primitiveSteps.primitive -join ",") -ne "chooseCards,recordLibrarySearch" -or $search.semantics.signal.actionId -ne "search" -or $search.support.runtime -ne "partial") { throw "Search partial integration metadata missing" }
    if (($shuffle.semantics.primitiveSteps.primitive -join ",") -ne "shuffleZone" -or $shuffle.semantics.signal.actionId -ne "shuffle" -or $shuffle.support.runtime -ne "partial") { throw "Shuffle partial integration metadata missing" }
    $abilityWords = @($registry.entries | Where-Object category -eq "abilityWord")
    if ($abilityWords.Count -ne 66) { throw "Expected all 66 CR 207.2c ability words" }
    foreach ($abilityWord in $abilityWords) {
        if ($abilityWord.semantics.kind -ne "markerOnly" -or @($abilityWord.semantics.primitiveSteps).Count -ne 0 -or $abilityWord.semantics.signal.policy -ne "never") {
            throw "Ability word '$($abilityWord.canonicalId)' gained executable semantics"
        }
    }
    if (@($lexicon.entries | Where-Object { $_.canonicalId -in @("activate", "landwalk", "tiered") }).Count -ne 0) { throw "Unsupported documentary entries leaked into the runtime lexicon" }
    if (@($registry.entries | Where-Object { $_.canonicalId -in @("activate", "landwalk", "tiered") }).Count -ne 0) { throw "Unsupported documentary entries leaked into the semantic registry" }
    $scryDocumentation = $documentation.entries | Where-Object canonicalId -eq "scry"
    $surveilDocumentation = $documentation.entries | Where-Object canonicalId -eq "surveil"
    if (@($scryDocumentation.rulesReferences | Where-Object section -eq "701.22").Count -ne 1) { throw "Scry documentation was not realigned" }
    if (@($surveilDocumentation.rulesReferences | Where-Object section -eq "701.25").Count -ne 1) { throw "Surveil documentation was not realigned" }
    $documentaryUnsupported = @($documentation.entries | Where-Object { $_.support.lexical -eq "unsupported" -and $_.support.parser -eq "unsupported" -and $_.support.runtime -eq "unsupported" })
    if ($documentaryUnsupported.Count -ne 162) { throw "Expected 162 documentary-only official keyword surfaces" }
    if (@($documentaryUnsupported | Where-Object category -in @("keywordAction", "keywordAbility")).Count -ne 162) { throw "Official keyword integration debt changed unexpectedly" }
    $c21Actions = @($registry.entries | Where-Object canonicalId -in @("destroy", "discard", "exile", "sacrifice", "tap-and-untap"))
    if ($c21Actions.Count -ne 5 -or @($c21Actions | Where-Object { $_.semantics.signal.policy -ne "never" }).Count -ne 0) { throw "C21 generic actions missing or emitting artificial ability signals" }
    if (@($c21Actions | Where-Object { $null -ne $_.compiledIntegration }).Count -ne 0) { throw "C21 generic actions claimed an unproved compiled integration" }
    $reveal = $registry.entries | Where-Object canonicalId -eq "reveal"
    if (($reveal.semantics.primitiveSteps.primitive -join ",") -ne "revealCards,revealHand,lookAtTopCards" -or $reveal.semantics.signal.policy -ne "never" -or $null -ne $reveal.compiledIntegration) { throw "C22 Reveal registry integration is inaccurate" }
    $transform = $registry.entries | Where-Object canonicalId -eq "transform"
    if (($transform.semantics.primitiveSteps.primitive -join ",") -ne "transformPermanent" -or $transform.semantics.signal.policy -ne "never" -or $transform.support.parser -ne "partial" -or $transform.support.runtime -ne "partial" -or $null -ne $transform.compiledIntegration) { throw "C23 Transform registry integration is inaccurate" }
    if (@($transform.semantics.intrinsicHooks) -notcontains "events.permanentTransformed" -or @($transform.semantics.intrinsicHooks) -notcontains "counters.preservedBySameObject" -or @($transform.semantics.intrinsicHooks) -notcontains "counters.noIntrinsicCounterName") { throw "C23 Transform event/counter boundaries are incomplete" }
    $chapterSymbol = $documentation.entries | Where-Object canonicalId -eq "chapter-symbol"
    if ($null -eq $chapterSymbol -or $chapterSymbol.category -ne "marker") { throw "Chapter Symbol documentary surface missing" }
    if (@($chapterSymbol.rulesReferences | Where-Object section -eq "107.15").Count -ne 1 -or @($chapterSymbol.rulesReferences | Where-Object section -eq "715.2").Count -ne 1) { throw "Chapter Symbol rules references are incomplete" }
    if (($chapterSymbol.grammarTemplateIds -join ",") -ne "chapter-symbol.single,chapter-symbol.multiple") { throw "Chapter Symbol grammar IDs are not deterministic" }
    if ($chapterSymbol.support.lexical -ne "supportedNative" -or $chapterSymbol.support.parser -ne "supportedNative" -or $chapterSymbol.support.runtime -ne "supportedNative") { throw "Chapter Symbol native support promotion is missing" }
    if (@($lexicon.entries | Where-Object canonicalId -eq "chapter-symbol").Count -ne 0 -or @($registry.entries | Where-Object canonicalId -eq "chapter-symbol").Count -ne 0) { throw "Chapter Symbol leaked into runtime indexes" }
    $generatedAbilityWords = @($abilityWords | Where-Object canonicalId -notin @("channel", "renewal", "revolt"))
    foreach ($abilityWord in $generatedAbilityWords) {
        if ($abilityWord.support.lexical -ne "supportedNative" -or $abilityWord.support.parser -ne "unsupported" -or $abilityWord.support.runtime -ne "unsupported") {
            throw "Generated ability word '$($abilityWord.canonicalId)' overstates support"
        }
    }
    Write-Host "Ability split-index tests passed: 177 runtime entries isolated from 340 documentary entries."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
