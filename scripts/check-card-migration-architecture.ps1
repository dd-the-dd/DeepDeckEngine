<#
.SYNOPSIS
Checks that card migrations stay generic, primitive-oriented, and isolated in test files.

.DESCRIPTION
This compilation-free guard reports card-identity branches in production code, serialized IR
primitive/composite kinds, parser recognizers whose names compose verbs, and migration test
filters that do not resolve to dedicated Rust test files.

Retired opaque or composite instructions are forbidden in all production IR. Any
new composite kind is also a violation. With a manifest, exact card names and collector IDs from the
ordered migration queue are also prohibited in production sources.

.PARAMETER AffectedManifest
Optional ordered queue using the official queue array, a cards array, or a bare array. Entries
use oracle_id when available or setCode plus collectorNumber, and may declare proof test filters.

.PARAMETER BaselinePath
Explicit resolver/operation debt baseline. Defaults to card-migration-architecture-baseline.json.

.PARAMETER RequireParserProof
Requires every queued card to declare parserTest.

.PARAMETER RequireAllProofs
Requires every queued card to declare parserTest, contractTest, and runtimeTest.

.PARAMETER Enforce
Returns exit code 1 when violations exist. Without this switch the command is report-only.

.PARAMETER Help
Prints this help without scanning files.

.EXAMPLE
.\scripts\check-card-migration-architecture.ps1 `
    -AffectedManifest .\scripts\card-set-audit.example.json `
    -RequireParserProof -Enforce

Checks exact affected-card identities and resolves parser proof functions to dedicated tests.

.EXAMPLE
.\scripts\check-card-migration-architecture.ps1

Reports repository-wide generic-branch and primitive/composite findings without failing.
#>
[CmdletBinding()]
param(
    [Alias('QueueFile')]
    [string] $AffectedManifest,
    [string] $BaselinePath,
    [string] $CacheDirectory,
    [string] $ReportJson,
    [switch] $RequireParserProof,
    [switch] $RequireAllProofs,
    [switch] $SummaryOnly,
    [switch] $DisableCache,
    [switch] $Enforce,
    [switch] $Help
)

$ErrorActionPreference = 'Stop'
$guardScriptPath = $MyInvocation.MyCommand.Path
$effectCensusScriptPath = Join-Path $PSScriptRoot 'audit-engine-effect-kinds.py'
$effectCensusTestPaths = @(
    (Join-Path $PSScriptRoot 'tests/test_audit_engine_effect_kinds.py')
    (Join-Path $PSScriptRoot 'tests/test_engine_effect_census_architecture.py')
)

if ($Help) {
    Get-Help -Name $MyInvocation.MyCommand.Path -Detailed
    exit 0
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($BaselinePath)) {
    $BaselinePath = Join-Path $PSScriptRoot 'card-migration-architecture-baseline.json'
}
$BaselinePath = (Resolve-Path -LiteralPath $BaselinePath).Path
$manifestPath = if ([string]::IsNullOrWhiteSpace($AffectedManifest)) {
    $null
}
else { (Resolve-Path -LiteralPath $AffectedManifest).Path }
if ([string]::IsNullOrWhiteSpace($CacheDirectory)) {
    $CacheDirectory = Join-Path $repoRoot '.tmp\card-migration-cache\architecture'
}
elseif (-not [IO.Path]::IsPathRooted($CacheDirectory)) {
    $CacheDirectory = Join-Path $repoRoot $CacheDirectory
}
$CacheDirectory = [IO.Path]::GetFullPath($CacheDirectory)
if (-not [string]::IsNullOrWhiteSpace($ReportJson)) {
    if (-not [IO.Path]::IsPathRooted($ReportJson)) { $ReportJson = Join-Path $repoRoot $ReportJson }
    $ReportJson = [IO.Path]::GetFullPath($ReportJson)
}

$architectureCacheSchema = 'deepdeck-card-migration-architecture-cache/v1'
function Get-ArchitectureRustFiles {
    return @(
        foreach ($relativeRoot in @('src', 'crates', 'tests')) {
            $root = Join-Path $repoRoot $relativeRoot
            if (Test-Path -LiteralPath $root -PathType Container) {
                Get-ChildItem -LiteralPath $root -Filter '*.rs' -File -Recurse
            }
        }
    ) | Sort-Object -Property FullName -Unique
}
function Get-ArchitectureFingerprint {
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add("schema=$architectureCacheSchema")
    $lines.Add("powershell=$($PSVersionTable.PSVersion)")
    $lines.Add("runtime=$([Runtime.InteropServices.RuntimeInformation]::FrameworkDescription)")
    $lines.Add("require-parser-proof=$([bool]$RequireParserProof)")
    $lines.Add("require-all-proofs=$([bool]$RequireAllProofs)")
    $domainManifest = Join-Path $repoRoot 'crates\deepdeck-card-domain\Cargo.toml'
    $vocabularyRoot = Join-Path $repoRoot 'data\oracle-vocabulary'
    if (-not (Test-Path -LiteralPath $domainManifest -PathType Leaf)) {
        throw 'Architecture fingerprint requires the shared card-domain manifest.'
    }
    if (-not (Test-Path -LiteralPath $vocabularyRoot -PathType Container)) {
        throw 'Architecture fingerprint requires the Oracle vocabulary data root.'
    }
    $domainDataInputs = @($domainManifest) + @(Get-ChildItem -LiteralPath $vocabularyRoot -File -Recurse | Sort-Object FullName | ForEach-Object { $_.FullName })
    foreach ($path in (@($guardScriptPath, $effectCensusScriptPath, $BaselinePath, $manifestPath) + $effectCensusTestPaths + $domainDataInputs)) {
        if ([string]::IsNullOrWhiteSpace($path)) { continue }
        $relative = if ($path.StartsWith($repoRoot, [StringComparison]::OrdinalIgnoreCase)) {
            $path.Substring($repoRoot.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
        }
        else { [IO.Path]::GetFullPath($path).Replace('\', '/') }
        $lines.Add("$relative=$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash)")
    }
    foreach ($file in Get-ArchitectureRustFiles) {
        $relative = $file.FullName.Substring($repoRoot.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
        $lines.Add("$relative=$((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash)")
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString(
            $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n")))
        ).Replace('-', '')
    }
    finally { $sha.Dispose() }
}
function Write-ArchitectureJson([string] $Path, $Value) {
    if ([string]::IsNullOrWhiteSpace($Path)) { return }
    $directory = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $directory -Force)
    }
    $temporary = "$Path.tmp-$PID"
    [IO.File]::WriteAllText($temporary, ($Value | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
}
function New-ArchitectureMutex {
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = [BitConverter]::ToString(
            $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($CacheDirectory.ToLowerInvariant()))
        ).Replace('-', '').Substring(0, 32)
    }
    finally { $sha.Dispose() }
    return [Threading.Mutex]::new($false, "Local\DeepDeckArchitectureCache-$hash")
}

$architectureFingerprintBefore = Get-ArchitectureFingerprint
$cachePath = Join-Path $CacheDirectory "$architectureFingerprintBefore.json"
$cachedReport = $null
if (-not $DisableCache) {
    if (-not (Test-Path -LiteralPath $CacheDirectory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $CacheDirectory -Force)
    }
    $mutex = New-ArchitectureMutex
    $lockTaken = $false
    try {
        try { $lockTaken = $mutex.WaitOne([TimeSpan]::FromSeconds(30)) }
        catch [Threading.AbandonedMutexException] { $lockTaken = $true }
        if (-not $lockTaken) { throw "Timed out waiting for architecture cache lock: $CacheDirectory" }
        if (Test-Path -LiteralPath $cachePath -PathType Leaf) {
            try {
                $candidate = Get-Content -LiteralPath $cachePath -Raw | ConvertFrom-Json
                if ($candidate.schema_version -eq $architectureCacheSchema -and
                    $candidate.fingerprint -eq $architectureFingerprintBefore -and
                    [int]$candidate.summary.violation_count -eq 0) {
                    $cachedReport = $candidate
                }
            }
            catch { Write-Warning "Ignoring invalid architecture cache metadata ${cachePath}: $($_.Exception.Message)" }
        }
    }
    finally {
        if ($lockTaken) { [void]$mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}
if ($null -ne $cachedReport) {
    $architectureFingerprintAfter = Get-ArchitectureFingerprint
    if ($architectureFingerprintAfter -ne $architectureFingerprintBefore) {
        throw 'Architecture inputs changed while reading the cache; retry the audit.'
    }
    $cachedReport.cache_hit = $true
    Write-ArchitectureJson $ReportJson $cachedReport
    Write-Host 'Card-migration architecture report (cache hit)' -ForegroundColor Cyan
    Write-Host "  production Rust files: $($cachedReport.summary.production_file_count)"
    Write-Host "  dedicated test files:  $($cachedReport.summary.dedicated_test_file_count)"
    if (-not $SummaryOnly) {
        foreach ($finding in @($cachedReport.warnings)) { Write-Warning $finding }
    }
    Write-Host "  result: 0 violation(s), $($cachedReport.summary.warning_count) warning(s)"
    exit 0
}

$debtBaseline = Get-Content -LiteralPath $BaselinePath -Raw | ConvertFrom-Json
if ([int]$debtBaseline.schemaVersion -ne 1) {
    throw "Unsupported architecture baseline schema in $BaselinePath"
}
$violations = [System.Collections.ArrayList]::new()
$warnings = [System.Collections.ArrayList]::new()
$information = [System.Collections.ArrayList]::new()

function Get-RelativePath {
    param([string] $Path)
    return $Path.Substring($repoRoot.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
}

function Add-Violation {
    param([string] $Message)
    [void]$violations.Add($Message)
}

function Add-WarningFinding {
    param([string] $Message)
    [void]$warnings.Add($Message)
}

function Add-InformationFinding {
    param([string] $Message)
    [void]$information.Add($Message)
}

$rustFiles = @(Get-ArchitectureRustFiles)

function Test-IsDedicatedTestFile {
    param([System.IO.FileInfo] $File)

    $relativePath = Get-RelativePath $File.FullName
    return $relativePath -match '(^|/)tests/' -or
        $File.Name -eq 'tests.rs' -or
        $File.BaseName -match '(^|_)(test|tests|parser_batch|migration)$'
}

function Test-IsProductionFile {
    param([System.IO.FileInfo] $File)

    $relativePath = Get-RelativePath $File.FullName
    return -not (Test-IsDedicatedTestFile $File) -and
        $relativePath -notmatch '(^|/)(examples|benches)/'
}

$productionFiles = @($rustFiles | Where-Object { Test-IsProductionFile $_ })
$dedicatedTestFiles = @($rustFiles | Where-Object { Test-IsDedicatedTestFile $_ })
$productionContents = @{}
foreach ($file in $productionFiles) {
    $productionContents[$file.FullName] = [System.IO.File]::ReadAllText($file.FullName)
}
$collectorIdentityAnchorPattern = [regex]::new(
    '(?i)(collector(_number)?|collector number)'
)
$collectorIdentityFiles = @($productionFiles | Where-Object {
    $collectorIdentityAnchorPattern.IsMatch($productionContents[$_.FullName])
})
$testFunctionsByName = @{}
$testFunctionPattern = [regex]::new(
    '(?m)^\s*(?:pub\s+)?fn\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)\s*\('
)
foreach ($file in $dedicatedTestFiles) {
    $relativePath = Get-RelativePath $file.FullName
    $content = [System.IO.File]::ReadAllText($file.FullName)
    foreach ($match in $testFunctionPattern.Matches($content)) {
        $name = $match.Groups['name'].Value
        if (-not $testFunctionsByName.ContainsKey($name)) {
            $testFunctionsByName[$name] = [System.Collections.Generic.List[string]]::new()
        }
        if (-not $testFunctionsByName[$name].Contains($relativePath)) {
            $testFunctionsByName[$name].Add($relativePath)
        }
    }
}

# The strict-native baseline permits no opaque resolver or card operation field.
$resolverPattern = [regex]::new('resolveTriggeredInstruction')
$operationFieldPattern = [regex]::new('"operation"\s*:')
$literalOperationPattern = [regex]::new('"operation"\s*:\s*"(?<operation>[A-Za-z][A-Za-z0-9_]*)"')
$actualResolverCounts = @{}
$actualOperationCounts = @{}
$actualOperationNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$actualHelperCallCounts = @{}
$actualHelperOperationNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$helperCallPattern = [regex]::new('\boperation\s*\(')
$helperLiteralPattern = [regex]::new('\boperation\s*\([^;\r\n]*?"(?<operation>[a-z][A-Za-z0-9_]*)"\s*\)')
foreach ($file in $productionFiles) {
    $relativePath = Get-RelativePath $file.FullName
    $content = $productionContents[$file.FullName]
    $resolverCount = $resolverPattern.Matches($content).Count
    $operationCount = $operationFieldPattern.Matches($content).Count
    if ($resolverCount -gt 0) {
        $actualResolverCounts[$relativePath] = $resolverCount
    }
    if ($operationCount -gt 0) {
        $actualOperationCounts[$relativePath] = $operationCount
    }
    foreach ($operationMatch in $literalOperationPattern.Matches($content)) {
        [void]$actualOperationNames.Add($operationMatch.Groups['operation'].Value)
    }
    $helperCallCount = $helperCallPattern.Matches($content).Count
    if ($helperCallCount -gt 0) {
        $actualHelperCallCounts[$relativePath] = $helperCallCount
    }
    foreach ($helperMatch in $helperLiteralPattern.Matches($content)) {
        [void]$actualHelperOperationNames.Add($helperMatch.Groups['operation'].Value)
    }
}

$baselineResolverCounts = @{}
foreach ($property in $debtBaseline.resolverReferencesByFile.PSObject.Properties) {
    $baselineResolverCounts[$property.Name] = [int]$property.Value
}
$baselineOperationCounts = @{}
foreach ($property in $debtBaseline.operationFieldsByFile.PSObject.Properties) {
    $baselineOperationCounts[$property.Name] = [int]$property.Value
}
$baselineHelperCallCounts = @{}
foreach ($property in $debtBaseline.operationHelperCallsByFile.PSObject.Properties) {
    $baselineHelperCallCounts[$property.Name] = [int]$property.Value
}
$debtFiles = @(
    $baselineResolverCounts.Keys
    $baselineOperationCounts.Keys
    $actualResolverCounts.Keys
    $actualOperationCounts.Keys
) | Sort-Object -Unique
$debtRows = @()
foreach ($relativePath in $debtFiles) {
    $baselineResolver = if ($baselineResolverCounts.ContainsKey($relativePath)) {
        $baselineResolverCounts[$relativePath]
    }
    else { 0 }
    $actualResolver = if ($actualResolverCounts.ContainsKey($relativePath)) {
        $actualResolverCounts[$relativePath]
    }
    else { 0 }
    $baselineOperations = if ($baselineOperationCounts.ContainsKey($relativePath)) {
        $baselineOperationCounts[$relativePath]
    }
    else { 0 }
    $actualOperations = if ($actualOperationCounts.ContainsKey($relativePath)) {
        $actualOperationCounts[$relativePath]
    }
    else { 0 }
    $debtRows += [pscustomobject]@{
        File = $relativePath
        ResolverActual = $actualResolver
        ResolverBaseline = $baselineResolver
        OperationActual = $actualOperations
        OperationBaseline = $baselineOperations
    }
    if ($actualResolver -gt $baselineResolver) {
        Add-Violation "$relativePath grew resolveTriggeredInstruction references from $baselineResolver to $actualResolver."
    }
    if ($actualOperations -gt $baselineOperations) {
        Add-Violation "$relativePath grew operation fields from $baselineOperations to $actualOperations."
    }
}

$helperFiles = @($baselineHelperCallCounts.Keys; $actualHelperCallCounts.Keys) | Sort-Object -Unique
foreach ($relativePath in $helperFiles) {
    $baselineCount = if ($baselineHelperCallCounts.ContainsKey($relativePath)) {
        $baselineHelperCallCounts[$relativePath]
    }
    else { 0 }
    $actualCount = if ($actualHelperCallCounts.ContainsKey($relativePath)) {
        $actualHelperCallCounts[$relativePath]
    }
    else { 0 }
    if ($actualCount -gt $baselineCount) {
        Add-Violation "$relativePath grew dynamic operation helper calls from $baselineCount to $actualCount."
    }
}

$allowedOperationNames = [System.Collections.Generic.HashSet[string]]::new(
    [string[]]@($debtBaseline.literalOperationNames),
    [System.StringComparer]::Ordinal
)
foreach ($operationName in ($actualOperationNames | Sort-Object)) {
    if (-not $allowedOperationNames.Contains($operationName)) {
        Add-Violation "new card-specific operation '$operationName'; compose primitive IR instead."
    }
}
$allowedHelperOperationNames = [System.Collections.Generic.HashSet[string]]::new(
    [string[]]@($debtBaseline.operationHelperLiteralNames),
    [System.StringComparer]::Ordinal
)
foreach ($operationName in ($actualHelperOperationNames | Sort-Object)) {
    if (-not $allowedHelperOperationNames.Contains($operationName)) {
        Add-Violation "new dynamic operation helper literal '$operationName'; compose primitive IR instead."
    }
}

# Branches on a card identity are forbidden even without a migration queue.
$identityBranchPattern = [regex]::new(
    '(?i)(card_name|collector_number|oracle_id|scryfall_id).{0,120}(==|!=|eq_ignore_ascii_case|contains|starts_with|ends_with).{0,100}"[^"\r\n]+"'
)
$identityMatchPattern = [regex]::new('(?i)\bmatch\s+[^\r\n]*(card_name|collector_number|oracle_id|scryfall_id)')
foreach ($file in $productionFiles) {
    $lineNumber = 0
    foreach ($line in [System.IO.File]::ReadLines($file.FullName)) {
        $lineNumber++
        if ($identityBranchPattern.IsMatch($line) -or $identityMatchPattern.IsMatch($line)) {
            Add-Violation "card-specific production branch at $(Get-RelativePath $file.FullName):${lineNumber}: $($line.Trim())"
        }
    }
}

$manifestCards = @()
$manifestNames = @{}
$identityChecks = [System.Collections.Generic.List[object]]::new()
$collectorPatternsByValue = @{}
if (-not [string]::IsNullOrWhiteSpace($AffectedManifest)) {
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($null -ne $manifest.cards) {
        $manifestCards = @($manifest.cards)
    }
    elseif ($null -ne $manifest.queue) {
        $manifestCards = @($manifest.queue | Sort-Object -Property global_rank)
    }
    elseif ($manifest -is [System.Array]) {
        $manifestCards = @($manifest)
    }
    else {
        throw "Affected manifest must contain a cards or queue array: $manifestPath"
    }
}

foreach ($card in $manifestCards) {
    $oracleId = if ($null -ne $card.oracleId) { [string]$card.oracleId } else { [string]$card.oracle_id }
    $setCode = if ($null -ne $card.setCode) { [string]$card.setCode } else { [string]$card.set_code }
    $collectorNumber = if ($null -ne $card.collectorNumber) {
        [string]$card.collectorNumber
    }
    else { [string]$card.collector_number }
    $cardName = [string]$card.name
    $rank = if ($null -ne $card.global_rank) { "rank $($card.global_rank) " } else { '' }
    $identity = if (-not [string]::IsNullOrWhiteSpace($oracleId)) {
        "oracle:$($oracleId.ToLowerInvariant())"
    }
    else {
        "print:$(([string]$setCode).ToLowerInvariant()):$collectorNumber"
    }
    $cardLabel = if ([string]::IsNullOrWhiteSpace($cardName)) {
        "$rank$identity"
    }
    else {
        "$rank$cardName ($identity)"
    }

    if ([string]::IsNullOrWhiteSpace($oracleId) -and
        ([string]::IsNullOrWhiteSpace($setCode) -or [string]::IsNullOrWhiteSpace($collectorNumber))) {
        Add-Violation "$cardLabel needs oracle_id or setCode plus collectorNumber."
    }
    if (-not [string]::IsNullOrWhiteSpace($cardName)) {
        $manifestNames[$cardName.ToLowerInvariant()] = $cardLabel
    }

    $collectorPattern = $null
    if (-not [string]::IsNullOrWhiteSpace($collectorNumber)) {
        $collectorKey = $collectorNumber.ToLowerInvariant()
        if (-not $collectorPatternsByValue.ContainsKey($collectorKey)) {
            $escapedCollector = [regex]::Escape($collectorNumber)
            $collectorPatternText = '(?i)(collector(_number)?|collector number).{0,50}["'']?{0}["'']?' -f `
                $escapedCollector
            $collectorPatternsByValue[$collectorKey] = [regex]::new(
                $collectorPatternText,
                [System.Text.RegularExpressions.RegexOptions]::Compiled
            )
        }
        $collectorPattern = $collectorPatternsByValue[$collectorKey]
    }

    $identityChecks.Add([pscustomobject]@{
        CardLabel = $cardLabel
        OracleId = $oracleId
        CollectorNumber = $collectorNumber
        CollectorPattern = $collectorPattern
    })

    $requiredProofs = @()
    if ($RequireAllProofs) {
        $requiredProofs = @('parserTest', 'contractTest', 'runtimeTest')
    }
    elseif ($RequireParserProof) {
        $requiredProofs = @('parserTest')
    }
    foreach ($proofProperty in $requiredProofs) {
        $snakeProperty = switch ($proofProperty) {
            'parserTest' { 'parser_test' }
            'contractTest' { 'contract_test' }
            'runtimeTest' { 'runtime_test' }
        }
        $requiredFilter = if ($null -ne $card.$proofProperty) {
            [string]$card.$proofProperty
        }
        else { [string]$card.$snakeProperty }
        if ([string]::IsNullOrWhiteSpace($requiredFilter)) {
            Add-Violation "$cardLabel must declare $proofProperty."
        }
    }

    foreach ($proofProperty in @('parserTest', 'contractTest', 'runtimeTest')) {
        $snakeProperty = switch ($proofProperty) {
            'parserTest' { 'parser_test' }
            'contractTest' { 'contract_test' }
            'runtimeTest' { 'runtime_test' }
        }
        $filter = if ($null -ne $card.$proofProperty) {
            [string]$card.$proofProperty
        }
        else { [string]$card.$snakeProperty }
        if ([string]::IsNullOrWhiteSpace($filter)) {
            continue
        }
        $locations = if ($testFunctionsByName.ContainsKey($filter)) {
            @($testFunctionsByName[$filter])
        }
        else { @() }
        if ($locations.Count -eq 0) {
            Add-Violation "$cardLabel declares $proofProperty '$filter', but that function is absent from dedicated test files."
        }
        else {
            if ($VerbosePreference -ne 'SilentlyContinue') {
                Add-InformationFinding "$cardLabel $proofProperty -> $($locations -join ', ')"
            }
        }
    }
}

# Scan all manifest oracle IDs in one pass per production file. The former
# card-by-file loop performed one case-insensitive full-file search for every
# `(card, file)` pair. This combined literal alternation preserves those exact
# matches while reducing the hot path to one regex traversal per file.
$oracleChecks = @($identityChecks | Where-Object {
    -not [string]::IsNullOrWhiteSpace([string]$_.OracleId)
})
$oracleMatchesById = @{}
if ($oracleChecks.Count -gt 0) {
    $oracleIds = @($oracleChecks | ForEach-Object {
        ([string]$_.OracleId).ToLowerInvariant()
    } | Sort-Object -Unique)
    $uuidOracleIds = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    $nonUuidOracleIds = [System.Collections.Generic.List[string]]::new()
    $uuidShape = [regex]::new(
        '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
        [System.Text.RegularExpressions.RegexOptions]::IgnoreCase
    )
    foreach ($oracleId in $oracleIds) {
        if ($uuidShape.IsMatch($oracleId)) {
            [void]$uuidOracleIds.Add($oracleId)
        }
        else {
            $nonUuidOracleIds.Add($oracleId)
        }
    }
    $oracleUuidPattern = [regex]::new(
        '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}',
        [System.Text.RegularExpressions.RegexOptions]::IgnoreCase -bor
            [System.Text.RegularExpressions.RegexOptions]::Compiled
    )
    foreach ($oracleId in $oracleIds) {
        $oracleMatchesById[$oracleId] = [System.Collections.Generic.List[string]]::new()
    }
    foreach ($file in $productionFiles) {
        $relativePath = Get-RelativePath $file.FullName
        $content = $productionContents[$file.FullName]
        $seenInFile = [System.Collections.Generic.HashSet[string]]::new(
            [System.StringComparer]::OrdinalIgnoreCase
        )
        foreach ($match in $oracleUuidPattern.Matches($content)) {
            $oracleId = $match.Value.ToLowerInvariant()
            if ($uuidOracleIds.Contains($oracleId) -and $seenInFile.Add($oracleId)) {
                $oracleMatchesById[$oracleId].Add($relativePath)
            }
        }
        # Non-UUID identities are valid manifest keys too. They retain the
        # exact prior case-insensitive substring semantics on this rare path.
        foreach ($oracleId in $nonUuidOracleIds) {
            if ($content.IndexOf($oracleId, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $oracleMatchesById[$oracleId].Add($relativePath)
            }
        }
    }
}

# Emit findings in manifest/card order, matching the previous deterministic
# output ordering. Collector checks retain their exact per-card regex.
$collectorMatchesByValue = @{}
foreach ($check in $identityChecks) {
    if ($null -ne $check.CollectorPattern) {
        $collectorKey = ([string]$check.CollectorNumber).ToLowerInvariant()
        if (-not $collectorMatchesByValue.ContainsKey($collectorKey)) {
            $matchedFiles = [System.Collections.Generic.List[string]]::new()
            foreach ($file in $collectorIdentityFiles) {
                if ($check.CollectorPattern.IsMatch($productionContents[$file.FullName])) {
                    $matchedFiles.Add((Get-RelativePath $file.FullName))
                }
            }
            $collectorMatchesByValue[$collectorKey] = $matchedFiles
        }
        foreach ($relativePath in $collectorMatchesByValue[$collectorKey]) {
            Add-Violation "$($check.CardLabel) appears by collector identity in production file $relativePath."
        }
    }
    $oracleId = [string]$check.OracleId
    if (-not [string]::IsNullOrWhiteSpace($oracleId)) {
        foreach ($relativePath in $oracleMatchesById[$oracleId.ToLowerInvariant()]) {
            Add-Violation "$($check.CardLabel) appears by oracle_id in production file $relativePath."
        }
    }
}

$reportedNameLiterals = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$stringLiteralPattern = [regex]::new('"(?<value>[^"\r\n]*)"')
foreach ($file in $productionFiles) {
    foreach ($literalMatch in $stringLiteralPattern.Matches($productionContents[$file.FullName])) {
        $value = $literalMatch.Groups['value'].Value.ToLowerInvariant()
        if ($manifestNames.ContainsKey($value)) {
            $findingKey = "$value|$($file.FullName)"
            if ($reportedNameLiterals.Add($findingKey)) {
                Add-WarningFinding "$($manifestNames[$value]) appears as an exact string literal in production file $(Get-RelativePath $file.FullName); verify it is grammar, not card dispatch."
            }
        }
    }
}

# Only serialized `kind` values are engine operations. Verb-composing parser variables below are
# recognizers and are deliberately classified separately.
$primitiveVerbs = @(
    'add', 'cast', 'copy', 'counter', 'create', 'damage', 'deal', 'destroy', 'discard',
    'draw', 'exile', 'gain', 'mill', 'play', 'put', 'remove', 'return', 'sacrifice',
    'search', 'tap', 'untap'
)
$verbAlternation = $primitiveVerbs -join '|'
$compositeKindPattern = [regex]::new(
    "(?i)^($verbAlternation)(And|Then|Or)($verbAlternation)[A-Za-z0-9_]*$"
)
$legacyCompositeKinds = @($debtBaseline.legacyCompositeKinds)
$retiredOracleKinds = [Collections.Generic.HashSet[string]]::new(
    [string[]]@($debtBaseline.retiredOracleKinds),
    [StringComparer]::Ordinal
)
$serializedKindPattern = [regex]::new('"kind"\s*:\s*"(?<kind>[A-Za-z][A-Za-z0-9_]*)"')
$primitiveKinds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$compositeKinds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($file in $productionFiles) {
    $relativePath = Get-RelativePath $file.FullName
    $content = $productionContents[$file.FullName]
    foreach ($kindMatch in $serializedKindPattern.Matches($content)) {
        $kind = $kindMatch.Groups['kind'].Value
        if ($retiredOracleKinds.Contains($kind)) {
            Add-Violation "retired Oracle instruction '$kind' in $relativePath; compose executable primitives instead."
        }
        if ($relativePath -notmatch '(^|/)oracle/canonical/') {
            continue
        }
        if ($compositeKindPattern.IsMatch($kind)) {
            [void]$compositeKinds.Add($kind)
            Add-Violation "composite IR kind '$kind' in $relativePath; emit ordered primitive effects instead."
        }
        elseif ($primitiveVerbs | Where-Object {
            $kind.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase)
        }) {
            [void]$primitiveKinds.Add($kind)
        }
    }
}

$recognizerPattern = [regex]::new(
    "(?i)\b($verbAlternation)_(and|then|or)_($verbAlternation)[A-Za-z0-9_]*\b"
)
$recognizerNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($file in $productionFiles) {
    $relativePath = Get-RelativePath $file.FullName
    if ($relativePath -notmatch '(^|/)oracle/canonical/') {
        continue
    }
    foreach ($recognizerMatch in $recognizerPattern.Matches($productionContents[$file.FullName])) {
        [void]$recognizerNames.Add($recognizerMatch.Value)
    }
}

# Count actual validator/executor roles; do not infer handlers from Some strings or verb prefixes.
$runtimeEffectCensus = $null
$runtimeEffectCensusError = $null
try {
    $pythonExecutable = (Get-Command python -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    $runtimeEffectCensusJson = @(& $pythonExecutable -X utf8 $effectCensusScriptPath `
        --repo-root $repoRoot --check-only --retired-kinds-json $BaselinePath) -join "`n"
    $runtimeEffectCensusExit = $LASTEXITCODE
    if ($runtimeEffectCensusExit -notin @(0, 2)) {
        throw "Runtime effect census failed with exit code $runtimeEffectCensusExit."
    }
    $runtimeEffectCensus = $runtimeEffectCensusJson | ConvertFrom-Json
    if ($runtimeEffectCensus.schemaVersion -cne 'deepdeck-engine-effect-census/v1' -or
        $null -eq $runtimeEffectCensus.counts -or $null -eq $runtimeEffectCensus.retiredGuard -or
        $runtimeEffectCensus.sourceEpochStable -ne $true) {
        throw 'Invalid or unstable runtime effect census result.'
    }
    if ($runtimeEffectCensusExit -eq 2 -and
        @($runtimeEffectCensus.retiredGuard.stillAccepted).Count -eq 0 -and
        @($runtimeEffectCensus.retiredGuard.stillExecuted).Count -eq 0) {
        throw 'Inconsistent retired runtime effect result.'
    }
}
catch {
    $runtimeEffectCensus = $null
    $runtimeEffectCensusError = $_.Exception.Message
    Add-Violation "runtime effect census extraction failed: $runtimeEffectCensusError"
}
if ($null -ne $runtimeEffectCensus) {
    foreach ($kind in @($runtimeEffectCensus.retiredGuard.stillAccepted)) {
        Add-Violation "retired Oracle instruction '$kind' remains accepted by a runtime effect contract."
    }
    foreach ($kind in @($runtimeEffectCensus.retiredGuard.stillExecuted)) {
        Add-Violation "retired Oracle instruction '$kind' remains in a runtime effect executor."
    }
}

Write-Host 'Card-migration architecture report' -ForegroundColor Cyan
Write-Host "  production Rust files: $($productionFiles.Count)"
Write-Host "  dedicated test files:  $($dedicatedTestFiles.Count)"
Write-Host "  resolver debt:          $((($debtRows | Measure-Object ResolverActual -Sum).Sum)) / $((($debtRows | Measure-Object ResolverBaseline -Sum).Sum)) baseline"
Write-Host "  operation fields:       $((($debtRows | Measure-Object OperationActual -Sum).Sum)) / $((($debtRows | Measure-Object OperationBaseline -Sum).Sum)) baseline"
Write-Host "  literal operations:     $($actualOperationNames.Count) / $($allowedOperationNames.Count) baseline names"
Write-Host "  operation helper calls: $((($actualHelperCallCounts.Values | Measure-Object -Sum).Sum)) / $((($baselineHelperCallCounts.Values | Measure-Object -Sum).Sum)) baseline"
Write-Host "  helper literals:        $($actualHelperOperationNames.Count) / $($allowedHelperOperationNames.Count) baseline names"
if ($null -eq $runtimeEffectCensus) {
    Write-Host '  runtime effect census: unavailable (extraction failed)'
}
else {
Write-Host "  accepted effect kinds:  $($runtimeEffectCensus.counts.acceptedKinds) (runtime contracts)"
Write-Host "  executed effect kinds:  $($runtimeEffectCensus.counts.executedKinds) (runtime handlers)"
}
Write-Host "  verb-prefix kind hints: $($primitiveKinds.Count) (heuristic; not a primitive census)"
Write-Host "  composite IR kinds:     $($compositeKinds.Count)"
Write-Host "  parser compositors:     $($recognizerNames.Count) (recognizers, not engine operations)"
if ($VerbosePreference -ne 'SilentlyContinue') {
    foreach ($row in $debtRows) {
        Write-Verbose ("Debt {0}: resolver {1}/{2}, operation fields {3}/{4}" -f `
            $row.File, $row.ResolverActual, $row.ResolverBaseline,
            $row.OperationActual, $row.OperationBaseline)
    }
    Write-Verbose "Verb-prefix kind hints (not a primitive census): $(($primitiveKinds | Sort-Object) -join ', ')"
    Write-Verbose "Parser compositors: $(($recognizerNames | Sort-Object) -join ', ')"
}
foreach ($consumer in @($debtBaseline.legacyEngineOnlyConsumers)) {
    $engineContent = [System.IO.File]::ReadAllText((Join-Path $repoRoot 'src\engine\mod.rs'))
    if ($engineContent.Contains('"' + $consumer + '"')) {
        Add-WarningFinding "legacy engine-only operation consumer '$consumer' remains without a parser producer."
    }
    else {
        Add-InformationFinding "legacy engine-only operation consumer '$consumer' was removed; delete it from the baseline."
    }
}
$architectureFingerprintAfter = Get-ArchitectureFingerprint
if ($architectureFingerprintAfter -ne $architectureFingerprintBefore) {
    throw 'Architecture inputs changed during the scan; no result was published. Retry the audit.'
}
$architectureReport = [pscustomobject][ordered]@{
    schema_version = $architectureCacheSchema
    fingerprint = $architectureFingerprintBefore
    cache_hit = $false
    generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
    summary = [pscustomobject][ordered]@{
        production_file_count = $productionFiles.Count
        dedicated_test_file_count = $dedicatedTestFiles.Count
        resolver_actual = [int](($debtRows | Measure-Object ResolverActual -Sum).Sum)
        resolver_baseline = [int](($debtRows | Measure-Object ResolverBaseline -Sum).Sum)
        operation_actual = [int](($debtRows | Measure-Object OperationActual -Sum).Sum)
        operation_baseline = [int](($debtRows | Measure-Object OperationBaseline -Sum).Sum)
        # Historical field retained for report consumers; it is a verb-prefix heuristic.
        primitive_kind_count = $primitiveKinds.Count
        accepted_effect_kind_count = if ($null -ne $runtimeEffectCensus) { [int]$runtimeEffectCensus.counts.acceptedKinds } else { $null }
        executed_effect_kind_count = if ($null -ne $runtimeEffectCensus) { [int]$runtimeEffectCensus.counts.executedKinds } else { $null }
        composite_kind_count = $compositeKinds.Count
        parser_compositor_count = $recognizerNames.Count
        violation_count = $violations.Count
        warning_count = $warnings.Count
    }
    runtime_effect_census = $runtimeEffectCensus
    runtime_effect_census_error = $runtimeEffectCensusError
    violations = @($violations)
    warnings = @($warnings)
    information = @($information)
}
Write-ArchitectureJson $ReportJson $architectureReport
if (-not $DisableCache -and $violations.Count -eq 0) {
    $mutex = New-ArchitectureMutex
    $lockTaken = $false
    try {
        try { $lockTaken = $mutex.WaitOne([TimeSpan]::FromSeconds(30)) }
        catch [Threading.AbandonedMutexException] { $lockTaken = $true }
        if (-not $lockTaken) { throw "Timed out waiting to publish architecture cache: $CacheDirectory" }
        Write-ArchitectureJson $cachePath $architectureReport
    }
    finally {
        if ($lockTaken) { [void]$mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}
foreach ($finding in $information) {
    Write-Host "  TEST  $finding"
}
if (-not $SummaryOnly) {
    foreach ($finding in $warnings) {
        Write-Warning $finding
    }
}
foreach ($finding in $violations) {
    Write-Host "  ERROR $finding" -ForegroundColor Red
}
Write-Host "  result: $($violations.Count) violation(s), $($warnings.Count) warning(s)"

if ($Enforce -and $violations.Count -gt 0) {
    exit 1
}
exit 0
