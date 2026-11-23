$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$partitionCounts = [ordered]@{
    "ci-unit-tests-1" = 0
    "ci-unit-tests-2" = 0
    "ci-unit-tests-3" = 0
    "ci-unit-tests-4" = 0
}
$expectedPartitionCounts = [ordered]@{
    "ci-unit-tests-1" = 258
    "ci-unit-tests-2" = 180
    "ci-unit-tests-3" = 816
    "ci-unit-tests-4" = 287
}
$expectedDedicatedEngineTestFileCount = 220

function Get-RelativePath([string]$Root,[string]$Path){return [IO.Path]::GetFullPath($Path).Substring([IO.Path]::GetFullPath($Root).Length).TrimStart([char[]]@('\','/')).Replace('\','/')}
function Test-IsDedicatedEngineTest([string]$RelativePath){return $RelativePath.StartsWith('src/engine/tests/') -or [IO.Path]::GetFileName($RelativePath) -match '(^tests|_tests)\.rs$'}
function Get-AttributeContext([string[]]$Lines,[int]$LineIndex){$attributes=@();$cursor=$LineIndex-1;$limit=[Math]::Max(0,$LineIndex-30);while($cursor-ge$limit){$trimmed=$Lines[$cursor].Trim();if($trimmed.StartsWith('#[')){$attributes=@($Lines[$cursor])+$attributes;$cursor--;continue};if($trimmed.EndsWith(']')){$block=@();while($cursor-ge$limit-and-not$Lines[$cursor].Trim().StartsWith('#[')){$block=@($Lines[$cursor])+$block;$cursor--};if($cursor-lt$limit){break};$block=@($Lines[$cursor])+$block;$attributes=$block+$attributes;$cursor--;continue};break};return($attributes-join"`n")}
function Get-PartitionFeatures([string]$Attributes){return @([regex]::Matches($Attributes,'feature\s*=\s*"(ci-unit-tests-[1-4])"')|ForEach-Object{$_.Groups[1].Value}|Sort-Object -Unique)}
function Add-FeatureSet([hashtable]$Sets,[string]$Path,[string[]]$Features){$before=@($Sets[$Path]);$after=@($before+$Features|Sort-Object -Unique);$Sets[$Path]=$after;return ($after.Count -ne $before.Count)}

function Get-EngineDedicatedTestInventory([string]$Root){
    $engineRoot=Join-Path $Root 'src/engine'
    $files=@(Get-ChildItem -LiteralPath $engineRoot -Recurse -File -Filter '*.rs')
    $candidates=@($files|ForEach-Object{Get-RelativePath $Root $_.FullName}|Where-Object{Test-IsDedicatedEngineTest $_}|Sort-Object -Unique)
    $edges=@()
    foreach($file in $files){
        $relative=Get-RelativePath $Root $file.FullName;$lines=[IO.File]::ReadAllLines($file.FullName)
        for($lineIndex=0;$lineIndex-lt$lines.Length;$lineIndex++){
            if($lines[$lineIndex]-notmatch'^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;'){continue}
            $module=$Matches[1];$attributes=Get-AttributeContext $lines $lineIndex;$pathMatch=[regex]::Match($attributes,'path\s*=\s*"([^"]+)"')
            if($pathMatch.Success){$target=[IO.Path]::GetFullPath((Join-Path $file.DirectoryName $pathMatch.Groups[1].Value))}
            else{$base=if($file.Name-eq'mod.rs'){$file.DirectoryName}else{Join-Path $file.DirectoryName $file.BaseName};$flat=Join-Path $base "$module.rs";$nested=Join-Path $base "$module/mod.rs";$target=if(Test-Path -LiteralPath $flat){$flat}elseif(Test-Path -LiteralPath $nested){$nested}else{$flat}}
            $targetRelative=Get-RelativePath $Root $target;$parentDedicated=Test-IsDedicatedEngineTest $relative;$targetDedicated=Test-IsDedicatedEngineTest $targetRelative
            if(-not(Test-Path -LiteralPath $target -PathType Leaf)){
                if($parentDedicated-or$targetDedicated){throw "${relative}:$($lineIndex+1): declared test module is missing: $targetRelative"}
                continue
            }
            $edges+=[pscustomobject]@{Parent=$relative;Child=$targetRelative;Features=@(Get-PartitionFeatures $attributes)}
        }
    }
    # Only production modules reachable from engine/mod.rs can make their
    # dedicated tests part of a compiled harness. This keeps abandoned or
    # staged source files visible to ordinary source review without forcing the
    # test guard to wire them into production merely to satisfy inventory.
    $reachableProduction=@{'src/engine/mod.rs'=$true}
    do{
        $changed=$false
        foreach($edge in $edges|Where-Object{$reachableProduction.ContainsKey($_.Parent)-and-not(Test-IsDedicatedEngineTest $_.Child)}){
            if(-not$reachableProduction.ContainsKey($edge.Child)){$reachableProduction[$edge.Child]=$true;$changed=$true}
        }
    }while($changed)
    $included=@{}
    foreach($edge in $edges|Where-Object{$reachableProduction.ContainsKey($_.Parent)-and(Test-IsDedicatedEngineTest $_.Child)}){
        if($edge.Features.Count-eq0){throw "$($edge.Parent): dedicated test module $($edge.Child) has no CI partition cfg"}
        [void](Add-FeatureSet $included $edge.Child $edge.Features)
    }
    do{
        $changed=$false
        foreach($edge in $edges|Where-Object{$included.ContainsKey($_.Parent)-and(Test-IsDedicatedEngineTest $_.Child)}){
            $features=if($edge.Features.Count){@($edge.Features)}else{@($included[$edge.Parent])}
            if(Add-FeatureSet $included $edge.Child $features){$changed=$true}
        }
    }while($changed)
    $relevantCandidates=@($candidates|Where-Object{
        if($included.ContainsKey($_)){return $true}
        if($_.StartsWith('src/engine/tests/')-and$included.ContainsKey('src/engine/tests.rs')){return $true}
        $fileName=[IO.Path]::GetFileName($_)
        if($fileName-match'^(.*)_tests\.rs$'){
            $companion=(Join-Path ([IO.Path]::GetDirectoryName($_)) "$($Matches[1]).rs").Replace('\','/')
            return $reachableProduction.ContainsKey($companion)
        }
        return $false
    })
    $orphans=@($relevantCandidates|Where-Object{-not$included.ContainsKey($_)})
    if($orphans.Count){throw "Orphan engine test files are not included by any test module: $($orphans -join ', ')"}
    $counts=[ordered]@{'ci-unit-tests-1'=0;'ci-unit-tests-2'=0;'ci-unit-tests-3'=0;'ci-unit-tests-4'=0}
    foreach($relative in $included.Keys|Sort-Object){$lines=[IO.File]::ReadAllLines((Join-Path $Root $relative));for($lineIndex=0;$lineIndex-lt$lines.Length;$lineIndex++){if($lines[$lineIndex].Trim()-ne'#[test]'){continue};$attributes=Get-AttributeContext $lines $lineIndex;$invalid=@([regex]::Matches($attributes,'feature\s*=\s*"(ci-unit-tests-[^"]+)"')|ForEach-Object{$_.Groups[1].Value}|Where-Object{$_-notmatch'^ci-unit-tests-[1-4]$'});if($invalid.Count){throw "${relative}:$($lineIndex+1): invalid CI partition cfg: $($invalid -join ', ')"};$direct=@(Get-PartitionFeatures $attributes);if($direct.Count-and@($included[$relative]).Count-and($direct[0]-notin@($included[$relative]))){throw "${relative}:$($lineIndex+1): test partition $($direct[0]) is excluded by its module partition(s) $(@($included[$relative]) -join ', ')"};$effective=@(if($direct.Count){$direct}else{@($included[$relative])});if($effective.Count-ne1){throw "${relative}:$($lineIndex+1): test must resolve to exactly one CI partition; found $($effective -join ', ')"};$counts[$effective[0]]++}}
    return [pscustomobject]@{Counts=$counts;Files=@($included.Keys|Sort-Object)}
}

function Add-PartitionedTests {
    param(
        [string]$RelativePath,
        [string[]]$AllowedFeatures
    )

    $path = Join-Path $repoRoot $RelativePath
    $lines = [System.IO.File]::ReadAllLines($path)
    for ($lineIndex = 0; $lineIndex -lt $lines.Length; $lineIndex++) {
        if ($lines[$lineIndex].Trim() -ne "#[test]") {
            continue
        }

        $feature = $null
        for ($attributeIndex = $lineIndex - 1; $attributeIndex -ge 0; $attributeIndex--) {
            $attribute = $lines[$attributeIndex].Trim()
            if (-not $attribute.StartsWith("#[")) {
                break
            }
            if ($attribute -match 'feature = "(ci-unit-tests-[1-4])"') {
                $feature = $Matches[1]
            }
        }

        if ($null -eq $feature) {
            throw "${RelativePath}:$($lineIndex + 1): test is missing a CI partition cfg"
        }
        if ($feature -notin $AllowedFeatures) {
            throw "${RelativePath}:$($lineIndex + 1): unexpected partition $feature"
        }
        $partitionCounts[$feature]++
    }
}

$coreTestModules = @(
    "src/agent_protocol.rs",
    "src/analytics.rs",
    "src/card_catalog.rs",
    "src/game_rules.rs",
    "src/history_queue.rs",
    "src/http.rs",
    "src/local_app.rs",
    "src/model.rs",
    "src/observation_delta.rs",
    "src/pilot_catalog.rs",
    "src/punching_bag.rs",
    "src/remote_ai.rs",
    "src/session.rs"
)

$allExpectedModules = @($coreTestModules)
$discoveredModules = Get-ChildItem (Join-Path $repoRoot "src") -Recurse -Filter "*.rs" |
    Where-Object {
        [System.IO.File]::ReadAllText($_.FullName) -match '(?m)^\s*mod tests\s*\{'
    } |
    ForEach-Object {
        $_.FullName.Substring($repoRoot.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
    }

$unexpectedModules = @($discoveredModules | Where-Object { $_ -notin $allExpectedModules })
$missingModules = @($allExpectedModules | Where-Object { $_ -notin $discoveredModules })
if ($unexpectedModules.Count -gt 0 -or $missingModules.Count -gt 0) {
    throw "Unit-test module inventory changed. Unexpected: $($unexpectedModules -join ', '); missing: $($missingModules -join ', ')"
}

$coreTestCount = 0
foreach ($relativePath in $coreTestModules) {
    $content = [System.IO.File]::ReadAllText((Join-Path $repoRoot $relativePath))
    if (-not $content.Contains('feature = "ci-unit-tests-1"')) {
        throw "${relativePath}: test module is not assigned to ci-unit-tests-1"
    }
    $coreTestCount += [regex]::Matches($content, '(?m)^\s*#\[test\]\s*$').Count
}
$partitionCounts["ci-unit-tests-1"] += $coreTestCount

$engineInventory=Get-EngineDedicatedTestInventory $repoRoot
if($engineInventory.Files.Count-ne$expectedDedicatedEngineTestFileCount){throw "Dedicated engine test file inventory contains $($engineInventory.Files.Count) files; expected $expectedDedicatedEngineTestFileCount"}
foreach($feature in @($partitionCounts.Keys)){$partitionCounts[$feature]+=[int]$engineInventory.Counts[$feature]}
$countMismatches = @()
foreach ($feature in $expectedPartitionCounts.Keys) {
    if ($partitionCounts[$feature] -ne $expectedPartitionCounts[$feature]) {
        $countMismatches += "$feature contains $($partitionCounts[$feature]) tests; expected $($expectedPartitionCounts[$feature])"
    }
    if (-not (Select-String -Path (Join-Path $repoRoot "Cargo.toml") -SimpleMatch "$feature =" -Quiet)) {
        throw "Cargo.toml does not declare $feature"
    }
}
if ($countMismatches.Count -ne 0) {
    throw ($countMismatches -join '; ')
}

$total = ($partitionCounts.Values | Measure-Object -Sum).Sum
$summary = $partitionCounts.Values -join ", "
Write-Host "Unit-test partitions verified: $total tests ($summary); $($engineInventory.Files.Count) dedicated engine files included."
