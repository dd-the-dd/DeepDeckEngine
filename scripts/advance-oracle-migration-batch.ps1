<#
.SYNOPSIS
Advances a fixed Oracle migration batch while retaining every unresolved card.

.DESCRIPTION
Reads one rotating queue, an index-aligned parser audit JSONL, and the matching
Oracle card array. An optional previous batch plus its fresh audit overlays the
older queue audit. Every still-non-native previous card is retained; vacancies
are filled in rotating queue order and within the configured provenance quotas.

All reads and writes are explicit UTF-8. Queue, audit, and Oracle identities are
checked at every index before selection. The output records hashes of every
input so subsequent before/after audits can prove that they used the same data.

.EXAMPLE
.\scripts\advance-oracle-migration-batch.ps1 `
  -QueuePath .tmp/card-migration-cache/rotating-migration-queue-450.json `
  -QueueAuditPath .tmp/rotating-migration-queue-450-abilities-current.jsonl `
  -OracleInputPath .tmp/rotating-migration-queue-450-input.json `
  -PreviousBatchPath .tmp/rotating-next-100-nonnative.json `
  -PreviousBatchAuditPath .tmp/rotating-next-100-after-current.jsonl `
  -OutputPath .tmp/rotating-next-100-round2.json `
  -CardInputOutputPath .tmp/rotating-next-100-round2-input.json
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string] $QueuePath,
    [Parameter(Mandatory = $true)][string] $QueueAuditPath,
    [Parameter(Mandatory = $true)][string] $OracleInputPath,
    [string] $PreviousBatchPath,
    [string] $PreviousBatchAuditPath,
    [Parameter(Mandatory = $true)][string] $OutputPath,
    [Parameter(Mandatory = $true)][string] $CardInputOutputPath,
    [ValidateRange(1, 100)][int] $BatchSize = 100,
    [hashtable] $SourceTargets = @{ recent = 34; priority = 33; league = 33 }
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$utf8 = [Text.Encoding]::UTF8
$utf8NoBom = [Text.UTF8Encoding]::new($false)

function Resolve-InputPath([string] $Path) {
    $absolute = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $repoRoot $Path }
    $absolute = [IO.Path]::GetFullPath($absolute)
    if (-not [IO.File]::Exists($absolute)) { throw "Required input is missing: $absolute" }
    return $absolute
}

function Resolve-OutputPath([string] $Path) {
    $absolute = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $repoRoot $Path }
    $absolute = [IO.Path]::GetFullPath($absolute)
    $directory = Split-Path -Parent $absolute
    if (-not [IO.Directory]::Exists($directory)) { [void][IO.Directory]::CreateDirectory($directory) }
    return $absolute
}

function Read-JsonDocument([string] $Path) {
    return [IO.File]::ReadAllText($Path, $utf8) | ConvertFrom-Json
}

function Read-JsonLines([string] $Path) {
    $rows = [Collections.Generic.List[object]]::new()
    foreach ($line in [IO.File]::ReadLines($Path, $utf8)) {
        if (-not [string]::IsNullOrWhiteSpace($line)) { $rows.Add(($line | ConvertFrom-Json)) }
    }
    return @($rows)
}

function Get-OracleId($Value, [string] $Label) {
    $id = [string]$Value.oracle_id
    if ([string]::IsNullOrWhiteSpace($id)) { throw "$Label has no oracle_id." }
    return $id
}

function Test-NativeComplete($AuditCard) {
    if ([string]$AuditCard.status -ne 'canonical') { return $false }
    $blockers = @($AuditCard.abilities | Where-Object {
        [string]$_.status -ne 'canonical' -or [string]$_.recognizer -ne 'native'
    })
    return $blockers.Count -eq 0
}

function Assert-UniqueOracleIds($Values, [string] $Label) {
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($value in @($Values)) {
        $id = Get-OracleId $value $Label
        if (-not $seen.Add($id)) { throw "$Label contains duplicate oracle_id '$id'." }
    }
}

$QueuePath = Resolve-InputPath $QueuePath
$QueueAuditPath = Resolve-InputPath $QueueAuditPath
$OracleInputPath = Resolve-InputPath $OracleInputPath
$OutputPath = Resolve-OutputPath $OutputPath
$CardInputOutputPath = Resolve-OutputPath $CardInputOutputPath
if ([string]::IsNullOrWhiteSpace($PreviousBatchPath) -ne
    [string]::IsNullOrWhiteSpace($PreviousBatchAuditPath)) {
    throw 'PreviousBatchPath and PreviousBatchAuditPath must be supplied together.'
}
if ($PreviousBatchPath) {
    $PreviousBatchPath = Resolve-InputPath $PreviousBatchPath
    $PreviousBatchAuditPath = Resolve-InputPath $PreviousBatchAuditPath
}

$inputPaths = @($QueuePath, $QueueAuditPath, $OracleInputPath,
    $PreviousBatchPath, $PreviousBatchAuditPath) | Where-Object { $_ }
if ($OutputPath -ieq $CardInputOutputPath) {
    throw 'Batch and Oracle-card outputs must be different files.'
}
foreach ($destination in @($OutputPath, $CardInputOutputPath)) {
    if ($destination -in $inputPaths) {
        throw "Output must not overwrite a cohort input: $destination"
    }
}

$queueDocument = Read-JsonDocument $QueuePath
$queue = @($queueDocument.queue | ForEach-Object { $_ })
$queueAudit = @(Read-JsonLines $QueueAuditPath)
$oracleValue = Read-JsonDocument $OracleInputPath
$oracleCards = @($oracleValue | ForEach-Object { $_ })
if ($queue.Count -ne $queueAudit.Count -or $queue.Count -ne $oracleCards.Count) {
    throw "Queue, audit, and Oracle input cardinalities differ: $($queue.Count)/$($queueAudit.Count)/$($oracleCards.Count)."
}
Assert-UniqueOracleIds $queue 'Queue'
Assert-UniqueOracleIds $queueAudit 'Queue audit'
Assert-UniqueOracleIds $oracleCards 'Oracle input'

$queueIndexById = @{}
for ($index = 0; $index -lt $queue.Count; $index++) {
    $queueId = Get-OracleId $queue[$index] "Queue row $index"
    $auditId = Get-OracleId $queueAudit[$index] "Queue audit row $index"
    $oracleId = Get-OracleId $oracleCards[$index] "Oracle input row $index"
    if ($queueId -cne $auditId -or $queueId -cne $oracleId) {
        throw "oracle_id alignment mismatch at index $index`: queue=$queueId audit=$auditId oracle=$oracleId."
    }
    $queueIndexById[$queueId] = $index
}

$effectiveAuditById = @{}
foreach ($card in $queueAudit) { $effectiveAuditById[(Get-OracleId $card 'Queue audit')] = $card }
$previousCards = @()
$previousAudit = @()
if ($PreviousBatchPath) {
    $previousDocument = Read-JsonDocument $PreviousBatchPath
    $previousCards = @($previousDocument.cards | ForEach-Object { $_ })
    $previousAudit = @(Read-JsonLines $PreviousBatchAuditPath)
    if ($previousCards.Count -ne $previousAudit.Count) {
        throw "Previous batch and fresh audit cardinalities differ: $($previousCards.Count)/$($previousAudit.Count)."
    }
    Assert-UniqueOracleIds $previousCards 'Previous batch'
    Assert-UniqueOracleIds $previousAudit 'Previous batch audit'
    for ($index = 0; $index -lt $previousCards.Count; $index++) {
        $batchId = Get-OracleId $previousCards[$index] "Previous batch row $index"
        $auditId = Get-OracleId $previousAudit[$index] "Previous batch audit row $index"
        if ($batchId -cne $auditId) {
            throw "Previous batch oracle_id mismatch at index $index`: batch=$batchId audit=$auditId."
        }
        if (-not $queueIndexById.ContainsKey($batchId)) {
            throw "Previous batch oracle_id '$batchId' is absent from the rotating queue."
        }
        $effectiveAuditById[$batchId] = $previousAudit[$index]
    }
}

$targetTotal = 0
foreach ($source in $SourceTargets.Keys) {
    $target = [int]$SourceTargets[$source]
    if ($target -lt 0) { throw "Source target '$source' must be nonnegative." }
    $targetTotal += $target
}
if ($targetTotal -ne $BatchSize) {
    throw "Source targets total $targetTotal, expected BatchSize $BatchSize."
}

$selectedIds = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$counts = @{}
foreach ($source in $SourceTargets.Keys) { $counts[$source] = 0 }
$retainedIds = [Collections.Generic.List[string]]::new()
foreach ($previous in $previousCards) {
    $id = Get-OracleId $previous 'Previous batch'
    if (Test-NativeComplete $effectiveAuditById[$id]) { continue }
    $row = $queue[$queueIndexById[$id]]
    $source = [string]$row.selected_source
    if (-not $SourceTargets.ContainsKey($source)) { throw "Unknown selected_source '$source' for $id." }
    if ($counts[$source] -ge [int]$SourceTargets[$source]) {
        throw "Retained cards exceed the '$source' target; refusing to discard unresolved work."
    }
    [void]$selectedIds.Add($id)
    $retainedIds.Add($id)
    $counts[$source]++
}

for ($index = 0; $index -lt $queue.Count -and $selectedIds.Count -lt $BatchSize; $index++) {
    $row = $queue[$index]
    $id = Get-OracleId $row "Queue row $index"
    if ($selectedIds.Contains($id) -or (Test-NativeComplete $effectiveAuditById[$id])) { continue }
    $source = [string]$row.selected_source
    if (-not $SourceTargets.ContainsKey($source)) { throw "Unknown selected_source '$source' for $id." }
    if ($counts[$source] -ge [int]$SourceTargets[$source]) { continue }
    [void]$selectedIds.Add($id)
    $counts[$source]++
}

if ($selectedIds.Count -ne $BatchSize) {
    throw "Only $($selectedIds.Count)/$BatchSize unresolved cards satisfy the provenance targets."
}
foreach ($source in $SourceTargets.Keys) {
    if ($counts[$source] -ne [int]$SourceTargets[$source]) {
        throw "Selected $($counts[$source]) '$source' cards, expected $($SourceTargets[$source])."
    }
}
foreach ($id in $retainedIds) {
    if (-not $selectedIds.Contains($id)) { throw "Unresolved previous card '$id' was not retained." }
}

$selectedRows = [Collections.Generic.List[object]]::new()
$selectedOracle = [Collections.Generic.List[object]]::new()
for ($index = 0; $index -lt $queue.Count; $index++) {
    $row = $queue[$index]
    $id = Get-OracleId $row "Queue row $index"
    if (-not $selectedIds.Contains($id)) { continue }
    $result = $effectiveAuditById[$id]
    $selectedRows.Add([pscustomobject][ordered]@{
        batch_rank = $selectedRows.Count + 1
        rotating_global_rank = $row.global_rank
        selected_source = [string]$row.selected_source
        sources = @($row.sources)
        oracle_id = $id
        name = [string]$result.name
        set = [string]$result.set
        collector_number = [string]$result.collector_number
        parse_status = [string]$result.status
        abilities = @($result.abilities)
    })
    $selectedOracle.Add($oracleCards[$index])
}

$baselineHashes = [ordered]@{
    queue_sha256 = (Get-FileHash -LiteralPath $QueuePath -Algorithm SHA256).Hash
    queue_audit_sha256 = (Get-FileHash -LiteralPath $QueueAuditPath -Algorithm SHA256).Hash
    oracle_input_sha256 = (Get-FileHash -LiteralPath $OracleInputPath -Algorithm SHA256).Hash
    previous_batch_sha256 = if ($PreviousBatchPath) { (Get-FileHash -LiteralPath $PreviousBatchPath -Algorithm SHA256).Hash } else { $null }
    previous_batch_audit_sha256 = if ($PreviousBatchAuditPath) { (Get-FileHash -LiteralPath $PreviousBatchAuditPath -Algorithm SHA256).Hash } else { $null }
}
$document = [ordered]@{
    schema_version = 'deepdeck-rotating-nonnative-batch/v3'
    generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
    selection = 'retain every unresolved previous card, then fill provenance quotas in rotating global order'
    batch_size = $BatchSize
    retained_count = $retainedIds.Count
    added_count = $BatchSize - $retainedIds.Count
    source_counts = $counts
    baseline_hashes = $baselineHashes
    cards = @($selectedRows)
}
[IO.File]::WriteAllText($OutputPath, ($document | ConvertTo-Json -Depth 100), $utf8NoBom)
[IO.File]::WriteAllText($CardInputOutputPath, (@($selectedOracle) | ConvertTo-Json -Depth 100), $utf8NoBom)
Write-Output "Advanced batch: retained=$($retainedIds.Count) added=$($BatchSize-$retainedIds.Count) total=$BatchSize output=$OutputPath"
