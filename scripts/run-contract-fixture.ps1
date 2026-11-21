<#
.SYNOPSIS
Build once, then run stable IR and full-card compilation contracts from DATA.
.DESCRIPTION
Uses the shared Cargo lane and fingerprint helpers. Without -Build, absent,
stale or tampered artifacts are rejected. Fixture JSON does not enter the binary
fingerprint. File/directory arguments go directly to the runner in one process,
preserving its recursive discovery, file deduplication and duplicate-ID checks.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]] $Fixture,
    [switch] $Build,
    [string] $TargetDirectory,
    [string] $ReportJson
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'runner-fingerprint.ps1')
if ([string]::IsNullOrWhiteSpace($TargetDirectory)) {
    $TargetDirectory = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { '.tmp/targets/card-migration' }
}
if (-not [IO.Path]::IsPathRooted($TargetDirectory)) { $TargetDirectory = Join-Path $repoRoot $TargetDirectory }
$targetPath = [IO.Path]::GetFullPath($TargetDirectory)
$env:CARGO_TARGET_DIR = $targetPath
$binaryName = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    'deepdeck-contract-fixture-runner.exe'
} else { 'deepdeck-contract-fixture-runner' }
$runnerPath = Join-Path (Join-Path $targetPath 'debug') $binaryName
$metadataPath = Join-Path (Join-Path $targetPath 'debug') 'deepdeck-contract-fixture-runner.fingerprint.json'
$fingerprintSchema = 'deepdeck-contract-runner-inputs/v1'
$metadataSchema = 'deepdeck-contract-runner-fingerprint/v1'
$profile = 'debug'
$buildArguments = @('build', '--locked', '-p', 'deepdeck-contract-fixture-runner', '--message-format=json')
$totalTimer = [Diagnostics.Stopwatch]::StartNew()
$queueSeconds = 0.0
$compileSeconds = 0.0
$executionSeconds = 0.0
$buildOccurred = $false
$compiledAnything = $false
$fingerprintVerified = $false
$fingerprint = $null
$inputFiles = @()
$arguments = @()
$runtimeArtifacts = @()
$resultCount = 0
$runnerSummary = $null
$failure = $null
$exitCode = 1

function Get-ContractRunnerInputFiles {
    $files = [Collections.Generic.List[IO.FileInfo]]::new()
    foreach ($relative in @('Cargo.toml', 'Cargo.lock',
        'crates/deepdeck-oracle/Cargo.toml', 'crates/deepdeck-engine-runtime/Cargo.toml',
        'crates/deepdeck-card-domain/Cargo.toml',
        'crates/deepdeck-oracle-engine-contracts/Cargo.toml', 'crates/deepdeck-contract-fixture-runner/Cargo.toml',
        'src/card_catalog.rs', 'src/game_rules.rs', 'src/model.rs', 'src/model_runtime.rs', 'src/oracle.rs',
        'data/builtin-game-pieces.json')) {
        $path = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Contract fingerprint input missing: $relative" }
        $files.Add((Get-Item -LiteralPath $path))
    }
    foreach ($relative in @('crates/deepdeck-oracle/src', 'crates/deepdeck-engine-runtime/src',
        'crates/deepdeck-card-domain/src',
        'crates/deepdeck-oracle-engine-contracts/src', 'crates/deepdeck-contract-fixture-runner/src',
        'src/oracle', 'src/engine')) {
        $path = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Container)) { throw "Contract fingerprint root missing: $relative" }
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File -Filter '*.rs') { $files.Add($file) }
    }
    foreach ($relative in @('crates/deepdeck-oracle/generated', 'crates/deepdeck-engine-runtime/generated',
        'data/ability-catalog', 'data/oracle-vocabulary')) {
        $path = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Container)) { throw "Contract data root missing: $relative" }
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File) { $files.Add($file) }
    }
    foreach ($relative in @('rust-toolchain', 'rust-toolchain.toml', '.cargo/config', '.cargo/config.toml',
        'data/cards-minimized.json', 'crates/deepdeck-oracle/build.rs', 'crates/deepdeck-engine-runtime/build.rs',
        'crates/deepdeck-oracle-engine-contracts/build.rs', 'crates/deepdeck-contract-fixture-runner/build.rs')) {
        $path = Join-Path $repoRoot $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) { $files.Add((Get-Item -LiteralPath $path)) }
    }
    if ($env:MTG_CARD_CATALOG_PATH) {
        $catalog = if ([IO.Path]::IsPathRooted($env:MTG_CARD_CATALOG_PATH)) { $env:MTG_CARD_CATALOG_PATH } else { Join-Path $repoRoot $env:MTG_CARD_CATALOG_PATH }
        $catalog = [IO.Path]::GetFullPath($catalog)
        if (-not (Test-Path -LiteralPath $catalog -PathType Leaf)) { throw "Configured card catalog missing: $catalog" }
        $files.Add((Get-Item -LiteralPath $catalog))
    }
    return @($files | Sort-Object FullName -Unique)
}

function Get-ContractFingerprint([IO.FileInfo[]] $Files) {
    Get-DeepDeckRunnerFingerprint -Schema $fingerprintSchema -Profile $profile -RepoRoot $repoRoot `
        -InputFiles $Files -BuildArguments $buildArguments `
        -Context @{ MTG_CARD_CATALOG_PATH = [Environment]::GetEnvironmentVariable('MTG_CARD_CATALOG_PATH') }
}

function Test-ContractMetadata([string] $Expected) {
    Test-DeepDeckRunnerMetadata -MetadataPath $metadataPath -SchemaVersion $metadataSchema `
        -Profile $profile -TargetPath $targetPath -RunnerPath $runnerPath -ExpectedFingerprint $Expected
}

Push-Location -LiteralPath $repoRoot
try {
    foreach ($requested in $Fixture) {
        if ([string]::IsNullOrWhiteSpace($requested)) { throw 'Fixture arguments must not be empty.' }
        $path = if ([IO.Path]::IsPathRooted($requested)) { $requested } else { Join-Path $repoRoot $requested }
        $arguments += [IO.Path]::GetFullPath($path)
    }
    if ($Build) {
        $lane = [Threading.Mutex]::new($false, (Get-DeepDeckCargoLaneName $targetPath))
        $taken = $false
        $queueTimer = [Diagnostics.Stopwatch]::StartNew()
        try {
            try { $taken = $lane.WaitOne([TimeSpan]::FromSeconds(60)) }
            catch [Threading.AbandonedMutexException] { $taken = $true }
            $queueTimer.Stop(); $queueSeconds = $queueTimer.Elapsed.TotalSeconds
            if (-not $taken) { throw 'Timed out waiting for the card-migration Cargo lane.' }
            $inputFiles = @(Get-ContractRunnerInputFiles)
            $before = Get-ContractFingerprint $inputFiles
            $compileTimer = [Diagnostics.Stopwatch]::StartNew()
            $buildOccurred = $true
            $priorPreference = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $output = @(& cargo @buildArguments 2>&1)
                $cargoExitCode = $LASTEXITCODE
            }
            finally {
                $ErrorActionPreference = $priorPreference
                $compileTimer.Stop(); $compileSeconds = $compileTimer.Elapsed.TotalSeconds
            }
            foreach ($line in $output) {
                $message = $null
                try { $message = $line.ToString() | ConvertFrom-Json -ErrorAction Stop }
                catch { Write-Host $line.ToString(); continue }
                if ($message.reason -eq 'compiler-message') { Write-Host $message.message.rendered }
                if ($message.reason -eq 'compiler-artifact') {
                    if (-not $message.fresh) { $compiledAnything = $true }
                    if ($message.target.name -eq 'deepdeck_engine_runtime') {
                        $runtimeArtifacts += [pscustomobject]@{ package_id = $message.package_id; fresh = [bool]$message.fresh; features = @($message.features) }
                    }
                }
            }
            if ($cargoExitCode -ne 0) { throw "Contract runner build failed with exit code $cargoExitCode." }
            $inputFiles = @(Get-ContractRunnerInputFiles)
            $fingerprint = Get-ContractFingerprint $inputFiles
            if ($fingerprint -cne $before) { throw 'Contract inputs changed during build; rebuild from a frozen snapshot.' }
            Write-DeepDeckRunnerMetadata -MetadataPath $metadataPath -SchemaVersion $metadataSchema -Profile $profile `
                -TargetPath $targetPath -RunnerPath $runnerPath -Fingerprint $fingerprint -InputFileCount $inputFiles.Count
        }
        finally { if ($taken) { $lane.ReleaseMutex() }; $lane.Dispose() }
    } else {
        $inputFiles = @(Get-ContractRunnerInputFiles)
        $fingerprint = Get-ContractFingerprint $inputFiles
    }
    $fingerprintVerified = Test-ContractMetadata $fingerprint
    if (-not $fingerprintVerified) { throw "Contract runner is absent, stale or tampered. Run -Build from a frozen snapshot. Metadata: $metadataPath" }
    $executionTimer = [Diagnostics.Stopwatch]::StartNew()
    $priorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = @(& $runnerPath @arguments)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $priorPreference
        $executionTimer.Stop(); $executionSeconds = $executionTimer.Elapsed.TotalSeconds
    }
    foreach ($line in $output) {
        Write-Output $line
        $result = $line.ToString() | ConvertFrom-Json -ErrorAction Stop
        if ($result.schemaVersion -cne 'deepdeck-contract-fixture-result/v1') { throw 'Unexpected contract result schema.' }
        switch ($result.type) {
            'fixtureResult' { $resultCount++ }
            'fixtureError' { }
            'summary' { if ($null -ne $runnerSummary) { throw 'Multiple contract summaries.' }; $runnerSummary = $result }
            default { throw "Unexpected contract result type: $($result.type)" }
        }
    }
    if ($null -eq $runnerSummary -or $runnerSummary.fixtureCount -ne $resultCount) { throw 'Missing or inconsistent contract summary.' }
}
catch { $failure = $_ }
finally { Pop-Location; $totalTimer.Stop() }

Write-Host ('Contract timing: queue={0:N3}s compile={1:N3}s execute={2:N3}s total={3:N3}s build={4}' -f `
    $queueSeconds, $compileSeconds, $executionSeconds, $totalTimer.Elapsed.TotalSeconds, $buildOccurred)
if ($ReportJson) {
    $path = if ([IO.Path]::IsPathRooted($ReportJson)) { $ReportJson } else { Join-Path $repoRoot $ReportJson }
    $path = [IO.Path]::GetFullPath($path)
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $path))
    $report = [ordered]@{
        schema_version = 'deepdeck-contract-fixture-timing/v1'; generated_at_utc = [DateTime]::UtcNow.ToString('o'); target_directory = $targetPath
        runner_path = $runnerPath; runner_metadata_path = $metadataPath; input_fingerprint = $fingerprint
        fingerprint_verified = $fingerprintVerified; input_file_count = $inputFiles.Count
        fixture_arguments = $arguments; build_occurred = $buildOccurred; cargo_compiled_anything = $compiledAnything
        queue_wait_seconds = $queueSeconds; compile_seconds = $compileSeconds; execution_seconds = $executionSeconds
        total_seconds = $totalTimer.Elapsed.TotalSeconds; runtime_artifacts = $runtimeArtifacts
        runtime_artifacts_fresh = if ($runtimeArtifacts.Count) { @($runtimeArtifacts | Where-Object { -not $_.fresh }).Count -eq 0 } else { $null }
        result_count = $resultCount; summary = $runnerSummary; exit_code = $exitCode
        passed = $null -eq $failure -and $exitCode -eq 0
        error = if ($null -ne $failure) { $failure.Exception.Message } else { $null }
    }
    [IO.File]::WriteAllText($path, ($report | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
}
if ($null -ne $failure) { Write-Error -ErrorAction Continue $failure.Exception.Message; exit 1 }
exit $exitCode
