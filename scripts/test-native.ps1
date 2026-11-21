<#
.SYNOPSIS
Runs the fast, data-driven native Oracle regression profile.

.DESCRIPTION
The native profile deliberately does not invoke `cargo test`: it runs the
compilation-free architecture/test-layout guards and executes every JSON case
through the single stable parser fixture runner. Use -Build only after parser
sources changed; subsequent fixture-only iterations reuse the existing binary.

Historical Rust integration tests remain available through ordinary Cargo
commands and CI, but are not linked by this profile. Use -InventoryOnly to
inspect the exact boundary without running a guard, runner, or Cargo command.

.PARAMETER Build
Build the parser fixture runner before executing it. This is the only path in
this script that can invoke Cargo, through run-parser-fixture.ps1.

.PARAMETER InventoryOnly
Print and optionally save the test-boundary inventory without executing tests.

.PARAMETER SkipArchitectureGuard
Skip the compilation-free card-migration architecture guard.

.PARAMETER SkipPartitionGuard
Skip the compilation-free private unit-test partition guard.

.PARAMETER TargetDirectory
Shared Cargo target directory used only when -Build is supplied. Defaults to
CARGO_TARGET_DIR, otherwise .tmp/targets/card-migration.

.PARAMETER ReportJson
Optional machine-readable profile report.
#>
[CmdletBinding()]
param(
    [switch] $Build,
    [switch] $InventoryOnly,
    [switch] $SkipArchitectureGuard,
    [switch] $SkipPartitionGuard,
    [string] $TargetDirectory,
    [string] $ReportJson
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$powerShellExecutable = (Get-Process -Id $PID).Path
$parserFixtureDirectory = Join-Path $repoRoot 'crates\deepdeck-parser-fixture-runner\fixtures'
$edgeProofs = @(
    'src/oracle/canonical/automata/tests.rs',
    'src/oracle/canonical/automata/sequence_context_tests.rs',
    'src/oracle/canonical/automata/arithmetic_expression_tests.rs',
    'src/oracle/canonical/effects/numeric_tests.rs',
    'src/engine/cost_payment_tests.rs',
    'src/engine/tests/source_lki_runtime.rs',
    'src/engine/tests/snapshot_bindings.rs'
)

function Get-RustTestInventory([string] $RelativeDirectory) {
    $directory = Join-Path $repoRoot $RelativeDirectory
    $files = if (Test-Path -LiteralPath $directory -PathType Container) {
        @(Get-ChildItem -LiteralPath $directory -File -Filter '*.rs' | Sort-Object Name)
    }
    else { @() }
    $testCount = 0
    foreach ($file in $files) {
        $testCount += [regex]::Matches(
            [IO.File]::ReadAllText($file.FullName),
            '(?m)^\s*#\[test\]\s*$'
        ).Count
    }
    [pscustomobject][ordered]@{
        directory = $RelativeDirectory.Replace('\', '/')
        target_count = $files.Count
        test_count = $testCount
        targets = @($files.BaseName)
    }
}

function Get-ParserFixtureInventory {
    $files = @(Get-ChildItem -LiteralPath $parserFixtureDirectory -File -Filter '*.json' |
        Sort-Object Name)
    $caseCount = 0
    foreach ($file in $files) {
        $fixture = [IO.File]::ReadAllText($file.FullName) | ConvertFrom-Json
        $caseCount += @($fixture.cases).Count
    }
    [pscustomobject][ordered]@{
        directory = 'crates/deepdeck-parser-fixture-runner/fixtures'
        fixture_count = $files.Count
        case_count = $caseCount
        fixtures = @($files.Name)
    }
}

function Get-TestBoundaryInventory {
    $missingEdgeProofs = @($edgeProofs | Where-Object {
        -not (Test-Path -LiteralPath (Join-Path $repoRoot $_) -PathType Leaf)
    })
    if ($missingEdgeProofs.Count -ne 0) {
        throw "Required edge-proof files are missing: $($missingEdgeProofs -join ', ')"
    }
    [pscustomobject][ordered]@{
        historical_rust_harnesses = [pscustomobject][ordered]@{
            parser = Get-RustTestInventory 'crates/deepdeck-oracle/tests'
            parser_engine_contracts = Get-RustTestInventory 'crates/deepdeck-oracle-engine-contracts/tests'
            runtime_fixture_runner = Get-RustTestInventory 'crates/deepdeck-runtime-fixture-runner/tests'
            engine_runtime = Get-RustTestInventory 'crates/deepdeck-engine-runtime/tests'
        }
        native_profile = [pscustomobject][ordered]@{
            cargo_test_harnesses = 0
            stable_runner_binaries = 1
            runner_package = 'deepdeck-parser-fixture-runner'
            runner_binary = 'deepdeck-parser-fixture-runner'
            fixtures = Get-ParserFixtureInventory
        }
        preserved_edge_proofs = @($edgeProofs | ForEach-Object { $_.Replace('\', '/') })
    }
}

function Resolve-OutputPath([string] $Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) { return $null }
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Write-JsonReport([string] $Path, $Value) {
    if ([string]::IsNullOrWhiteSpace($Path)) { return }
    $directory = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $directory -Force)
    }
    [IO.File]::WriteAllText(
        $Path,
        ($Value | ConvertTo-Json -Depth 12),
        [Text.UTF8Encoding]::new($false)
    )
}

function Invoke-Guard([string] $Name, [string] $Script, [string[]] $Arguments) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    & $powerShellExecutable -NoProfile -File $Script @Arguments |
        ForEach-Object { Write-Host $_ }
    $exitCode = $LASTEXITCODE
    $timer.Stop()
    [pscustomobject][ordered]@{
        name = $Name
        seconds = [Math]::Round($timer.Elapsed.TotalSeconds, 3)
        exit_code = $exitCode
        passed = $exitCode -eq 0
    }
}

$inventory = Get-TestBoundaryInventory
$parserHistorical = $inventory.historical_rust_harnesses.parser
$nativeFixtures = $inventory.native_profile.fixtures
Write-Host 'DeepDeck native test boundary' -ForegroundColor Cyan
Write-Host ("  historical parser integration: {0} harnesses / {1} tests" -f `
    $parserHistorical.target_count, $parserHistorical.test_count)
Write-Host ("  native data-driven profile:    {0} test harnesses / {1} runner / {2} fixtures / {3} cases" -f `
    $inventory.native_profile.cargo_test_harnesses,
    $inventory.native_profile.stable_runner_binaries,
    $nativeFixtures.fixture_count,
    $nativeFixtures.case_count)

$reportPath = Resolve-OutputPath $ReportJson
if ($InventoryOnly) {
    $report = [pscustomobject][ordered]@{
        schema_version = 'deepdeck-native-test-profile/v1'
        generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        mode = 'inventory-only'
        inventory = $inventory
        cargo_invoked = $false
        passed = $true
    }
    Write-JsonReport $reportPath $report
    exit 0
}

$targetDefault = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR
}
else { Join-Path $repoRoot '.tmp\targets\card-migration' }
if ([string]::IsNullOrWhiteSpace($TargetDirectory)) { $TargetDirectory = $targetDefault }
elseif (-not [IO.Path]::IsPathRooted($TargetDirectory)) {
    $TargetDirectory = Join-Path $repoRoot $TargetDirectory
}
$TargetDirectory = [IO.Path]::GetFullPath($TargetDirectory)

$stages = [Collections.Generic.List[object]]::new()
$failed = $false
if (-not $SkipPartitionGuard) {
    $result = Invoke-Guard 'unit-test-partitions' `
        (Join-Path $PSScriptRoot 'check-unit-test-partitions.ps1') @()
    $stages.Add($result)
    $failed = $failed -or -not $result.passed
}
if (-not $SkipArchitectureGuard) {
    $result = Invoke-Guard 'migration-architecture' `
        (Join-Path $PSScriptRoot 'check-card-migration-architecture.ps1') @('-SummaryOnly', '-Enforce')
    $stages.Add($result)
    $failed = $failed -or -not $result.passed
}

$parserReportPath = Join-Path $repoRoot ".tmp\native-test-profile\parser-fixtures-$PID.json"
$parserArguments = @(
    '-NoProfile',
    '-File', (Join-Path $PSScriptRoot 'run-parser-fixture.ps1'),
    '-Fixture', $parserFixtureDirectory,
    '-TargetDirectory', $TargetDirectory,
    '-ReportJson', $parserReportPath,
    '-NativeOnly'
)
if ($Build) { $parserArguments += '-Build' }
$timer = [Diagnostics.Stopwatch]::StartNew()
& $powerShellExecutable @parserArguments
$parserExitCode = $LASTEXITCODE
$timer.Stop()
$parserStage = [pscustomobject][ordered]@{
    name = 'native-parser-fixtures'
    seconds = [Math]::Round($timer.Elapsed.TotalSeconds, 3)
    exit_code = $parserExitCode
    passed = $parserExitCode -eq 0
    report = $parserReportPath.Substring($repoRoot.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
}
$stages.Add($parserStage)
$failed = $failed -or -not $parserStage.passed

$report = [pscustomobject][ordered]@{
    schema_version = 'deepdeck-native-test-profile/v1'
    generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
    mode = 'nativeOnly'
    inventory = $inventory
    cargo_invoked = [bool]$Build
    target_directory = $TargetDirectory
    stages = @($stages)
    passed = -not $failed
}
Write-JsonReport $reportPath $report
if ($failed) { exit 1 }
exit 0
