<#
.SYNOPSIS
Caches the public league deck-card list for local migration planning.
.DESCRIPTION
Reuses the local snapshot unless Refresh is requested. Uses public GET requests
only; it neither publishes changes nor requires account credentials.
#>
[CmdletBinding()]
param(
    [string] $SnapshotPath = '.tmp/card-migration-cache/league/public-deck-cards.json',
    [string] $BaseUri = 'https://www.deepdeckleague.com/api/v1',
    [switch] $Refresh
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($SnapshotPath)) { $SnapshotPath = Join-Path $repoRoot $SnapshotPath }
if ((Test-Path -LiteralPath $SnapshotPath -PathType Leaf) -and -not $Refresh) {
    $cached = [IO.File]::ReadAllText($SnapshotPath) | ConvertFrom-Json
    if ($cached.schema_version -ne 'deepdeck-league-migration-source/v1' -or -not $cached.complete) {
        throw 'The cached league snapshot is incomplete or has an unsupported schema.'
    }
    Write-Output "Local league snapshot: $($cached.deck_count) decks, $($cached.cards.Count) unique cards."
    exit 0
}

function Get-PublicData([string] $Uri) {
    $response = Invoke-RestMethod -Method Get -Uri $Uri -TimeoutSec 20 -Headers @{
        'User-Agent' = 'DeepDeck local migration audit/1.0'
        Accept = 'application/json'
    }
    if ($null -eq $response.data) { throw "Public response omitted data: $Uri" }
    return $response.data
}

$decks = [Collections.Generic.List[object]]::new()
$seenDecks = [Collections.Generic.HashSet[string]]::new()
$page = 1
do {
    $data = Get-PublicData "$BaseUri/decks?page=$page&page_size=100"
    if ($null -eq $data.pagination -or $null -eq $data.items) { throw 'Deck pagination is missing.' }
    foreach ($deck in @($data.items)) {
        if (-not $seenDecks.Add([string]$deck.id)) { throw "Duplicate deck version: $($deck.id)" }
        $decks.Add($deck)
    }
    $page++
} while ($page -le [int]$data.pagination.totalPages)
if ($decks.Count -ne [int]$data.pagination.totalItems) { throw 'Deck pagination changed; retry the snapshot.' }

$cards = @{}
$deckSources = [Collections.Generic.List[object]]::new()
$playableSections = @('main', 'commander', 'sideboard', 'companion')
$index = 0
foreach ($deck in $decks) {
    $uri = "$BaseUri/decks/$($deck.id)"
    $detail = Get-PublicData $uri
    $deckSources.Add([ordered]@{ version_id = $deck.id; deck_id = $deck.deckId; name = $deck.name; format = $deck.format; uri = $uri })
    foreach ($entry in @($detail.cards)) {
        $oracleId = [string]$entry.oracleId
        if ([string]::IsNullOrWhiteSpace($oracleId)) { throw "Card without oracleId in deck $($deck.id)." }
        $section = [string]$entry.section
        if ($section -ne 'considering' -and $section -notin $playableSections) { throw "Unknown deck section: $section" }
        if (-not $cards.ContainsKey($oracleId)) {
            $cards[$oracleId] = [ordered]@{
                oracle_id = $oracleId; name = [string]$entry.name
                played_deck_versions = [Collections.Generic.HashSet[string]]::new()
                considering_deck_versions = [Collections.Generic.HashSet[string]]::new()
                quantity = 0
            }
        }
        if ($section -eq 'considering') {
            [void]$cards[$oracleId].considering_deck_versions.Add([string]$deck.id)
        } else {
            [void]$cards[$oracleId].played_deck_versions.Add([string]$deck.id)
            $cards[$oracleId].quantity += [int]$entry.quantity
        }
    }
    $index++
    if ($index % 10 -eq 0 -or $index -eq $decks.Count) { Write-Output "Read public deck $index/$($decks.Count)." }
}
$rows = @(foreach ($value in $cards.Values) {
    [pscustomobject][ordered]@{
        oracle_id = $value.oracle_id; name = $value.name
        played_deck_count = $value.played_deck_versions.Count
        considering_deck_count = $value.considering_deck_versions.Count
        quantity = $value.quantity
        played_deck_versions = @($value.played_deck_versions | Sort-Object)
        considering_deck_versions = @($value.considering_deck_versions | Sort-Object)
    }
}) | Sort-Object @{ Expression = 'played_deck_count'; Descending = $true }, @{ Expression = 'quantity'; Descending = $true }, name
$snapshot = [ordered]@{
    schema_version = 'deepdeck-league-migration-source/v1'
    generated_at_utc = [datetime]::UtcNow.ToString('o')
    complete = $true; source_uri = "$BaseUri/decks"; deck_count = $decks.Count
    playable_sections = $playableSections; decks = @($deckSources); cards = @($rows)
}
$parent = Split-Path -Parent $SnapshotPath
[void][IO.Directory]::CreateDirectory($parent)
$temporary = "$SnapshotPath.$PID.tmp"
[IO.File]::WriteAllText($temporary, (ConvertTo-Json -InputObject $snapshot -Depth 15), [Text.UTF8Encoding]::new($false))
if ([IO.File]::Exists($SnapshotPath)) { [IO.File]::Replace($temporary, $SnapshotPath, $null) }
else { [IO.File]::Move($temporary, $SnapshotPath) }
Write-Output "Cached $($decks.Count) public decks; $($rows.Count) unique cards; $(@($rows | Where-Object played_deck_count -gt 0).Count) in playable sections."
