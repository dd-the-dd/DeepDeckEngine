<#
.SYNOPSIS
Builds a local work queue alternating recent sets, the priority pile and league decks.
.DESCRIPTION
Does not download data. Keeps the historical priority queue and proof manifest intact.
Rows already listed in the proof manifest can be omitted from planning; this does not
recertify their proofs or change the independent strict-prefix regression.
#>
[CmdletBinding()]
param(
    [string] $OracleBulkGzip = '.tmp/oracle-cards.jsonl.gz',
    [string] $PriorityQueue = 'docs/played-card-migration-priority-expanded-2026-11-09.json',
    [string] $LeagueSnapshot = '.tmp/card-migration-cache/league/public-deck-cards.json',
    [string] $ProofManifest = 'docs/native-card-proof-manifest.json',
    [string] $OutputPath = '.tmp/card-migration-cache/rotating-migration-queue.json',
    [datetime] $AsOfDate = '2026-11-12',
    [ValidateRange(1, 100000)] [int] $Limit = 90,
    [switch] $IncludeManifestCards
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
function Resolve-Local([string] $Path) {
    if ([IO.Path]::IsPathRooted($Path)) { return $Path }
    return Join-Path $repoRoot $Path
}
function Read-LocalJson([string] $Path) {
    $absolute = Resolve-Local $Path
    if (-not [IO.File]::Exists($absolute)) { throw "Required local input is missing: $absolute" }
    return [IO.File]::ReadAllText($absolute) | ConvertFrom-Json
}
$priority = Read-LocalJson $PriorityQueue
$league = Read-LocalJson $LeagueSnapshot
if ($league.schema_version -ne 'deepdeck-league-migration-source/v1' -or -not $league.complete) {
    throw 'Use a complete local public-deck snapshot.'
}
$manifest = Read-LocalJson $ProofManifest
$listed = [Collections.Generic.HashSet[string]]::new()
foreach ($card in @($manifest.cards)) { [void]$listed.Add([string]$card.oracle_id) }
$local = @{}
$stream = [IO.File]::OpenRead((Resolve-Local $OracleBulkGzip))
try {
    $gzip = [IO.Compression.GzipStream]::new($stream, [IO.Compression.CompressionMode]::Decompress)
    $reader = [IO.StreamReader]::new($gzip)
    while ($null -ne ($line = $reader.ReadLine())) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $card = $line | ConvertFrom-Json
        $oracleId = [string]$card.oracle_id
        if ([string]::IsNullOrWhiteSpace($oracleId) -or $local.ContainsKey($oracleId)) {
            throw 'The local archive must contain one record per Oracle identity.'
        }
        $local[$oracleId] = [pscustomobject]@{
            oracle_id = $oracleId; name = [string]$card.name
            set_code = [string]$card.PSObject.Properties['set'].Value
            collector_number = [string]$card.collector_number
            released_at = [string]$card.released_at
            set_type = [string]$card.set_type; layout = [string]$card.layout
        }
    }
} finally {
    if ($reader) { $reader.Dispose() }
    $stream.Dispose()
}

function Get-PlanningCard($Row) {
    $id = [string]$Row.oracle_id
    if ($local.ContainsKey($id)) { return $local[$id] }
    return [pscustomobject]@{
        oracle_id = $id; name = [string]$Row.name; set_code = $null
        collector_number = ''; released_at = ''; set_type = ''; layout = ''
    }
}
function Is-Pending($Card) {
    return $IncludeManifestCards -or -not $listed.Contains([string]$Card.oracle_id)
}
function Collector-Key([string] $Value) {
    $number = 0L
    if ([long]::TryParse($Value, [ref]$number)) { return '{0:D12}' -f $number }
    return "999999999999:$Value"
}
$dateKey = $AsOfDate.ToString('yyyy-MM-dd')
$recent = @($local.Values | Where-Object {
    $_.set_type -in @('expansion', 'core', 'draft_innovation', 'commander') -and
    $_.released_at -and $_.released_at -le $dateKey -and (Is-Pending $_)
} | Sort-Object @{Expression='released_at';Descending=$true}, set_code, @{Expression={Collector-Key $_.collector_number}})
$popular = @($priority.queue | Sort-Object global_rank | ForEach-Object { Get-PlanningCard $_ } | Where-Object { Is-Pending $_ })
$site = @($league.cards | Where-Object played_deck_count -gt 0 | ForEach-Object { Get-PlanningCard $_ } | Where-Object { Is-Pending $_ })
$lanes = [ordered]@{ recent = $recent; priority = $popular; league = $site }
$positions = @{ recent=0; priority=0; league=0 }
$priorityById = @{}
foreach ($row in @($priority.queue)) { $priorityById[[string]$row.oracle_id] = $row }
$leagueById = @{}
foreach ($row in @($league.cards)) { $leagueById[[string]$row.oracle_id] = $row }
$recentIds = [Collections.Generic.HashSet[string]]::new()
foreach ($row in $recent) { [void]$recentIds.Add([string]$row.oracle_id) }
$seen = [Collections.Generic.HashSet[string]]::new()
$queue = [Collections.Generic.List[object]]::new()
do {
    $advanced = $false
    foreach ($lane in @('recent', 'priority', 'league')) {
        while ($positions[$lane] -lt $lanes[$lane].Count) {
            $card = $lanes[$lane][$positions[$lane]]
            $positions[$lane]++
            $id = [string]$card.oracle_id
            if (-not $seen.Add($id)) { continue }
            $sources = [Collections.Generic.List[string]]::new()
            if ($recentIds.Contains($id)) { $sources.Add('recent') }
            if ($priorityById.ContainsKey($id)) { $sources.Add('priority') }
            if ($leagueById.ContainsKey($id) -and $leagueById[$id].played_deck_count -gt 0) { $sources.Add('league') }
            $queue.Add([pscustomobject][ordered]@{
                global_rank = $queue.Count + 1; oracle_id = $id; name = $card.name
                selected_source = $lane; sources = @($sources); set_code = $card.set_code
                collector_number = $card.collector_number; local_oracle_available = $local.ContainsKey($id)
                priority_rank = if ($priorityById.ContainsKey($id)) { $priorityById[$id].global_rank } else { $null }
                played_deck_count = if ($leagueById.ContainsKey($id)) { $leagueById[$id].played_deck_count } else { 0 }
                proof_manifest_listed = $listed.Contains($id)
            })
            $advanced = $true
            break
        }
        if ($queue.Count -ge $Limit) { break }
    }
} while ($advanced -and $queue.Count -lt $Limit)

$output = [ordered]@{
    schema_version = 'deepdeck-rotating-migration-queue/v1'
    generated_at_utc = [datetime]::UtcNow.ToString('o'); as_of_date = $dateKey
    source_cycle = @('recent','priority','league'); local_first = $true
    historical_priority_queue_preserved = $true; planning_only = $true
    inputs = @{ oracle_bulk=$OracleBulkGzip; priority_queue=$PriorityQueue; league_snapshot=$LeagueSnapshot; proof_manifest=$ProofManifest }
    source_counts = @{ local_oracle_cards=$local.Count; recent=$recent.Count; priority=$popular.Count; league=$site.Count }
    source_cursors = $positions
    queue = @($queue)
}
$absoluteOutput = Resolve-Local $OutputPath
[void][IO.Directory]::CreateDirectory((Split-Path -Parent $absoluteOutput))
[IO.File]::WriteAllText($absoluteOutput, (ConvertTo-Json -InputObject $output -Depth 15), [Text.UTF8Encoding]::new($false))
Write-Output "Local rotating queue: $($queue.Count) cards; source order recent / priority / league."
$queue | Select-Object -First 12 global_rank, selected_source, name, set_code, priority_rank | Format-Table -AutoSize
