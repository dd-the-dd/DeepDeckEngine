<#
.SYNOPSIS
Runs the focused test loop used while migrating one card to the generic parser and engine.

.DESCRIPTION
Runs any supplied stages in dependency order: parser, parser/engine contract, then runtime.
Every stage uses a Cargo test-name filter and stops the pipeline immediately on failure.
At least one test filter must be supplied. Omit a filter to skip that stage.

The script always runs from the repository root, sets RUST_MIN_STACK for the test
processes, and prints per-stage plus total elapsed times.

.PARAMETER ParserTest
Substring filter passed to the deepdeck-oracle card_coverage integration test.

.PARAMETER ParserTestTarget
Exact deepdeck-oracle integration-test target. Defaults to card_coverage.
Use lib for focused private automaton tests in the standalone parser library.

.PARAMETER ContractTest
Substring filter passed to the deepdeck-oracle-engine-contracts test suite.

.PARAMETER ContractTestTarget
Optional exact contract integration-test target.

.PARAMETER ContractPartition
CI test partition used by the contract crate (1 through 4). Defaults to 3.

.PARAMETER RuntimeTest
Substring filter passed to the deepdeck-engine-runtime test suite.

.PARAMETER RuntimeTestTarget
Optional exact runtime integration-test target. Without one, only --lib is built.

.PARAMETER ReportJson
Optional JSON report separating compilation/link time from test execution time.

.PARAMETER RuntimePartition
CI test partition used by the runtime crate (1 through 4). Defaults to 2.

.PARAMETER RuntimeStandalone
Compile pure engine tests without the Oracle parser or the historical parser-backed aggregate.

.PARAMETER RustMinStack
Value assigned to RUST_MIN_STACK for every Cargo process. Defaults to 33554432.

.PARAMETER TargetDirectory
Cargo target directory. Defaults to CARGO_TARGET_DIR when set, otherwise the isolated
.tmp/targets/card-migration directory.

.PARAMETER Help
Prints this detailed help without running Cargo.

.EXAMPLE
.\scripts\test-card-migration.ps1 `
    -ParserTest 'namor' `
    -ContractTest 'reusable_marvel_leaves_share_events_targets_and_primitive_effects' `
    -ContractPartition 3 `
    -RuntimeTest 'parsed_mana_symbol_cast_trigger_filters_and_uses_the_symbol_count_for_tokens' `
    -RuntimePartition 2

Runs all three focused migration gates and stops at the first failure.

.EXAMPLE
.\scripts\test-card-migration.ps1 -ParserTest 'lightning_helix'

Runs only the fast parser coverage gate.

.EXAMPLE
.\scripts\test-card-migration.ps1 -Help

Prints this help and exits without compiling or testing.
#>
[CmdletBinding()]
param(
    [string] $ParserTest,

    [string] $ParserTestTarget = 'card_coverage',

    [string] $ContractTest,

    [string] $ContractTestTarget,

    [ValidateRange(1, 4)]
    [int] $ContractPartition = 3,

    [string] $RuntimeTest,

    [string] $RuntimeTestTarget,

    [ValidateRange(1, 4)]
    [int] $RuntimePartition = 2,

    [switch] $RuntimeStandalone,

    [ValidateRange(1, 2147483647)]
    [int] $RustMinStack = 33554432,

    [string] $TargetDirectory,

    [string] $ReportJson,

    [ValidateRange(1, [long]::MaxValue)]
    [long] $TargetBudgetBytes = 40GB,

    [switch] $SkipTargetTrim,

    [switch] $Help
)

$ErrorActionPreference = 'Stop'

if ($Help) {
    Get-Help -Name $MyInvocation.MyCommand.Path -Detailed
    exit 0
}

$stages = @()

if (-not [string]::IsNullOrWhiteSpace($ParserTest)) {
    $parserTargetArguments = if ($ParserTestTarget -eq 'lib') { @('--lib') }
    else { @('--test', $ParserTestTarget) }
    $stages += [pscustomobject]@{
        Name = 'parser'
        CargoArguments = @(
            'test',
            '--locked',
            '-p', 'deepdeck-oracle'
        ) + $parserTargetArguments + @(
            $ParserTest,
            '--', '--nocapture'
        )
    }
}

if (-not [string]::IsNullOrWhiteSpace($ContractTest)) {
    $contractTargetArguments = if ([string]::IsNullOrWhiteSpace($ContractTestTarget)) { @() }
    else { @('--test', $ContractTestTarget) }
    $stages += [pscustomobject]@{
        Name = "contract (partition $ContractPartition)"
        CargoArguments = @(
            'test',
            '--locked',
            '-p', 'deepdeck-oracle-engine-contracts',
            '--no-default-features',
            '--features', "ci-unit-tests-$ContractPartition"
        ) + $contractTargetArguments + @($ContractTest, '--', '--nocapture')
    }
}

if (-not [string]::IsNullOrWhiteSpace($RuntimeTest)) {
    $runtimeTargetArguments = if ([string]::IsNullOrWhiteSpace($RuntimeTestTarget)) { @('--lib') }
    else { @('--test', $RuntimeTestTarget) }
    $runtimeFeatures = if ($RuntimeStandalone) { "ci-unit-tests-$RuntimePartition" }
    else { "oracle-compilation,ci-unit-tests-$RuntimePartition" }
    $stages += [pscustomobject]@{
        Name = "runtime (partition $RuntimePartition)"
        CargoArguments = @(
            'test',
            '--locked',
            '-p', 'deepdeck-engine-runtime',
            '--no-default-features',
            '--features', $runtimeFeatures
        ) + $runtimeTargetArguments + @($RuntimeTest, '--', '--nocapture')
    }
}

if ($stages.Count -eq 0) {
    throw 'Supply at least one of -ParserTest, -ContractTest, or -RuntimeTest. Use -Help for examples.'
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$targetDefault = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR
}
else {
    Join-Path $repoRoot '.tmp\targets\card-migration'
}
if ([string]::IsNullOrWhiteSpace($TargetDirectory)) {
    $TargetDirectory = $targetDefault
}
elseif (-not [System.IO.Path]::IsPathRooted($TargetDirectory)) {
    $TargetDirectory = Join-Path $repoRoot $TargetDirectory
}
$env:CARGO_TARGET_DIR = [System.IO.Path]::GetFullPath($TargetDirectory)
$env:RUST_MIN_STACK = $RustMinStack.ToString([System.Globalization.CultureInfo]::InvariantCulture)
if (-not $SkipTargetTrim) {
    & (Join-Path $PSScriptRoot 'trim-cargo-incremental.ps1') -TargetDirectory $env:CARGO_TARGET_DIR -BudgetBytes $TargetBudgetBytes
}
$laneHashAlgorithm = [Security.Cryptography.SHA256]::Create()
try {
    $laneHash = [BitConverter]::ToString(
        $laneHashAlgorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($env:CARGO_TARGET_DIR.ToLowerInvariant()))
    ).Replace('-', '').Substring(0, 32)
}
finally { $laneHashAlgorithm.Dispose() }
$cargoLaneName = "Local\DeepDeckCardMigrationCargoLane-$laneHash"
$results = [ordered]@{}
$totalTimer = [System.Diagnostics.Stopwatch]::StartNew()
$failure = $null

function Invoke-MigrationStage {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,

        [Parameter(Mandatory = $true)]
        [string[]] $CargoArguments
    )

    $separator = [Array]::IndexOf($CargoArguments, '--')
    if ($separator -lt 0) { $separator = $CargoArguments.Count }
    $testFilter = [string]$CargoArguments[$separator - 1]
    $compileArguments = @($CargoArguments[0..($separator - 1)]) + @('--no-run', '--message-format=json')
    $queueTimer = [System.Diagnostics.Stopwatch]::StartNew()
    $cargoLane = [Threading.Mutex]::new($false, $cargoLaneName)
    $cargoLaneTaken = $false
    try { $cargoLaneTaken = $cargoLane.WaitOne([TimeSpan]::FromMinutes(10)) }
    catch [Threading.AbandonedMutexException] { $cargoLaneTaken = $true }
    $queueTimer.Stop()
    if (-not $cargoLaneTaken) { throw "Timed out waiting for Cargo lane $cargoLaneName" }
    $compileTimer = [System.Diagnostics.Stopwatch]::StartNew()
    $executionTimer = $null
    $status = 'PASS'
    $buildOccurred = $false
    Write-Host "`n[$Name compile] cargo $($compileArguments -join ' ')" -ForegroundColor Cyan

    try {
        $priorErrorActionPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            $compileOutput = @(& cargo @compileArguments 2>&1)
            $compileExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $priorErrorActionPreference
        }
        $testExecutables = [System.Collections.Generic.List[string]]::new()
        foreach ($line in $compileOutput) {
            $message = $null
            try { $message = $line.ToString() | ConvertFrom-Json -ErrorAction Stop }
            catch { Write-Host $line; continue }
            if ($message.reason -eq 'compiler-message' -and $message.message.rendered) {
                Write-Host ([string]$message.message.rendered)
            }
            if ($message.reason -eq 'compiler-artifact') {
                if (-not [bool]$message.fresh) { $buildOccurred = $true }
                if ($message.executable -and [bool]$message.profile.test) {
                    $executable = [System.IO.Path]::GetFullPath([string]$message.executable)
                    if (-not $testExecutables.Contains($executable)) { $testExecutables.Add($executable) }
                }
            }
        }
        if ($compileExitCode -ne 0) {
            throw "Cargo compilation failed in stage '$Name' with exit code $compileExitCode."
        }
        if ($testExecutables.Count -eq 0) {
            throw "Cargo returned no test executable for stage '$Name'."
        }
        $compileTimer.Stop()
        $cargoLane.ReleaseMutex()
        $cargoLaneTaken = $false
        Write-Host "[$Name execute] $($testExecutables.Count) direct test harness(es), filter '$testFilter'" -ForegroundColor Cyan
        $executionTimer = [System.Diagnostics.Stopwatch]::StartNew()
        $matchedTestCount = 0
        foreach ($testExecutable in $testExecutables) {
            $priorErrorActionPreference = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $executionOutput = @(& $testExecutable $testFilter '--nocapture' 2>&1)
                $executionExitCode = $LASTEXITCODE
            }
            finally { $ErrorActionPreference = $priorErrorActionPreference }
            foreach ($line in $executionOutput) {
                Write-Host $line
                if ($line.ToString() -match '^running\s+([0-9]+)\s+tests?') {
                    $matchedTestCount += [int]$Matches[1]
                }
            }
            if ($executionExitCode -ne 0) {
                throw "Test executable failed in stage '$Name' with exit code $executionExitCode."
            }
        }
        if ($matchedTestCount -eq 0) { throw "No test matched filter '$testFilter' in stage '$Name'." }
    }
    catch {
        $status = 'FAIL'
        throw
    }
    finally {
        if ($compileTimer.IsRunning) { $compileTimer.Stop() }
        if ($null -ne $executionTimer -and $executionTimer.IsRunning) { $executionTimer.Stop() }
        if ($cargoLaneTaken) { $cargoLane.ReleaseMutex() }
        $cargoLane.Dispose()
        $results[$Name] = [pscustomobject]@{
            Status = $status
            QueueWaitSeconds = $queueTimer.Elapsed.TotalSeconds
            CompileSeconds = $compileTimer.Elapsed.TotalSeconds
            ExecutionSeconds = if ($null -eq $executionTimer) { 0.0 } else { $executionTimer.Elapsed.TotalSeconds }
            BuildOccurred = $buildOccurred
        }
    }
}

Push-Location -LiteralPath $repoRoot
try {
    Write-Host "Repository: $repoRoot"
    Write-Host "Cargo target: $env:CARGO_TARGET_DIR"
    Write-Host "RUST_MIN_STACK=$env:RUST_MIN_STACK"

    foreach ($stage in $stages) {
        Invoke-MigrationStage -Name $stage.Name -CargoArguments $stage.CargoArguments
    }
}
catch {
    $failure = $_
}
finally {
    Pop-Location
    $totalTimer.Stop()
}

Write-Host "`nMigration test summary" -ForegroundColor Cyan
foreach ($name in $results.Keys) {
    $result = $results[$name]
    Write-Host ('  {0,-28} {1,-4} compile={2,8:N2} s execute={3,8:N2} s build={4}' -f `
        $name, $result.Status, $result.CompileSeconds, $result.ExecutionSeconds, $result.BuildOccurred)
    if ($result.QueueWaitSeconds -gt 0.01) {
        Write-Host ('  {0,-28}      queue-wait={1,8:N2} s' -f '', $result.QueueWaitSeconds)
    }
}
Write-Host ('  {0,-28} {1,14:N2} s' -f 'total', $totalTimer.Elapsed.TotalSeconds)

if (-not [string]::IsNullOrWhiteSpace($ReportJson)) {
    if (-not [System.IO.Path]::IsPathRooted($ReportJson)) {
        $ReportJson = Join-Path $repoRoot $ReportJson
    }
    $reportDirectory = Split-Path -Parent $ReportJson
    if (-not (Test-Path -LiteralPath $reportDirectory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $reportDirectory -Force)
    }
    $stageReports = @($results.Keys | ForEach-Object {
        $result = $results[$_]
        [pscustomobject][ordered]@{
            name = $_
            status = $result.Status.ToLowerInvariant()
            queue_wait_seconds = [Math]::Round($result.QueueWaitSeconds, 3)
            compile_seconds = [Math]::Round($result.CompileSeconds, 3)
            execution_seconds = [Math]::Round($result.ExecutionSeconds, 3)
            build_occurred = [bool]$result.BuildOccurred
        }
    })
    $report = [pscustomobject][ordered]@{
        schema_version = 'deepdeck-card-migration-tests/v1'
        generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        target_directory = $env:CARGO_TARGET_DIR
        total_seconds = [Math]::Round($totalTimer.Elapsed.TotalSeconds, 3)
        stages = $stageReports
        passed = $null -eq $failure
    }
    [System.IO.File]::WriteAllText(
        [System.IO.Path]::GetFullPath($ReportJson),
        ($report | ConvertTo-Json -Depth 8),
        [Text.UTF8Encoding]::new($false)
    )
    Write-Host "Timing JSON: $([System.IO.Path]::GetFullPath($ReportJson))"
}

if ($null -ne $failure) {
    Write-Error -ErrorAction Continue $failure.Exception.Message
    exit 1
}

exit 0
