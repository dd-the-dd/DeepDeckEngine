<#
.SYNOPSIS
Builds and runs the stable data-driven engine runtime fixture runner.

.DESCRIPTION
Use -Build once after runtime sources, manifests, generated runtime data,
toolchain, or build flags change. Without -Build, the script refuses to run an
absent, stale, or modified binary. Fixture JSON files are deliberately excluded
from the binary fingerprint, so fixture-only proof loops do not rebuild.

Directories are expanded recursively and passed to the runner as short paths in
bounded batches. This keeps the full runtime fixture directory below Windows'
command-line limit without creating one process per fixture. Directories named
`known-gaps` are excluded from ordinary gates.

.PARAMETER IncludeKnownGaps
Include fixtures below `known-gaps` directories for an explicit diagnostic run.
An individual known-gap JSON file is always runnable when passed directly.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]] $Fixture,
    [switch] $Build,
    [string] $TargetDirectory,
    [string] $ReportJson,
    [string] $RunnerPath,
    [string] $RunnerMetadataPath,
    [switch] $IncludeKnownGaps,
    [ValidateRange(1024, 30000)][int] $MaxBatchArgumentCharacters = 24000
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'runner-fingerprint.ps1')

$targetDefault = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $repoRoot '.tmp\targets\card-migration'
}
if ([string]::IsNullOrWhiteSpace($TargetDirectory)) {
    $TargetDirectory = $targetDefault
} elseif (-not [IO.Path]::IsPathRooted($TargetDirectory)) {
    $TargetDirectory = Join-Path $repoRoot $TargetDirectory
}
$targetPath = [IO.Path]::GetFullPath($TargetDirectory)
$env:CARGO_TARGET_DIR = $targetPath
$runnerOverrideProvided = -not [string]::IsNullOrWhiteSpace($RunnerPath)
$runnerName = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    'deepdeck-runtime-fixture-runner.exe'
} else {
    'deepdeck-runtime-fixture-runner'
}
if ([string]::IsNullOrWhiteSpace($RunnerPath)) {
    $RunnerPath = Join-Path (Join-Path $targetPath 'debug') $runnerName
} elseif (-not [IO.Path]::IsPathRooted($RunnerPath)) {
    $RunnerPath = Join-Path $repoRoot $RunnerPath
}
$runnerPath = [IO.Path]::GetFullPath($RunnerPath)
if ([string]::IsNullOrWhiteSpace($RunnerMetadataPath)) {
    $RunnerMetadataPath = Join-Path (Split-Path -Parent $runnerPath) 'deepdeck-runtime-fixture-runner.fingerprint.json'
} elseif (-not [IO.Path]::IsPathRooted($RunnerMetadataPath)) {
    $RunnerMetadataPath = Join-Path $repoRoot $RunnerMetadataPath
}
$runnerMetadataPath = [IO.Path]::GetFullPath($RunnerMetadataPath)

$fingerprintSchema = 'deepdeck-runtime-runner-inputs/v1'
$metadataSchema = 'deepdeck-runtime-runner-fingerprint/v1'
$profile = 'debug'
$buildArguments = @('build', '--locked', '-p', 'deepdeck-runtime-fixture-runner')
$buildSeconds = 0.0
$executionSeconds = 0.0
$buildOccurred = $false
$fingerprintVerified = $false
$runnerFingerprint = $null
$resolvedFixtures = @()
$executionBatches = @()
$resultCount = 0
$passedResults = 0
$failedResults = 0
$assertionCount = 0
$failure = $null
$exitCode = 1

function Get-RuntimeRunnerInputFiles {
    $files = [Collections.Generic.List[IO.FileInfo]]::new()
    foreach ($relative in @(
        'Cargo.toml',
        'Cargo.lock',
        'crates\deepdeck-engine-runtime\Cargo.toml',
        'crates\deepdeck-card-domain\Cargo.toml',
        'crates\deepdeck-runtime-fixture-runner\Cargo.toml',
        'data\builtin-game-pieces.json',
        'src\card_catalog.rs',
        'src\game_rules.rs',
        'src\model_runtime.rs'
    )) {
        $path = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Runtime runner fingerprint input is missing: $relative"
        }
        $files.Add((Get-Item -LiteralPath $path))
    }
    foreach ($relativeRoot in @(
        'crates\deepdeck-engine-runtime\src',
        'crates\deepdeck-card-domain\src',
        'data\oracle-vocabulary',
        'crates\deepdeck-engine-runtime\generated',
        'crates\deepdeck-runtime-fixture-runner\src',
        'src\engine'
    )) {
        $path = Join-Path $repoRoot $relativeRoot
        if (-not (Test-Path -LiteralPath $path -PathType Container)) {
            throw "Runtime runner fingerprint root is missing: $relativeRoot"
        }
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File) { $files.Add($file) }
    }
    foreach ($relative in @('rust-toolchain', 'rust-toolchain.toml', '.cargo\config', '.cargo\config.toml')) {
        $path = Join-Path $repoRoot $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) { $files.Add((Get-Item -LiteralPath $path)) }
    }
    return @($files | Sort-Object FullName -Unique)
}

function Resolve-RuntimeFixtures {
    $files = [Collections.Generic.List[IO.FileInfo]]::new()
    foreach ($requested in $Fixture) {
        $candidate = if ([IO.Path]::IsPathRooted($requested)) { $requested } else { Join-Path $repoRoot $requested }
        $candidate = [IO.Path]::GetFullPath($candidate)
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            if ([IO.Path]::GetExtension($candidate) -ine '.json') {
                throw "Runtime fixture is not JSON: $candidate"
            }
            $files.Add((Get-Item -LiteralPath $candidate))
        } elseif (Test-Path -LiteralPath $candidate -PathType Container) {
            foreach ($file in Get-ChildItem -LiteralPath $candidate -Recurse -File -Filter '*.json') {
                if (-not $IncludeKnownGaps) {
                    $relative = $file.FullName.Substring($candidate.TrimEnd([char[]]@('\', '/')).Length)
                    $segments = @($relative.TrimStart([char[]]@('\', '/')) -split '[\\/]')
                    if ($segments -icontains 'known-gaps') { continue }
                }
                $files.Add($file)
            }
        } else {
            throw "Runtime fixture path does not exist: $candidate"
        }
    }
    $result = @($files | Sort-Object FullName -Unique)
    if ($result.Count -eq 0) { throw 'No runtime fixture JSON files were selected.' }
    return $result
}

function Get-RunnerArgument([string] $FullName) {
    $rootPrefix = $repoRoot.TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar
    if ($FullName.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        return $FullName.Substring($rootPrefix.Length)
    }
    return $FullName
}

function New-FixtureBatches([IO.FileInfo[]] $Files) {
    $batches = [Collections.Generic.List[object]]::new()
    foreach ($directoryGroup in @($Files | Group-Object DirectoryName | Sort-Object Name)) {
        $arguments = [Collections.Generic.List[string]]::new()
        $characters = 0
        foreach ($file in @($directoryGroup.Group | Sort-Object FullName)) {
            $argument = Get-RunnerArgument $file.FullName
            $addedCharacters = $argument.Length + 3
            if ($arguments.Count -gt 0 -and $characters + $addedCharacters -gt $MaxBatchArgumentCharacters) {
                $batches.Add([pscustomobject]@{ directory = $directoryGroup.Name; arguments = @($arguments) })
                $arguments = [Collections.Generic.List[string]]::new()
                $characters = 0
            }
            $arguments.Add($argument)
            $characters += $addedCharacters
        }
        if ($arguments.Count -gt 0) {
            $batches.Add([pscustomobject]@{ directory = $directoryGroup.Name; arguments = @($arguments) })
        }
    }
    return @($batches)
}

function Invoke-RunnerBatch([string[]] $Arguments) {
    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        if ([IO.Path]::GetExtension($runnerPath) -ieq '.ps1') {
            $powershell = (Get-Process -Id $PID).Path
            $output = @(& $powershell -NoLogo -NoProfile -File $runnerPath @Arguments)
        } else {
            $output = @(& $runnerPath @Arguments)
        }
        $runnerExitCode = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $priorErrorActionPreference }
    return [pscustomobject]@{ output = $output; exit_code = $runnerExitCode }
}

$inputFiles = @()
Push-Location -LiteralPath $repoRoot
try {
    $inputFiles = @(Get-RuntimeRunnerInputFiles)
    if ($Build -and $runnerOverrideProvided) {
        throw '-Build cannot be combined with a RunnerPath override.'
    }
    if ($Build) {
        $fingerprintBeforeBuild = Get-DeepDeckRunnerFingerprint `
            -Schema $fingerprintSchema -Profile $profile -RepoRoot $repoRoot `
            -InputFiles $inputFiles -BuildArguments $buildArguments
        $lane = [Threading.Mutex]::new($false, (Get-DeepDeckCargoLaneName $targetPath))
        $laneTaken = $false
        try {
            try { $laneTaken = $lane.WaitOne([TimeSpan]::FromSeconds(60)) }
            catch [Threading.AbandonedMutexException] { $laneTaken = $true }
            if (-not $laneTaken) { throw 'Timed out waiting for the card-migration Cargo lane.' }
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $buildOccurred = $true
            $buildOutput = @()
            $priorErrorActionPreference = $ErrorActionPreference
            try {
                # Windows PowerShell otherwise promotes ordinary Cargo progress
                # written on stderr to NativeCommandError in an outer wrapper.
                # Capture both streams here and replay them as plain log lines;
                # Cargo's process exit code remains the only build verdict.
                $ErrorActionPreference = 'Continue'
                $buildOutput = @(& cargo @buildArguments 2>&1)
                $cargoExitCode = $LASTEXITCODE
            }
            finally {
                $ErrorActionPreference = $priorErrorActionPreference
                $timer.Stop()
                $buildSeconds = $timer.Elapsed.TotalSeconds
            }
            foreach ($line in $buildOutput) { Write-Host $line.ToString() }
            if ($cargoExitCode -ne 0) { throw "Runtime fixture runner build failed with exit code $cargoExitCode." }
            $postBuildInputFiles = @(Get-RuntimeRunnerInputFiles)
            $runnerFingerprint = Get-DeepDeckRunnerFingerprint `
                -Schema $fingerprintSchema -Profile $profile -RepoRoot $repoRoot `
                -InputFiles $postBuildInputFiles -BuildArguments $buildArguments
            if ($runnerFingerprint -ne $fingerprintBeforeBuild) {
                throw 'Runtime inputs changed during the runner build; rebuild from a stable source snapshot.'
            }
            $inputFiles = $postBuildInputFiles
            Write-DeepDeckRunnerMetadata -MetadataPath $runnerMetadataPath `
                -SchemaVersion $metadataSchema -Profile $profile -TargetPath $targetPath `
                -RunnerPath $runnerPath -Fingerprint $runnerFingerprint -InputFileCount $inputFiles.Count
            $fingerprintVerified = Test-DeepDeckRunnerMetadata -MetadataPath $runnerMetadataPath `
                -SchemaVersion $metadataSchema -Profile $profile -TargetPath $targetPath `
                -RunnerPath $runnerPath -ExpectedFingerprint $runnerFingerprint
            if (-not $fingerprintVerified) { throw 'Runtime runner fingerprint metadata failed post-build verification.' }
        }
        finally {
            if ($laneTaken) { $lane.ReleaseMutex() }
            $lane.Dispose()
        }
    }
    if (-not $Build) {
        $runnerFingerprint = Get-DeepDeckRunnerFingerprint `
            -Schema $fingerprintSchema -Profile $profile -RepoRoot $repoRoot `
            -InputFiles $inputFiles -BuildArguments $buildArguments
        $fingerprintVerified = Test-DeepDeckRunnerMetadata -MetadataPath $runnerMetadataPath `
            -SchemaVersion $metadataSchema -Profile $profile -TargetPath $targetPath `
            -RunnerPath $runnerPath -ExpectedFingerprint $runnerFingerprint
        if (-not $fingerprintVerified) {
            throw "Runtime fixture runner is absent or stale for current sources, flags, toolchain, target, or binary hash. Run once with -Build after the runtime snapshot stabilizes. Metadata: $runnerMetadataPath"
        }
    }

    $resolvedFixtures = @(Resolve-RuntimeFixtures)
    $batches = @(New-FixtureBatches $resolvedFixtures)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    try {
        $overallExitCode = 0
        foreach ($batch in $batches) {
            $batchResult = Invoke-RunnerBatch $batch.arguments
            $overallExitCode = [Math]::Max($overallExitCode, [int]$batchResult.exit_code)
            $executionBatches += [pscustomobject]@{
                directory = [string]$batch.directory
                fixture_count = @($batch.arguments).Count
                exit_code = [int]$batchResult.exit_code
            }
            foreach ($line in @($batchResult.output)) {
                $text = $line.ToString()
                Write-Output $text
                try { $result = $text | ConvertFrom-Json }
                catch { throw "Runtime runner emitted non-JSON output: $text" }
                $resultCount++
                if ([string]$result.status -eq 'passed') { $passedResults++ } else { $failedResults++ }
                $assertionCount += @($result.assertions).Count
            }
        }
    }
    finally {
        $timer.Stop()
        $executionSeconds = $timer.Elapsed.TotalSeconds
    }
    $exitCode = $overallExitCode
    if ($resultCount -ne $resolvedFixtures.Count) {
        throw "Runtime runner returned $resultCount results for $($resolvedFixtures.Count) fixtures."
    }
}
catch { $failure = $_ }
finally { Pop-Location }

Write-Host ('Runtime fixture timing: build={0:N3}s execute={1:N3}s fixtures={2} batches={3} buildOccurred={4}' -f `
    $buildSeconds, $executionSeconds, @($resolvedFixtures).Count, @($executionBatches).Count, $buildOccurred)

if (-not [string]::IsNullOrWhiteSpace($ReportJson)) {
    $reportPath = if ([IO.Path]::IsPathRooted($ReportJson)) { $ReportJson } else { Join-Path $repoRoot $ReportJson }
    $reportPath = [IO.Path]::GetFullPath($reportPath)
    $reportDirectory = Split-Path -Parent $reportPath
    if (-not (Test-Path -LiteralPath $reportDirectory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $reportDirectory -Force)
    }
    $report = [ordered]@{
        schema_version = 'deepdeck-runtime-fixture-timing/v1'
        generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        target_directory = $targetPath
        runner_path = $runnerPath
        runner_metadata_path = $runnerMetadataPath
        input_fingerprint = $runnerFingerprint
        fingerprint_verified = $fingerprintVerified
        input_file_count = @($inputFiles).Count
        fixtures = @($resolvedFixtures | ForEach-Object { $_.FullName })
        include_known_gaps = [bool]$IncludeKnownGaps
        execution_batches = $executionBatches
        build_occurred = $buildOccurred
        build_seconds = [Math]::Round($buildSeconds, 3)
        execution_seconds = [Math]::Round($executionSeconds, 3)
        result_count = $resultCount
        passed_results = $passedResults
        failed_results = $failedResults
        assertion_count = $assertionCount
        exit_code = $exitCode
        passed = $null -eq $failure -and $exitCode -eq 0 -and $failedResults -eq 0
        error = if ($null -eq $failure) { $null } else { $failure.Exception.Message }
    }
    [IO.File]::WriteAllText($reportPath, ($report | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
}

if ($null -ne $failure) {
    Write-Error -ErrorAction Continue $failure.Exception.Message
    exit 1
}
exit $exitCode
