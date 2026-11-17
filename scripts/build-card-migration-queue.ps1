[CmdletBinding()]
param(
    [string] $SeedQueue = 'docs/played-card-migration-priority-2026-11-09.json',
    [string] $PopularOutput = 'docs/played-card-migration-priority-expanded-2026-11-09.json',
    [string] $ExhaustiveOutput = '.tmp/card-migration-cache/played-card-migration-exhaustive-2026-11-09.json',
    [string] $OracleBulkGzip = '.tmp/card-migration-cache/source/oracle-cards.jsonl.gz',
    [string] $DefaultBulkGzip = '.tmp/card-migration-cache/source/default-cards.jsonl.gz',
    [ValidateRange(1, 20)]
    [int] $EdhrecPages = 5,
    [ValidateRange(500, 10000)]
    [int] $ScryfallEdhrecRankLimit = 2000,
    [string] $SnapshotDate = '2026-11-09',
    [string] $MarvelSetCode = 'msh',
    [switch] $PopularOnly,
    [switch] $RefreshSources
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

function Resolve-RepoPath([string] $Path) {
    if ([IO.Path]::IsPathRooted($Path)) { return $Path }
    return Join-Path $repoRoot $Path
}

function New-ParentDirectory([string] $Path) {
    $parent = Split-Path -Parent $Path
    if ($parent -and -not (Test-Path -LiteralPath $parent -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $parent -Force)
    }
}

function Get-PropertyValue($Object, [string] $Name) {
    if ($Object -is [Collections.IDictionary] -and $Object.Contains($Name)) {
        return $Object[$Name]
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Get-ScryfallBulkMetadata {
    $headers = @{
        'User-Agent' = 'DeepDeck migration priority/0.2 (+local development)'
        Accept = 'application/json;q=0.9,*/*;q=0.8'
    }
    return Invoke-RestMethod -Uri 'https://api.scryfall.com/bulk-data' -Headers $headers
}

function Save-RemoteFile([string] $Uri, [string] $Path) {
    New-ParentDirectory $Path
    $headers = @{
        'User-Agent' = 'DeepDeck migration priority/0.2 (+local development)'
        Accept = 'application/json;q=0.9,*/*;q=0.8'
    }
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -Headers $headers -OutFile $Path
}

function Ensure-ScryfallBulks {
    $oracleMissing = $RefreshSources -or -not (Test-Path -LiteralPath $script:OracleBulkGzip -PathType Leaf)
    $defaultMissing = $RefreshSources -or -not (Test-Path -LiteralPath $script:DefaultBulkGzip -PathType Leaf)
    if (-not $oracleMissing -and -not $defaultMissing) { return $null }

    Write-Host 'Fetching Scryfall bulk metadata...' -ForegroundColor Cyan
    $metadata = Get-ScryfallBulkMetadata
    foreach ($request in @(
        [pscustomobject]@{ Type = 'oracle_cards'; Missing = $oracleMissing; Path = $script:OracleBulkGzip },
        [pscustomobject]@{ Type = 'default_cards'; Missing = $defaultMissing; Path = $script:DefaultBulkGzip }
    )) {
        if (-not $request.Missing) { continue }
        $bulk = @($metadata.data | Where-Object type -eq $request.Type)
        if ($bulk.Count -ne 1) { throw "Scryfall bulk metadata omitted $($request.Type)." }
        $uri = [string](Get-PropertyValue $bulk[0] 'jsonl_download_uri')
        if ([string]::IsNullOrWhiteSpace($uri)) {
            $uri = [string](Get-PropertyValue $bulk[0] 'download_uri')
        }
        if ([string]::IsNullOrWhiteSpace($uri)) {
            throw "Scryfall bulk $($request.Type) omitted a download URI."
        }
        Write-Host "Downloading $($request.Type)..." -ForegroundColor Cyan
        Save-RemoteFile $uri $request.Path
    }
    return $metadata
}

function Read-GzipJsonLines([string] $Path, [scriptblock] $Visitor) {
    $file = [IO.File]::OpenRead($Path)
    $gzip = [IO.Compression.GZipStream]::new($file, [IO.Compression.CompressionMode]::Decompress)
    $reader = [IO.StreamReader]::new($gzip)
    try {
        while (-not $reader.EndOfStream) {
            $line = $reader.ReadLine()
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            & $Visitor ($line | ConvertFrom-Json)
        }
    }
    finally {
        $reader.Dispose()
        $gzip.Dispose()
        $file.Dispose()
    }
}

function Get-EdhrecTopCards([int] $Pages) {
    $results = [Collections.Generic.List[object]]::new()
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    # Each card component has one name button. DFCs have multiple face anchors,
    # so parsing anchors would count their faces as separate popularity rows.
    $pattern = '<button type="button" class="[^"]*_name">([^<]+)</button>'
    $headers = @{ 'User-Agent' = 'DeepDeck migration priority/0.2'; Accept = 'text/html' }
    for ($page = 1; $page -le $Pages; $page++) {
        $uri = "https://cloudflare.edhrec.com/top?page=$page"
        Write-Host "Reading EDHREC top cards page $page/$Pages..." -ForegroundColor Cyan
        $html = (Invoke-WebRequest -UseBasicParsing -Uri $uri -Headers $headers).Content
        $matches = [regex]::Matches($html, $pattern, [Text.RegularExpressions.RegexOptions]::Singleline)
        if ($matches.Count -lt 100) {
            throw "EDHREC page $page exposed only $($matches.Count) card rows; expected at least 100."
        }
        for ($index = 0; $index -lt 100; $index++) {
            $name = [Net.WebUtility]::HtmlDecode($matches[$index].Groups[1].Value).Trim()
            if (-not $seen.Add($name)) {
                throw "EDHREC top pages repeated '$name' before rank $(($page - 1) * 100 + $index + 1)."
            }
            [void]$results.Add([pscustomobject]@{
                Rank = (($page - 1) * 100 + $index + 1)
                Name = $name
                Uri = $uri
            })
        }
    }
    return @($results)
}

function Get-CollectorSortKey([string] $CollectorNumber) {
    if ($CollectorNumber -match '^(\d+)(.*)$') {
        return ('{0:D10}:{1}' -f [long]$Matches[1], $Matches[2].ToLowerInvariant())
    }
    return "9999999999:$($CollectorNumber.ToLowerInvariant())"
}

function Get-CatalogKind($Card) {
    $layout = ([string](Get-PropertyValue $Card 'layout')).ToLowerInvariant()
    $setType = ([string](Get-PropertyValue $Card 'set_type')).ToLowerInvariant()
    if ($layout -in @('token', 'double_faced_token', 'emblem') -or $setType -eq 'token') {
        return 'game_piece'
    }
    if ($layout -eq 'art_series' -or $setType -eq 'memorabilia') {
        return 'non_game_piece'
    }
    return 'playable_card'
}

function Get-PrintPriority($Print, [datetime] $MarvelRelease) {
    $setCode = ([string]$Print.set).ToLowerInvariant()
    $setName = [string]$Print.set_name
    $release = [datetime]::ParseExact([string]$Print.released_at, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
    $isMarvelFamily = $setCode -eq $script:MarvelSetCode -or $setName.StartsWith('Marvel Super Heroes', [StringComparison]::OrdinalIgnoreCase)
    if ($isMarvelFamily) {
        $familyOrder = if ($setCode -eq $script:MarvelSetCode) { 0 } elseif ($setCode -eq 'msc') { 1 } else { 2 }
        return [pscustomobject]@{
            Group = 0
            SetOrder = ('{0:D2}:{1}' -f $familyOrder, $setCode)
            ReleaseOrder = '0000-00-00'
        }
    }
    if ($release -lt $MarvelRelease) {
        $inverseTicks = [datetime]::MaxValue.Ticks - $release.Ticks
        return [pscustomobject]@{
            Group = 1
            SetOrder = $setCode
            ReleaseOrder = ('{0:D19}' -f $inverseTicks)
        }
    }
    return [pscustomobject]@{
        Group = 2
        SetOrder = $setCode
        ReleaseOrder = ([string]$Print.released_at)
    }
}

function Compare-PrintCandidate($Left, $Right) {
    foreach ($property in @('Group', 'ReleaseOrder', 'SetOrder', 'CollectorSortKey')) {
        $comparison = [string]::CompareOrdinal([string]$Left.$property, [string]$Right.$property)
        if ($comparison -ne 0) { return $comparison }
    }
    return [string]::CompareOrdinal([string]$Left.id, [string]$Right.id)
}

function Add-QueueRow(
    [Collections.Generic.List[object]] $Queue,
    [Collections.Generic.HashSet[string]] $SeenOracleIds,
    $Row
) {
    $oracleId = [string]$Row.oracle_id
    if ([string]::IsNullOrWhiteSpace($oracleId)) { throw 'Queue row omitted oracle_id.' }
    if (-not $SeenOracleIds.Add($oracleId)) { return $false }
    $Row.global_rank = $Queue.Count + 1
    [void]$Queue.Add($Row)
    return $true
}

$SeedQueue = Resolve-RepoPath $SeedQueue
$PopularOutput = Resolve-RepoPath $PopularOutput
$ExhaustiveOutput = Resolve-RepoPath $ExhaustiveOutput
$script:OracleBulkGzip = Resolve-RepoPath $OracleBulkGzip
$script:DefaultBulkGzip = Resolve-RepoPath $DefaultBulkGzip
$script:MarvelSetCode = $MarvelSetCode.ToLowerInvariant()

if (-not (Test-Path -LiteralPath $SeedQueue -PathType Leaf)) {
    throw "Seed queue not found: $SeedQueue"
}
New-ParentDirectory $PopularOutput
New-ParentDirectory $ExhaustiveOutput
$bulkMetadata = Ensure-ScryfallBulks

$seed = Get-Content -LiteralPath $SeedQueue -Raw | ConvertFrom-Json
$seedRows = @($seed.queue | Sort-Object global_rank)
if ($seedRows.Count -eq 0) { throw 'Seed queue is empty.' }
for ($index = 0; $index -lt $seedRows.Count; $index++) {
    if ([int]$seedRows[$index].global_rank -ne $index + 1) {
        throw "Seed queue rank discontinuity at index $index."
    }
}

Write-Host 'Indexing Scryfall Oracle cards...' -ForegroundColor Cyan
$oracleById = @{}
$oracleByName = @{}
Read-GzipJsonLines $script:OracleBulkGzip {
    param($card)
    $oracleId = [string]$card.oracle_id
    if ([string]::IsNullOrWhiteSpace($oracleId)) { return }
    $minimal = [pscustomobject]@{
        oracle_id = $oracleId
        name = [string]$card.name
        edhrec_rank = Get-PropertyValue $card 'edhrec_rank'
        legalities = $card.legalities
        catalog_kind = Get-CatalogKind $card
        layout = [string](Get-PropertyValue $card 'layout')
        set_type = [string](Get-PropertyValue $card 'set_type')
    }
    $oracleById[$oracleId] = $minimal
    $oracleByName[[string]$card.name] = $minimal
}

$edhrecRows = Get-EdhrecTopCards $EdhrecPages
$unresolvedEdhrec = [Collections.Generic.List[object]]::new()
$popularQueue = [Collections.Generic.List[object]]::new()
$popularSeen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($seedRow in $seedRows) {
    [void](Add-QueueRow $popularQueue $popularSeen $seedRow)
}
foreach ($entry in $edhrecRows | Where-Object Rank -gt 50) {
    if (-not $oracleByName.ContainsKey($entry.Name)) {
        [void]$unresolvedEdhrec.Add($entry)
        continue
    }
    $card = $oracleByName[$entry.Name]
    $row = [pscustomobject]@{
        global_rank = 0
        name = $card.name
        oracle_id = $card.oracle_id
        priority_score = $null
        priority_tier = 'edhrec_top_500_extension'
        formats = @([pscustomobject]@{
            format = 'commander'
            rank = $entry.Rank
            source_score = $null
            source = 'edhrec'
            source_date = "observed-$SnapshotDate"
        })
        legalities = [pscustomobject]@{
            standard = [string]$card.legalities.standard
            legacy = [string]$card.legalities.legacy
            commander = [string]$card.legalities.commander
        }
        source_format_legalities_verified = $true
    }
    [void](Add-QueueRow $popularQueue $popularSeen $row)
}

$popularDocument = [ordered]@{
    schema_version = '2.0'
    generated_at = $SnapshotDate
    purpose = 'Expanded high-play migration queue; the exhaustive local queue is generated separately.'
    seed_snapshot = [ordered]@{
        path = 'docs/played-card-migration-priority-2026-11-09.json'
        preserved_rows = $seedRows.Count
        preserved_order = $true
    }
    method = [ordered]@{
        seed = 'Preserve the original equal-format RRF top-50 queue exactly.'
        extension = 'Append public EDHREC Top Cards ranks 51-500 in published order, deduplicated by Scryfall oracle_id.'
        legacy_standard_limit = 'Moxou publicly exposes only the top 50; deeper ranks are gated and are not inferred.'
    }
    sources = @(
        [ordered]@{ id = 'seed-priority-snapshot'; observed_at = $SnapshotDate; path = 'docs/played-card-migration-priority-2026-11-09.json' },
        [ordered]@{ id = 'edhrec-top'; observed_at = $SnapshotDate; url = 'https://edhrec.com/top'; pages = $EdhrecPages; rows_per_page = 100 },
        [ordered]@{ id = 'scryfall-oracle-cards'; observed_at = $SnapshotDate; url = 'https://api.scryfall.com/bulk-data'; deduplication_key = 'oracle_id' }
    )
    counts = [ordered]@{
        seed_rows = $seedRows.Count
        edhrec_rows_observed = $edhrecRows.Count
        unique_priority_rows = $popularQueue.Count
        unresolved_edhrec_names = $unresolvedEdhrec.Count
    }
    unresolved_edhrec = @($unresolvedEdhrec)
    queue = @($popularQueue)
}
$popularDocument | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $PopularOutput -Encoding UTF8

if ($PopularOnly) {
    Write-Host "Popular queue: $($popularQueue.Count) rows -> $PopularOutput" -ForegroundColor Green
    return
}

Write-Host 'Selecting one deterministic print per Oracle card for set-order fallback...' -ForegroundColor Cyan
$marvelRelease = [datetime]::ParseExact('2026-06-26', 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
$bestPrintByOracle = @{}
Read-GzipJsonLines $script:DefaultBulkGzip {
    param($print)
    $oracleId = [string]$print.oracle_id
    if ([string]::IsNullOrWhiteSpace($oracleId) -or -not $oracleById.ContainsKey($oracleId)) { return }
    $priority = Get-PrintPriority $print $marvelRelease
    $candidate = [pscustomobject]@{
        id = [string]$print.id
        oracle_id = $oracleId
        set = ([string]$print.set).ToLowerInvariant()
        set_name = [string]$print.set_name
        released_at = [string]$print.released_at
        collector_number = [string]$print.collector_number
        CollectorSortKey = Get-CollectorSortKey ([string]$print.collector_number)
        Group = [int]$priority.Group
        ReleaseOrder = [string]$priority.ReleaseOrder
        SetOrder = [string]$priority.SetOrder
    }
    if (-not $bestPrintByOracle.ContainsKey($oracleId) -or (Compare-PrintCandidate $candidate $bestPrintByOracle[$oracleId]) -lt 0) {
        $bestPrintByOracle[$oracleId] = $candidate
    }
}

$queue = [Collections.Generic.List[object]]::new()
$seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($row in $popularQueue) {
    $copy = [ordered]@{
        global_rank = 0
        oracle_id = [string]$row.oracle_id
        name = [string]$row.name
        priority_tier = if ($null -ne (Get-PropertyValue $row 'priority_tier')) { [string]$row.priority_tier } else { 'seed_cross_format_top_50' }
        catalog_kind = [string]$oracleById[[string]$row.oracle_id].catalog_kind
    }
    [void](Add-QueueRow $queue $seen $copy)
}

$scryfallRanked = @($oracleById.Values | Where-Object {
    $rank = Get-PropertyValue $_ 'edhrec_rank'
    $null -ne $rank -and [int]$rank -le $ScryfallEdhrecRankLimit -and [string]$_.legalities.commander -eq 'legal'
} | Sort-Object @{ Expression = { [int]$_.edhrec_rank } }, name, oracle_id)
foreach ($card in $scryfallRanked) {
    $row = [ordered]@{
        global_rank = 0
        oracle_id = [string]$card.oracle_id
        name = [string]$card.name
        priority_tier = 'scryfall_edhrec_rank_extension'
        catalog_kind = [string]$card.catalog_kind
        commander_rank = [int]$card.edhrec_rank
    }
    [void](Add-QueueRow $queue $seen $row)
}

$fallbackRows = foreach ($card in $oracleById.Values) {
    if ($seen.Contains([string]$card.oracle_id)) { continue }
    $print = $bestPrintByOracle[[string]$card.oracle_id]
    if ($null -eq $print) {
        [pscustomobject]@{
            Card = $card; Print = $null; Group = 6; ReleaseOrder = '9999999999999999999'; SetOrder = ''; CollectorSortKey = ''
        }
    }
    else {
        $finalGroup = [int]$print.Group
        if ([string]$card.catalog_kind -eq 'game_piece') {
            $finalGroup = 4
        }
        elseif ([string]$card.catalog_kind -eq 'non_game_piece') {
            $finalGroup = 5
        }
        elseif ($finalGroup -eq 2 -and [datetime]::ParseExact([string]$print.released_at, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture) -gt [datetime]::ParseExact($SnapshotDate, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)) {
            $finalGroup = 3
        }
        [pscustomobject]@{
            Card = $card; Print = $print; Group = $finalGroup; ReleaseOrder = $print.ReleaseOrder; SetOrder = $print.SetOrder; CollectorSortKey = $print.CollectorSortKey
        }
    }
}
$fallbackRows = @($fallbackRows | Sort-Object Group, ReleaseOrder, SetOrder, CollectorSortKey, @{ Expression = { $_.Card.name } }, @{ Expression = { $_.Card.oracle_id } })
foreach ($entry in $fallbackRows) {
    $print = $entry.Print
    $tier = switch ([int]$entry.Group) {
        0 { 'marvel_super_heroes_family' }
        1 { 'older_sets_descending' }
        2 { 'post_cutoff_reconciliation' }
        3 { 'future_unreleased_reconciliation' }
        4 { 'game_pieces_after_cards' }
        5 { 'non_game_pieces_after_cards' }
        default { 'catalog_without_print' }
    }
    $row = [ordered]@{
        global_rank = 0
        oracle_id = [string]$entry.Card.oracle_id
        name = [string]$entry.Card.name
        priority_tier = $tier
        catalog_kind = [string]$entry.Card.catalog_kind
    }
    if ($null -ne $print) {
        $row.set = [string]$print.set
        $row.set_name = [string]$print.set_name
        $row.released_at = [string]$print.released_at
        $row.collector_number = [string]$print.collector_number
    }
    [void](Add-QueueRow $queue $seen $row)
}

if ($queue.Count -ne $oracleById.Count) {
    throw "Exhaustive queue has $($queue.Count) rows for $($oracleById.Count) Oracle IDs."
}
for ($index = 0; $index -lt $queue.Count; $index++) {
    if ([int]$queue[$index].global_rank -ne $index + 1) {
        throw "Exhaustive rank discontinuity at index $index."
    }
}

$tierCounts = [ordered]@{}
foreach ($row in $queue) {
    $tier = [string](Get-PropertyValue $row 'priority_tier')
    if ([string]::IsNullOrWhiteSpace($tier)) { throw 'Exhaustive queue row omitted priority_tier.' }
    if (-not $tierCounts.Contains($tier)) { $tierCounts[$tier] = 0 }
    $tierCounts[$tier] = 1 + [int]$tierCounts[$tier]
}
$tierTotal = 0
foreach ($value in $tierCounts.Values) { $tierTotal += [int]$value }
if ($tierCounts.Contains('')) {
    throw 'Exhaustive tier counts contain an empty key.'
}
if ($tierTotal -ne $queue.Count) {
    throw "Exhaustive tier counts sum to $tierTotal for $($queue.Count) rows."
}
$exhaustiveDocument = [ordered]@{
    schema_version = '1.0'
    generated_at = $SnapshotDate
    generated_from = [ordered]@{
        popular_queue = 'docs/played-card-migration-priority-expanded-2026-11-09.json'
        scryfall_bulk_endpoint = 'https://api.scryfall.com/bulk-data'
        oracle_bulk_path = $script:OracleBulkGzip
        default_bulk_path = $script:DefaultBulkGzip
    }
    method = [ordered]@{
        tiers = @(
            'Preserved cross-format top-50 seed',
            'EDHREC published top-500 extension',
            "Scryfall EDHREC ranks up to $ScryfallEdhrecRankLimit",
            'Marvel Super Heroes set family',
            'Sets older than Marvel Super Heroes, newest to oldest',
            'Post-cutoff reconciliation for released Oracle cards not represented in older sets',
            'Future unreleased cards, after released playable cards',
            'Tokens and emblems, after playable cards',
            'Art-series and memorabilia objects, after playable cards and game pieces'
        )
        deduplication_key = 'oracle_id'
        already_prioritized_cards_are_skipped_when_their_set_is_reached = $true
        catalog_policy = 'Every Scryfall Oracle ID is retained. Playable cards precede tokens/emblems and art-series/memorabilia. Cards releasing after the snapshot date are isolated in a future reconciliation tier.'
    }
    counts = [ordered]@{
        oracle_cards = $oracleById.Count
        queue_rows = $queue.Count
        tiers = $tierCounts
    }
    queue = @($queue)
}
$exhaustiveDocument | ConvertTo-Json -Depth 12 -Compress | Set-Content -LiteralPath $ExhaustiveOutput -Encoding UTF8

Write-Host "Popular queue:    $($popularQueue.Count) rows -> $PopularOutput" -ForegroundColor Green
Write-Host "Exhaustive queue: $($queue.Count) rows -> $ExhaustiveOutput" -ForegroundColor Green
Write-Host "Tier counts: $($tierCounts | ConvertTo-Json -Compress)"
