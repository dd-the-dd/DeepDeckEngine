[CmdletBinding()]
param(
    [string]$SourceUrl = "https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt",
    [string]$EffectiveDate = "2026-11-06",
    [string]$OutputPath = "data/ability-catalog/official-ability-snapshot-v2.json"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Resolve-RepositoryPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $repositoryRoot $Path))
}

function Get-RuleSurfaces {
    param(
        [Parameter(Mandatory = $true)][string[]]$Lines,
        [Parameter(Mandatory = $true)][ValidateSet("701", "702")][string]$Chapter
    )

    $headingPattern = "^$Chapter\.(\d+)\. (.+)$"
    $clausePattern = "^$Chapter\.(\d+)([a-z]) (.+)$"
    $clausesByOrdinal = @{}
    foreach ($line in $Lines) {
        if ($line -notmatch $clausePattern) { continue }
        $ordinal = [int]$Matches[1]
        if (-not $clausesByOrdinal.ContainsKey($ordinal)) {
            $clausesByOrdinal[$ordinal] = [System.Collections.Generic.List[object]]::new()
        }
        $clausesByOrdinal[$ordinal].Add([pscustomobject][ordered]@{
            section = "$Chapter.$ordinal$($Matches[2])"
            text = [string]$Matches[3]
        })
    }

    $surfaces = [System.Collections.Generic.List[object]]::new()
    foreach ($line in $Lines) {
        if ($line -notmatch $headingPattern) { continue }
        $ordinal = [int]$Matches[1]
        if ($ordinal -le 1) { continue }
        $clauses = if ($clausesByOrdinal.ContainsKey($ordinal)) { @($clausesByOrdinal[$ordinal]) } else { @() }
        if ($clauses.Count -eq 0) { throw "Official surface $Chapter.$ordinal has no numbered clauses" }
        $surfaces.Add([pscustomobject][ordered]@{
            section = "$Chapter.$ordinal"
            name = [string]$Matches[2].Trim()
            clauses = @($clauses)
        })
    }
    return @($surfaces | Sort-Object { [int]($_.section.Split('.')[1]) })
}

$client = [Net.WebClient]::new()
try { $bytes = $client.DownloadData($SourceUrl) }
finally { $client.Dispose() }
$rulesText = [Text.Encoding]::UTF8.GetString($bytes).TrimStart([char]0xFEFF)
$effectiveSentence = "These rules are effective as of " + ([datetime]::ParseExact($EffectiveDate, "yyyy-MM-dd", [Globalization.CultureInfo]::InvariantCulture).ToString("MMMM d, yyyy", [Globalization.CultureInfo]::InvariantCulture)) + "."
if (-not $rulesText.Contains($effectiveSentence)) { throw "Rules effective-date sentence not found: $effectiveSentence" }
$lines = @($rulesText -split "`r?`n" | Where-Object { $_.Length -gt 0 })

$keywordActions = Get-RuleSurfaces -Lines $lines -Chapter "701"
$keywordAbilities = Get-RuleSurfaces -Lines $lines -Chapter "702"
$abilityWordMatch = [regex]::Match($rulesText, 'The ability words are (.*?)\.', [Text.RegularExpressions.RegexOptions]::Singleline)
if (-not $abilityWordMatch.Success) { throw "Unable to locate the CR 207.2c ability-word list" }
$abilityWordText = ($abilityWordMatch.Groups[1].Value -replace '\s+', ' ').Trim() -replace ', and ', ', '
$abilityWords = @($abilityWordText.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })

if ($keywordActions.Count -ne 71 -or $keywordAbilities.Count -ne 195 -or $abilityWords.Count -ne 66) {
    throw "Unexpected official inventory counts: actions=$($keywordActions.Count), abilities=$($keywordAbilities.Count), abilityWords=$($abilityWords.Count)"
}
$allSections = @($keywordActions.section) + @($keywordAbilities.section)
if (@($allSections | Sort-Object -Unique).Count -ne $allSections.Count) { throw "Duplicate official keyword section" }

$sha256 = [Security.Cryptography.SHA256]::Create()
try { $digest = ([BitConverter]::ToString($sha256.ComputeHash($bytes))).Replace('-', '') }
finally { $sha256.Dispose() }

$snapshot = [ordered]@{
    snapshotSchemaVersion = "deepdeck-official-ability-snapshot/v2"
    snapshotVersion = $EffectiveDate
    source = [ordered]@{
        document = "ComprehensiveRules"
        effectiveDate = $EffectiveDate
        url = $SourceUrl
        sha256 = $digest
    }
    keywordActions = $keywordActions
    keywordAbilities = $keywordAbilities
    abilityWords = $abilityWords
}

$resolvedOutput = Resolve-RepositoryPath $OutputPath
$parent = Split-Path -Parent $resolvedOutput
if (-not (Test-Path -LiteralPath $parent -PathType Container)) { $null = New-Item -ItemType Directory -Path $parent -Force }
[IO.File]::WriteAllText($resolvedOutput, (($snapshot | ConvertTo-Json -Depth 20) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
Write-Host "Official ability snapshot v2 generated: $($keywordActions.Count) actions, $($keywordAbilities.Count) keyword abilities, $($abilityWords.Count) ability words, with numbered clauses."
