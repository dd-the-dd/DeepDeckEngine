[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$updater = Join-Path $repositoryRoot "scripts/update-official-ability-snapshot.ps1"
$checkedPath = Join-Path $repositoryRoot "data/ability-catalog/official-ability-snapshot-v2.json"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-official-snapshot-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $temporaryRoot
try {
    $firstPath = Join-Path $temporaryRoot "snapshot-one.json"
    $secondPath = Join-Path $temporaryRoot "snapshot-two.json"
    & $updater -OutputPath $firstPath
    & $updater -OutputPath $secondPath
    $first = Get-Content -LiteralPath $firstPath -Raw -Encoding UTF8
    $second = Get-Content -LiteralPath $secondPath -Raw -Encoding UTF8
    if ($first -ne $second) { throw "Official snapshot generation is not deterministic" }
    if ($first -ne (Get-Content -LiteralPath $checkedPath -Raw -Encoding UTF8)) { throw "Checked official snapshot v2 is stale" }

    $snapshot = $first | ConvertFrom-Json
    if ($snapshot.snapshotSchemaVersion -ne "deepdeck-official-ability-snapshot/v2") { throw "Unexpected snapshot schema" }
    if (@($snapshot.keywordActions).Count -ne 71 -or @($snapshot.keywordAbilities).Count -ne 195 -or @($snapshot.abilityWords).Count -ne 66) { throw "Unexpected official surface counts" }
    if (@($snapshot.keywordActions.clauses).Count -ne 233 -or @($snapshot.keywordAbilities.clauses).Count -ne 574) { throw "Unexpected official clause counts" }
    foreach ($surface in @($snapshot.keywordActions) + @($snapshot.keywordAbilities)) {
        if (@($surface.clauses).Count -eq 0) { throw "Official surface '$($surface.name)' has no clauses" }
        foreach ($clause in @($surface.clauses)) {
            if (-not ([string]$clause.section).StartsWith([string]$surface.section)) { throw "Clause '$($clause.section)' escaped '$($surface.section)'" }
            if ([string]::IsNullOrWhiteSpace([string]$clause.text)) { throw "Clause '$($clause.section)' is empty" }
        }
    }
    $scry = $snapshot.keywordActions | Where-Object name -eq "Scry"
    $surveil = $snapshot.keywordActions | Where-Object name -eq "Surveil"
    if ($scry.section -ne "701.22" -or @($scry.clauses).Count -ne 4) { throw "Official Scry rules missing" }
    if ($surveil.section -ne "701.25" -or @($surveil.clauses).Count -ne 4) { throw "Official Surveil rules missing" }
    Write-Host "Official ability snapshot v2 tests passed: 332 headings and 807 numbered keyword clauses."
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
