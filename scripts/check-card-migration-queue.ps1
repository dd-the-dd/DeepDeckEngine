[CmdletBinding()]
param(
    [string] $SeedQueue = 'docs/played-card-migration-priority-2026-11-09.json',
    [string] $PopularQueue = 'docs/played-card-migration-priority-expanded-2026-11-09.json',
    [string] $ExhaustiveQueue = '.tmp/card-migration-cache/played-card-migration-exhaustive-2026-11-09.json'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

function Resolve-RepoPath([string] $Path) {
    if ([IO.Path]::IsPathRooted($Path)) { return $Path }
    return Join-Path $repoRoot $Path
}

function Read-QueueDocument([string] $Path, [string] $Label) {
    $resolved = Resolve-RepoPath $Path
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw "$Label queue not found: $resolved"
    }
    $document = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json
    if ($null -eq $document.queue) {
        throw "$Label document omitted queue."
    }
    return $document
}

function Assert-ContinuousUniqueQueue($Rows, [string] $Label) {
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    for ($index = 0; $index -lt $Rows.Count; $index++) {
        $row = $Rows[$index]
        $expectedRank = $index + 1
        if ([int]$row.global_rank -ne $expectedRank) {
            throw "$Label rank discontinuity at index ${index}: expected $expectedRank, found $($row.global_rank)."
        }
        $oracleId = [string]$row.oracle_id
        if ([string]::IsNullOrWhiteSpace($oracleId)) {
            throw "$Label row $expectedRank omitted oracle_id."
        }
        if (-not $seen.Add($oracleId)) {
            throw "$Label repeats oracle_id $oracleId at rank $expectedRank."
        }
    }
}

$seedDocument = Read-QueueDocument $SeedQueue 'Seed'
$popularDocument = Read-QueueDocument $PopularQueue 'Popular'
$exhaustiveDocument = Read-QueueDocument $ExhaustiveQueue 'Exhaustive'

$seed = @($seedDocument.queue | Sort-Object global_rank)
$popular = @($popularDocument.queue | Sort-Object global_rank)
$exhaustive = @($exhaustiveDocument.queue | Sort-Object global_rank)

Assert-ContinuousUniqueQueue $seed 'Seed'
Assert-ContinuousUniqueQueue $popular 'Popular'
Assert-ContinuousUniqueQueue $exhaustive 'Exhaustive'

if ($popular.Count -lt $seed.Count) {
    throw "Popular queue has $($popular.Count) rows but must preserve all $($seed.Count) seed rows."
}
for ($index = 0; $index -lt $seed.Count; $index++) {
    $expected = $seed[$index] | ConvertTo-Json -Depth 30 -Compress
    $actual = $popular[$index] | ConvertTo-Json -Depth 30 -Compress
    if ($actual -cne $expected) {
        throw "Popular queue changed seed row $($index + 1)."
    }
}

if ($exhaustive.Count -lt $popular.Count) {
    throw "Exhaustive queue has fewer rows ($($exhaustive.Count)) than the popular queue ($($popular.Count))."
}
for ($index = 0; $index -lt $popular.Count; $index++) {
    if ([string]$exhaustive[$index].oracle_id -cne [string]$popular[$index].oracle_id) {
        throw "Exhaustive queue does not preserve popular oracle_id at rank $($index + 1)."
    }
}

if ([int]$exhaustiveDocument.counts.queue_rows -ne $exhaustive.Count) {
    throw 'Exhaustive declared queue_rows does not match the generated queue.'
}
if ([int]$exhaustiveDocument.counts.oracle_cards -ne $exhaustive.Count) {
    throw 'Exhaustive queue does not contain exactly one row per declared Oracle card.'
}

$actualTierCounts = @{}
foreach ($row in $exhaustive) {
    $tier = [string]$row.priority_tier
    if ([string]::IsNullOrWhiteSpace($tier)) {
        throw "Exhaustive row $($row.global_rank) omitted priority_tier."
    }
    if (-not $actualTierCounts.ContainsKey($tier)) { $actualTierCounts[$tier] = 0 }
    $actualTierCounts[$tier]++
}
$declaredTierCounts = @($exhaustiveDocument.counts.tiers.PSObject.Properties)
if ($declaredTierCounts.Count -ne $actualTierCounts.Count) {
    throw 'Exhaustive declared tier set does not match the queue.'
}
foreach ($property in $declaredTierCounts) {
    if (-not $actualTierCounts.ContainsKey($property.Name) -or
        [int]$property.Value -ne [int]$actualTierCounts[$property.Name]) {
        throw "Exhaustive tier count mismatch for '$($property.Name)'."
    }
}

Write-Host (
    'Migration queues verified: {0} immutable seed rows, {1} popular rows, {2} exhaustive unique Oracle IDs.' -f
    $seed.Count, $popular.Count, $exhaustive.Count
) -ForegroundColor Green
