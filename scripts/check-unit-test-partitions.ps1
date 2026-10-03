$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$partitionCounts = [ordered]@{
    "ci-unit-tests-1" = 0
    "ci-unit-tests-2" = 0
    "ci-unit-tests-3" = 0
    "ci-unit-tests-4" = 0
}
$expectedPartitionCounts = [ordered]@{
    "ci-unit-tests-1" = 191
    "ci-unit-tests-2" = 214
    "ci-unit-tests-3" = 213
    "ci-unit-tests-4" = 100
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
    "src/engine/state.rs",
    "src/game_rules.rs",
    "src/history_queue.rs",
    "src/http.rs",
    "src/local_app.rs",
    "src/model.rs",
    "src/observation_delta.rs",
    "src/oracle/pipeline.rs",
    "src/pilot_catalog.rs",
    "src/punching_bag.rs",
    "src/remote_ai.rs",
    "src/session.rs"
)

$allExpectedModules = @($coreTestModules) + @(
    "src/engine/mod.rs",
    "src/oracle/canonical/mod.rs"
)
$discoveredModules = Get-ChildItem (Join-Path $repoRoot "src") -Recurse -Filter "*.rs" |
    Where-Object {
        [System.IO.File]::ReadAllText($_.FullName) -match '(?m)^\s*mod tests(?:\s*;|\s*\{)'
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

Add-PartitionedTests "src/engine/tests.rs" @(
    "ci-unit-tests-1",
    "ci-unit-tests-2",
    "ci-unit-tests-3",
    "ci-unit-tests-4"
)
Add-PartitionedTests "src/oracle/canonical/tests.rs" @(
    "ci-unit-tests-2",
    "ci-unit-tests-3"
)

foreach ($feature in $expectedPartitionCounts.Keys) {
    if ($partitionCounts[$feature] -ne $expectedPartitionCounts[$feature]) {
        throw "$feature contains $($partitionCounts[$feature]) tests; expected $($expectedPartitionCounts[$feature])"
    }
    if (-not (Select-String -Path (Join-Path $repoRoot "Cargo.toml") -SimpleMatch "$feature =" -Quiet)) {
        throw "Cargo.toml does not declare $feature"
    }
}

$total = ($partitionCounts.Values | Measure-Object -Sum).Sum
$summary = $partitionCounts.Values -join ", "
Write-Host "Unit-test partitions verified: $total tests ($summary)."
