[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$validator = Join-Path $repositoryRoot "scripts/validate-ability-catalog.ps1"
$catalogPath = Join-Path $repositoryRoot "data/ability-catalog/ability-catalog-v1.json"
$schemaPath = Join-Path $repositoryRoot "data/ability-catalog/ability-catalog-v1.schema.json"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-ability-catalog-tests-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $temporaryRoot

function Write-TestCatalog {
    param(
        [Parameter(Mandatory = $true)]$Catalog,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $path = Join-Path $temporaryRoot "$Name.json"
    $json = $Catalog | ConvertTo-Json -Depth 32
    [IO.File]::WriteAllText($path, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    return $path
}

function Read-FreshCatalog {
    return Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
}

function Assert-ValidationFails {
    param(
        [Parameter(Mandatory = $true)]$Catalog,
        [Parameter(Mandatory = $true)][string]$Case,
        [Parameter(Mandatory = $true)][string]$ExpectedMessage
    )

    $testPath = Write-TestCatalog $Catalog $Case
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $captured = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $validator -CatalogPath $testPath -SchemaPath $schemaPath 2>&1)
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorAction
    $combined = (($captured | Out-String) -replace '\s+', ' ')
    if ($exitCode -eq 0) {
        throw "Case '$Case' unexpectedly passed validation"
    }
    $normalizedExpected = ($ExpectedMessage -replace '\s+', ' ')
    if ($combined -notmatch [regex]::Escape($normalizedExpected)) {
        throw "Case '$Case' failed for the wrong reason: $combined"
    }
}

try {
    $generated = (& $validator -CatalogPath $catalogPath -SchemaPath $schemaPath -PrintGeneratedIndex | Out-String) | ConvertFrom-Json
    if (@($generated.entries).Count -ne 340) {
        throw "Expected three hundred forty generated catalog entries"
    }
    $actions = @($generated.entries | Where-Object { $null -ne $_.namedActionId } | ForEach-Object { $_.namedActionId })
    if (($actions -join ",") -ne "blight,connive,earthbend,firebending,scry,search,shuffle,surveil,waterbend") {
        throw "Only rules-defined named actions may generate signals; got $($actions -join ',')"
    }
    $search = $generated.entries | Where-Object canonicalId -eq "search"
    $shuffle = $generated.entries | Where-Object canonicalId -eq "shuffle"
    if (($search.primitives -join ",") -ne "chooseCards,recordLibrarySearch" -or $search.namedActionId -ne "search" -or $search.support.lexical -ne "supportedNative" -or $search.support.parser -ne "partial" -or $search.support.runtime -ne "partial") {
        throw "Search registry promotion is incomplete or overstated"
    }
    if (($shuffle.primitives -join ",") -ne "shuffleZone" -or $shuffle.namedActionId -ne "shuffle" -or $shuffle.support.lexical -ne "supportedNative" -or $shuffle.support.parser -ne "partial" -or $shuffle.support.runtime -ne "partial") {
        throw "Shuffle registry promotion is incomplete or overstated"
    }
    $teamwork = $generated.entries | Where-Object { $_.canonicalId -eq "teamwork" }
    if ($null -ne $teamwork.namedActionId -or $teamwork.category -ne "keywordAbility") {
        throw "Teamwork must remain a keyword ability without a named action"
    }
    $chapterSymbol = $generated.entries | Where-Object canonicalId -eq "chapter-symbol"
    if ($null -eq $chapterSymbol -or $chapterSymbol.category -ne "marker") {
        throw "Chapter Symbol must exist as a documentary marker"
    }
    if (($chapterSymbol.ruleSections -join ",") -ne "107.15,715.2" -or ($chapterSymbol.grammarTemplateIds -join ",") -ne "chapter-symbol.single,chapter-symbol.multiple") {
        throw "Chapter Symbol references or grammar IDs are incomplete"
    }
    $chapterPrimitives = @("sagaChapterReached", "grantAbility", "chooseCards", "moveCards", "shuffleZone", "recordLibrarySearch")
    if ((@($chapterSymbol.primitives) -join ",") -ne ($chapterPrimitives -join ",") -or $null -ne $chapterSymbol.namedActionId -or $chapterSymbol.support.lexical -ne "supportedNative" -or $chapterSymbol.support.parser -ne "supportedNative" -or $chapterSymbol.support.runtime -ne "supportedNative") {
        throw "Chapter Symbol support must match the certified native Saga vertical"
    }

    $c5SemanticHooks = @{
        dredge = "replacement.draw"
        foretell = "specialAction.permission"
        plot = "specialAction.permission"
        suspend = "events.lastCounterRemoved"
        rebound = "stack.exitReplacement"
        unearth = "zones.leaveBattlefieldReplacement"
        dash = "delayedTrigger.nextEndStep"
        blitz = "events.dies"
        ninjutsu = "combat.putOntoBattlefieldAttacking"
        miracle = "linkedAbility"
    }
    $catalog = Read-FreshCatalog
    foreach ($concept in $c5SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $concept }
        if ($null -eq $entry -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C5 concept '$concept' must exist without an artificial named action"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c5SemanticHooks[$concept]) {
            throw "C5 concept '$concept' is missing semantic hook '$($c5SemanticHooks[$concept])'"
        }
    }

    $c6SemanticHooks = @{
        persist = "lastKnownInformation.counters"
        undying = "lastKnownInformation.counters"
        exploit = "resolution.resultBinding"
        evolve = "triggers.interveningIf"
        graft = "counterMoved"
        riot = "entry.choicePerAbilityInstance"
        toxic = "damage.results.additional"
        infect = "damage.results.replacement"
        wither = "damage.results.replacement"
        annihilator = "combat.defendingPlayer"
    }
    foreach ($concept in $c6SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $concept }
        if ($null -eq $entry -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C6 concept '$concept' must exist without an artificial named action"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c6SemanticHooks[$concept]) {
            throw "C6 concept '$concept' is missing semantic hook '$($c6SemanticHooks[$concept])'"
        }
    }
    $riot = $catalog.entries | Where-Object canonicalId -eq "riot"
    if (($riot.semantic.primitiveDecomposition.primitive -join ",") -ne "chooseEntryAlternative,putEnteringCounters,grantKeyword" -or $riot.support.lexical -ne "supportedNative" -or $riot.support.parser -ne "partial" -or $riot.support.runtime -ne "supportedNative") {
        throw "Riot support must match the certified #112 native grant and runtime boundary"
    }
    foreach ($hook in @("continuous.grantKeyword", "replacement.copyBeforeGrantedEntryChoice", "entry.choicePerAbilityInstance")) {
        if (@($riot.semantic.intrinsicHooks) -notcontains $hook) { throw "Riot is missing certified hook '$hook'" }
    }

    $c7SemanticHooks = @{
        changeling = "characteristics.cda"
        devoid = "characteristics.cda"
        shroud = "targeting.legality"
        fear = "evasion.blockerPredicate"
        intimidate = "evasion.blockerPredicate"
        shadow = "evasion.symmetricCompatibility"
        skulk = "evasion.numericPredicate"
        horsemanship = "evasion.asymmetricKeywordPredicate"
        forestwalk = "evasion.landwalk"
        islandwalk = "evasion.landwalk"
        mountainwalk = "evasion.landwalk"
        plainswalk = "evasion.landwalk"
        swampwalk = "evasion.landwalk"
        "umbra-armor" = "replacement.destroy"
    }
    foreach ($concept in $c7SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $concept }
        if ($null -eq $entry -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C7 concept '$concept' must exist without an artificial named action"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c7SemanticHooks[$concept]) {
            throw "C7 concept '$concept' is missing semantic hook '$($c7SemanticHooks[$concept])'"
        }
    }
    $landwalkPrimitives = @($catalog.entries | Where-Object { $_.canonicalId -in @("forestwalk", "islandwalk", "mountainwalk", "plainswalk", "swampwalk") } | ForEach-Object { $_.semantic.primitiveDecomposition[0].primitive } | Select-Object -Unique)
    if ($landwalkPrimitives.Count -ne 1 -or $landwalkPrimitives[0] -ne "applyLandwalkBlockingRestriction") {
        throw "Landwalk variants must share one parameterized semantic primitive"
    }

    $c8SemanticHooks = @{
        cascade = "casting.zonePermission"
        conspire = "stack.copySpell"
        gravestorm = "turnHistory.zoneChanges"
        replicate = "casting.paymentBinding"
        storm = "turnHistory.spellsCast"
        splice = "stack.textChange"
        "split-second" = "stack.actionRestriction"
        fuse = "casting.modePermission"
        escalate = "casting.modeDeclaration"
        offspring = "copy.tokenCopiableValues"
        gift = "casting.choiceBinding"
    }
    foreach ($concept in $c8SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $concept }
        if ($null -eq $entry -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C8 concept '$concept' must exist without an artificial named action"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c8SemanticHooks[$concept]) {
            throw "C8 concept '$concept' is missing semantic hook '$($c8SemanticHooks[$concept])'"
        }
    }
    foreach ($copyConcept in @("storm", "gravestorm", "replicate")) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $copyConcept }
        if (@($entry.semantic.primitiveDecomposition.primitive) -notcontains "copySpellBatchOnStack") {
            throw "C8 concept '$copyConcept' must share batch stack-copy semantics"
        }
    }

    $c9SemanticHooks = @{
        exalted = "combat.attackPattern"
        flanking = "combat.blockerKeywordPredicate"
        myriad = "copy.tokenCopiableValues"
        encore = "activation.zonePermission"
        "living-weapon" = "attachments.equipment"
        mobilize = "tokens.predefinedCharacteristics"
        firebending = "mana.retentionBoundary"
        waterbend = "cost.componentIsolation"
        blight = "counterSemantics.-1/-1"
    }
    foreach ($concept in $c9SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $concept }
        if ($null -eq $entry) {
            throw "C9 concept '$concept' must exist"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c9SemanticHooks[$concept]) {
            throw "C9 concept '$concept' is missing semantic hook '$($c9SemanticHooks[$concept])'"
        }
    }
    foreach ($temporaryCopyConcept in @("myriad", "encore")) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $temporaryCopyConcept }
        if (@($entry.semantic.primitiveDecomposition.primitive) -notcontains "createTokenCopyBatch") {
            throw "C9 concept '$temporaryCopyConcept' must share batched token-copy semantics"
        }
    }
    foreach ($attackTriggerConcept in @("exalted", "myriad", "mobilize", "firebending")) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $attackTriggerConcept }
        if (@($entry.semantic.intrinsicHooks) -notcontains $(if ($attackTriggerConcept -eq "exalted") { "events.attackersDeclared" } else { "events.creatureAttacks" })) {
            throw "C9 concept '$attackTriggerConcept' must expose its generic attack event"
        }
    }
    foreach ($rulesBackedSignal in @("blight", "firebending", "waterbend")) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $rulesBackedSignal }
        if ($entry.semantic.namedActionPerformed.actionId -ne $rulesBackedSignal -or $entry.semantic.namedActionPerformed.cardinality -ne "oncePerAction") {
            throw "C9 rules-defined event '$rulesBackedSignal' must emit exactly one matching named-action signal"
        }
    }
    foreach ($nonAction in @("exalted", "flanking", "myriad", "encore", "living-weapon", "mobilize")) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $nonAction }
        if ($entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C9 keyword ability '$nonAction' must not invent a named-action signal"
        }
    }
    $mobilize = $catalog.entries | Where-Object canonicalId -eq "mobilize"
    if (($mobilize.semantic.primitiveDecomposition.primitive -join ",") -ne "createTokens,installDelayedStepTrigger,sacrificePermanents" -or $mobilize.support.lexical -ne "supportedNative" -or $mobilize.support.parser -ne "supportedNative" -or $mobilize.support.runtime -ne "supportedNative") {
        throw "Mobilize must match the certified #113 native primitive vertical"
    }
    foreach ($hook in @("bindings.captureBindings", "combat.putOntoBattlefieldAttacking", "delayedTrigger.nextEndStep", "sacrifice.currentControllerOnly")) {
        if (@($mobilize.semantic.intrinsicHooks) -notcontains $hook) { throw "Mobilize is missing certified hook '$hook'" }
    }
    $compiledIntegrations = @($catalog.entries | Where-Object { $null -ne $_.compiledIntegration })
    if ($compiledIntegrations.Count -ne 1 -or $compiledIntegrations[0].canonicalId -ne "mobilize") {
        throw "Mobilize must be the only certified compiled integration in c20"
    }
    if ($mobilize.compiledIntegration.recognizerId -ne "keyword.mobilize.v1" -or $mobilize.compiledIntegration.loweringAdapterId -ne "lower.keyword.mobilize.v1") {
        throw "Mobilize compiled integration IDs differ from the closed registry"
    }
    if ((@($mobilize.compiledIntegration.bindings | ForEach-Object { "$($_.name):$($_.type):$($_.source)" }) -join ",") -ne "quantity:positiveInteger:recognizerOutput,mobilizeTokens:objectSet:loweringOutput") {
        throw "Mobilize compiled binding contract changed"
    }
    if ((@($mobilize.compiledIntegration.emittedPrimitives) -join ",") -ne "createTokens,installDelayedStepTrigger,sacrificePermanents" -or $mobilize.compiledIntegration.unknownPolicy -ne "structuredFailure") {
        throw "Mobilize compiled primitive/UNKNOWN contract changed"
    }

    $earthbend = $catalog.entries | Where-Object canonicalId -eq "earthbend"
    if (($earthbend.semantic.primitiveDecomposition.primitive -join ",") -ne "becomeCreature,grantKeyword,putCounters,installDelayedZoneChangeTrigger,moveTriggeringCard,emitAbilitySignal") {
        throw "Earthbend target decomposition must remain primitive and ordered"
    }
    if ($earthbend.semantic.namedActionPerformed.actionId -ne "earthbend" -or $earthbend.semantic.namedActionPerformed.cardinality -ne "oncePerAction" -or $earthbend.semantic.namedActionPerformed.zeroQuantity -ne "emit") {
        throw "Earthbend must emit exactly once when its delayed return trigger is created"
    }
    if ($earthbend.support.lexical -ne "supportedNative" -or $earthbend.support.parser -ne "supportedNative" -or $earthbend.support.runtime -ne "supportedNative") {
        throw "Earthbend support must reflect the certified #114 native vertical"
    }
    if ($null -ne $earthbend.compiledIntegration) { throw "Earthbend cannot claim compiledIntegration before its adapter validates a closed typed contract" }
    foreach ($proofPath in @("crates/deepdeck-oracle/tests/ba_sing_se_fsm.rs", "crates/deepdeck-contract-fixture-runner/fixtures/ba-sing-se.json", "src/engine/tests/earthbend_primitives_runtime.rs", "crates/deepdeck-runtime-fixture-runner/fixtures/ba-sing-se-earthbend.json")) {
        if (($earthbend.provenance.locator -join ";") -notmatch [regex]::Escape($proofPath)) { throw "Earthbend is missing certified proof '$proofPath'" }
    }
    foreach ($hook in @("targeting.controlledLand", "delayedTrigger.diesOrExiled", "events.namedActionAtDelayedTriggerCreation")) {
        if (@($earthbend.semantic.intrinsicHooks) -notcontains $hook) { throw "Earthbend is missing rules hook '$hook'" }
    }

    $investigate = $catalog.entries | Where-Object canonicalId -eq "investigate"
    if (($investigate.semantic.primitiveDecomposition.primitive -join ",") -ne "createTokens" -or $investigate.semantic.namedActionPerformed.policy -ne "never" -or $investigate.support.lexical -ne "supportedNative" -or $investigate.support.parser -ne "partial" -or $investigate.support.runtime -ne "supportedNative") {
        throw "Investigate must expose the certified named-Clue primitive without inventing a signal"
    }
    $mill = $catalog.entries | Where-Object canonicalId -eq "mill"
    if (($mill.semantic.primitiveDecomposition.primitive -join ",") -ne "mill" -or $mill.semantic.namedActionPerformed.policy -ne "never" -or $mill.support.lexical -ne "supportedNative" -or $mill.support.parser -ne "partial" -or $mill.support.runtime -ne "supportedNative") {
        throw "Mill must expose its atomic runtime operation and partial native grammar honestly"
    }
    foreach ($hook in @("replacement.millMultiplier", "resolution.resultBinding")) {
        if (@($mill.semantic.intrinsicHooks) -notcontains $hook) { throw "Mill is missing runtime hook '$hook'" }
    }
    $harness = $catalog.entries | Where-Object canonicalId -eq "harness"
    if (($harness.semantic.primitiveDecomposition.primitive -join ",") -ne "setPermanentFlag" -or $harness.semantic.namedActionPerformed.policy -ne "never" -or $harness.support.lexical -ne "supportedNative" -or $harness.support.parser -ne "supportedLegacy" -or $harness.support.runtime -ne "partial") {
        throw "Harness must remain a legacy-recognized designation with partial runtime proof"
    }
    $overload = $catalog.entries | Where-Object canonicalId -eq "overload"
    if ($overload.support.lexical -ne "unsupported" -or $overload.support.parser -ne "unsupported" -or $overload.support.runtime -ne "unsupported") {
        throw "Overload must remain explicit Marvel integration debt until parser/runtime proof exists"
    }

    $c21Actions = @{
        destroy = [pscustomobject]@{ section = "701.8"; primitives = "destroyPermanent" }
        discard = [pscustomobject]@{ section = "701.9"; primitives = "discardCards" }
        exile = [pscustomobject]@{ section = "701.13"; primitives = "exilePermanent,moveCards" }
        sacrifice = [pscustomobject]@{ section = "701.21"; primitives = "sacrificePermanents" }
        "tap-and-untap" = [pscustomobject]@{ section = "701.26"; primitives = "tapPermanent,tapPermanents,untapPermanent,untapPermanents" }
    }
    foreach ($actionId in $c21Actions.Keys) {
        $entry = $catalog.entries | Where-Object canonicalId -eq $actionId
        $expected = $c21Actions[$actionId]
        if (@($entry.rulesReferences | Where-Object section -eq $expected.section).Count -ne 1) { throw "C21 action '$actionId' lost CR $($expected.section)" }
        if (($entry.semantic.primitiveDecomposition.primitive -join ",") -ne $expected.primitives) { throw "C21 action '$actionId' primitive map changed" }
        if ($entry.semantic.namedActionPerformed.policy -ne "never") { throw "C21 action '$actionId' must use intrinsic events, not an ability signal" }
        if ($entry.support.lexical -ne "supportedNative" -or $entry.support.parser -ne "partial" -or $entry.support.runtime -ne "supportedNative") { throw "C21 action '$actionId' support is overstated or stale" }
        if ($null -ne $entry.compiledIntegration) { throw "C21 action '$actionId' cannot claim compiledIntegration without a closed adapter" }
    }
    $tapAndUntap = $catalog.entries | Where-Object canonicalId -eq "tap-and-untap"
    if ((@($tapAndUntap.aliases) -join ",") -ne "Tap,Untap,Taps,Untaps") { throw "Tap and Untap lexical variants changed" }

    $reveal = $catalog.entries | Where-Object canonicalId -eq "reveal"
    if (@($reveal.rulesReferences | Where-Object section -eq "701.20").Count -ne 1 -or @($reveal.rulesText.clauses).Count -ne 5) { throw "Reveal must retain the complete CR 701.20 documentary rules" }
    if (($reveal.grammarTemplates.templateId -join ",") -ne "reveal.card-reference,reveal.library-top,reveal.hand,reveal.look-at") { throw "Reveal local grammar variants changed" }
    if (($reveal.semantic.primitiveDecomposition.primitive -join ",") -ne "revealCards,revealHand,lookAtTopCards") { throw "Reveal primitive alternatives changed" }
    if ($reveal.semantic.namedActionPerformed.policy -ne "never" -or $reveal.support.lexical -ne "supportedNative" -or $reveal.support.parser -ne "partial" -or $reveal.support.runtime -ne "supportedNative") { throw "Reveal support or intrinsic-event policy is inaccurate" }
    if ($null -ne $reveal.compiledIntegration) { throw "Reveal cannot claim compiledIntegration before one closed adapter covers its variants" }

    $c10SemanticHooks = @{
        ascend = "designation.player"
        awaken = "targetDeclaration.conditionalTargets"
        typecycling = "events.cardCycled"
        "basic-landcycling" = "events.cardCycled"
        companion = "specialAction.permission"
        echo = "history.controlDuration"
        morph = "characteristics.faceDown"
        prototype = "characteristics.alternativeSet"
    }
    foreach ($concept in $c10SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object { $_.canonicalId -eq $concept }
        if ($null -eq $entry -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C10 concept '$concept' must exist without an artificial named action"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c10SemanticHooks[$concept]) {
            throw "C10 concept '$concept' is missing semantic hook '$($c10SemanticHooks[$concept])'"
        }
    }
    $typecyclingPrimitives = @($catalog.entries | Where-Object { $_.canonicalId -in @("typecycling", "basic-landcycling") } | ForEach-Object { ,@($_.semantic.primitiveDecomposition.primitive) })
    if ($typecyclingPrimitives.Count -ne 2 -or (($typecyclingPrimitives[0] -join ",") -ne ($typecyclingPrimitives[1] -join ","))) {
        throw "Basic landcycling and Typecycling must share one parameterized primitive chain"
    }
    $ascend = $catalog.entries | Where-Object canonicalId -eq "ascend"
    if (@($ascend.semantic.primitiveDecomposition.primitive) -contains "createEmblem" -or @($ascend.semantic.primitiveDecomposition.primitive) -contains "putCounters") {
        throw "Ascend must model the city's blessing as a designation, not an emblem or counter"
    }
    $morph = $catalog.entries | Where-Object canonicalId -eq "morph"
    if (@($morph.semantic.primitiveDecomposition.primitive) -notcontains "grantSpecialActionPermission" -or @($morph.semantic.primitiveDecomposition.primitive) -notcontains "turnPermanentFaceUp") {
        throw "Morph must separate face-down casting from the turn-face-up special action"
    }
    $prototype = $catalog.entries | Where-Object canonicalId -eq "prototype"
    if (@($prototype.semantic.primitiveDecomposition.primitive) -notcontains "markCopiableCharacteristics") {
        throw "Prototype must preserve its selected characteristic set as copiable values"
    }

    foreach ($abilityWordId in @("channel", "renewal", "revolt")) {
        $entry = $catalog.entries | Where-Object canonicalId -eq $abilityWordId
        if ($null -eq $entry -or $entry.category -ne "abilityWord") {
            throw "C11 '$abilityWordId' must remain an ability word"
        }
        if (@($entry.semantic.primitiveDecomposition).Count -ne 0 -or @($entry.semantic.intrinsicHooks).Count -ne 0 -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C11 ability word '$abilityWordId' cannot own semantics or a signal"
        }
    }
    $renewal = $catalog.entries | Where-Object canonicalId -eq "renewal"
    if (@($renewal.aliases) -notcontains "Renew" -or $renewal.classificationNotes -notmatch "authoritative ability word is Renew") {
        throw "The local Renewal inventory key must expose official Renew as the authoritative label"
    }

    $c11SemanticHooks = @{
        impending = "characteristics.typeSuppression"
        mayhem = "turnHistory.discards"
        paradigm = "gameHistory.resolutionsByName"
        "read-ahead" = "saga.chapterTrigger"
        warp = "turnHistory.alternativeCosts"
    }
    foreach ($concept in $c11SemanticHooks.Keys) {
        $entry = $catalog.entries | Where-Object canonicalId -eq $concept
        if ($null -eq $entry -or $entry.category -ne "keywordAbility" -or $entry.semantic.namedActionPerformed.policy -ne "never") {
            throw "C11 keyword '$concept' must exist without an artificial named-action signal"
        }
        if (@($entry.semantic.intrinsicHooks) -notcontains $c11SemanticHooks[$concept]) {
            throw "C11 keyword '$concept' is missing semantic hook '$($c11SemanticHooks[$concept])'"
        }
    }
    $mayhem = $catalog.entries | Where-Object canonicalId -eq "mayhem"
    if (@($mayhem.semantic.primitiveDecomposition.primitive) -notcontains "grantPlayPermission") {
        throw "Costless Mayhem must support playing lands, not only casting spells"
    }
    $paradigm = $catalog.entries | Where-Object canonicalId -eq "paradigm"
    if (@($paradigm.semantic.primitiveDecomposition.primitive) -notcontains "createRepeatingDelayedTrigger" -or @($paradigm.semantic.primitiveDecomposition.primitive) -notcontains "createObjectCopyInZone") {
        throw "Paradigm must decompose into resolution history, a repeating trigger, and an exile-zone copy"
    }
    $warp = $catalog.entries | Where-Object canonicalId -eq "warp"
    if (@($warp.semantic.primitiveDecomposition.primitive) -notcontains "recordAlternativeCostHistory" -or @($warp.semantic.primitiveDecomposition.primitive) -notcontains "bindLinkedZoneObject") {
        throw "Warp must separate cast-mode history from linked delayed exile permission"
    }

    $collision = Read-FreshCatalog
    $collision.entries[1].aliases += "Scry"
    Assert-ValidationFails $collision "lexeme-collision" "lexeme collision"

    $duplicateLexeme = Read-FreshCatalog
    $duplicateLexeme.entries[0].aliases += "scry"
    Assert-ValidationFails $duplicateLexeme "duplicate-normalized-lexeme" "duplicate normalized lexeme"

    $identityCollision = Read-FreshCatalog
    $identityCollision.entries[1].aliases += "double strike"
    Assert-ValidationFails $identityCollision "identity-collision" "ability identity collision"

    $exactPhrase = Read-FreshCatalog
    $exactPhrase.entries[0].grammarTemplates[0].tokenGrammar = "Scry a card."
    Assert-ValidationFails $exactPhrase "exact-phrase" "symbolic token atoms"

    $teamworkSignal = Read-FreshCatalog
    $teamworkEntry = $teamworkSignal.entries | Where-Object { $_.canonicalId -eq "teamwork" }
    $teamworkEntry.semantic.namedActionPerformed.policy = "afterActionCompletes"
    $teamworkEntry.semantic.namedActionPerformed.cardinality = "oncePerAction"
    $teamworkEntry.semantic.namedActionPerformed | Add-Member -NotePropertyName actionId -NotePropertyValue "teamwork"
    Assert-ValidationFails $teamworkSignal "teamwork-signal" "cannot emit namedActionPerformed"

    $abilityWord = Read-FreshCatalog
    $abilityWord.entries[5].category = "abilityWord"
    Assert-ValidationFails $abilityWord "ability-word-semantics" "ability word 'flying' cannot own"

    $markerSemantics = Read-FreshCatalog
    $markerEntry = $markerSemantics.entries | Where-Object canonicalId -eq "chapter-symbol"
    $markerEntry.support.runtime = "unsupported"
    Assert-ValidationFails $markerSemantics "marker-semantics" "semantic marker 'chapter-symbol' must"

    $badReference = Read-FreshCatalog
    $badReference.entries[0].rulesReferences[0].section = "Scry rule"
    Assert-ValidationFails $badReference "rules-reference" "invalid rules section"

    $unlinkedRulesText = Read-FreshCatalog
    $unlinkedRulesText.entries[0].rulesText.clauses[0].sourceSection = "999.1a"
    Assert-ValidationFails $unlinkedRulesText "rules-text-reference" "absent from rulesReferences"

    $unknownCompiledRecognizer = Read-FreshCatalog
    ($unknownCompiledRecognizer.entries | Where-Object canonicalId -eq "mobilize").compiledIntegration.recognizerId = "keyword.unknown.v1"
    Assert-ValidationFails $unknownCompiledRecognizer "unknown-compiled-recognizer" "keyword.unknown.v1"

    $unknownCompiledBinding = Read-FreshCatalog
    ($unknownCompiledBinding.entries | Where-Object canonicalId -eq "mobilize").compiledIntegration.bindings[0].name = "unknownQuantity"
    Assert-ValidationFails $unknownCompiledBinding "unknown-compiled-binding" "keyword.mobilize.v1"

    Write-Host "Ability catalog tests passed: positive generation plus 11 negative invariants."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
    }
}
