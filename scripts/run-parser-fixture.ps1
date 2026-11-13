<#
.SYNOPSIS
Builds the stable Oracle parser fixture runner once, then executes JSON fixtures directly.

.DESCRIPTION
Use -Build once after parser source changes. Subsequent fixture-only iterations omit
-Build and execute the existing binary without invoking Cargo. The script reports
Cargo build time separately from fixture execution time and uses the shared card-
migration Cargo lane.

.PARAMETER Fixture
One or more parser fixture JSON files or directories.

.PARAMETER Build
Ensure the runner is rebuilt before executing fixtures.

.PARAMETER TargetDirectory
Cargo target directory. Defaults to CARGO_TARGET_DIR, otherwise
.tmp/targets/card-migration.

.PARAMETER ReportJson
Optional timing report path.

.PARAMETER NativeOnly
Run only token-driven native automata. Legacy classification and parsing are
disabled, so an unclaimed ability is reported as unsupported.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]] $Fixture,

    [switch] $Build,

    [string] $TargetDirectory,

    [string] $ReportJson,

    [switch] $NativeOnly
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
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
$runnerName = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    'deepdeck-parser-fixture-runner.exe'
} else {
    'deepdeck-parser-fixture-runner'
}
$runnerPath = Join-Path (Join-Path $targetPath 'debug') $runnerName
$runnerMetadataPath = Join-Path (Join-Path $targetPath 'debug') 'deepdeck-parser-fixture-runner.fingerprint.json'
$buildSeconds = 0.0
$executionSeconds = 0.0
$buildOccurred = $false
$fingerprintVerified = $false
$runnerFingerprint = $null
$resolvedFixtures = @()
$failure = $null
$exitCode = 1

function Get-ParserRunnerInputFiles {
    $files = [Collections.Generic.List[IO.FileInfo]]::new()
    foreach ($relative in @(
        'Cargo.toml',
        'Cargo.lock',
        'crates\deepdeck-oracle\Cargo.toml',
        'crates\deepdeck-parser-fixture-runner\Cargo.toml',
        'data\builtin-game-pieces.json',
        'src\card_catalog.rs',
        'src\oracle.rs'
    )) {
        $path = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Parser runner fingerprint input is missing: $relative"
        }
        $files.Add((Get-Item -LiteralPath $path))
    }
    foreach ($relativeRoot in @(
        'crates\deepdeck-oracle\src',
        'crates\deepdeck-oracle\generated',
        'crates\deepdeck-parser-fixture-runner\src',
        'src\oracle',
        'data\ability-catalog',
        'data\oracle-vocabulary'
    )) {
        $path = Join-Path $repoRoot $relativeRoot
        if (-not (Test-Path -LiteralPath $path -PathType Container)) {
            throw "Parser runner fingerprint root is missing: $relativeRoot"
        }
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File) {
            $files.Add($file)
        }
    }
    foreach ($relative in @(
        'rust-toolchain',
        'rust-toolchain.toml',
        '.cargo\config',
        '.cargo\config.toml'
    )) {
        $path = Join-Path $repoRoot $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $files.Add((Get-Item -LiteralPath $path))
        }
    }
    return @($files | Sort-Object FullName -Unique)
}

function Get-RustCompilerIdentity {
    $compiler = if ([string]::IsNullOrWhiteSpace($env:RUSTC)) { 'rustc' } else { $env:RUSTC }
    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $identity = @(& $compiler -Vv 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $priorErrorActionPreference }
    if ($exitCode -ne 0 -or $identity.Count -eq 0) {
        throw "Cannot fingerprint the Rust compiler through '$compiler -Vv'."
    }
    return ($identity | ForEach-Object { $_.ToString().TrimEnd() }) -join "`n"
}

function Get-ParserRunnerFingerprint {
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add('schema=deepdeck-parser-runner-inputs/v1')
    $lines.Add("profile=debug")
    $lines.Add("rustc=$env:RUSTC")
    $lines.Add("rustc-identity=$(Get-RustCompilerIdentity)")
    $lines.Add("rustflags=$($env:RUSTFLAGS)")
    $lines.Add("encoded-rustflags=$($env:CARGO_ENCODED_RUSTFLAGS)")
    $lines.Add("cargo-build-rustflags=$($env:CARGO_BUILD_RUSTFLAGS)")
    $lines.Add("cargo-build-target=$($env:CARGO_BUILD_TARGET)")
    foreach ($file in Get-ParserRunnerInputFiles) {
        $relative = $file.FullName.Substring($repoRoot.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        $lines.Add("$relative=$hash")
    }
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString(
            $algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n")))
        ).Replace('-', '')
    }
    finally { $algorithm.Dispose() }
}

function Test-ParserRunnerMetadata([string] $ExpectedFingerprint) {
    if (-not (Test-Path -LiteralPath $runnerMetadataPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $runnerPath -PathType Leaf)) {
        return $false
    }
    try { $metadata = Get-Content -LiteralPath $runnerMetadataPath -Raw | ConvertFrom-Json }
    catch { return $false }
    if ([string]$metadata.schema_version -ne 'deepdeck-parser-runner-fingerprint/v1' -or
        [string]$metadata.profile -ne 'debug' -or
        [string]$metadata.target_directory -ne $targetPath -or
        [string]$metadata.executable -ne $runnerPath -or
        [string]$metadata.input_fingerprint -ne $ExpectedFingerprint) {
        return $false
    }
    $actualHash = (Get-FileHash -LiteralPath $runnerPath -Algorithm SHA256).Hash
    return [string]$metadata.executable_hash -eq $actualHash
}

function Write-ParserRunnerMetadata([string] $Fingerprint) {
    if (-not (Test-Path -LiteralPath $runnerPath -PathType Leaf)) {
        throw "Parser fixture runner not found after build: $runnerPath"
    }
    $metadata = [pscustomobject][ordered]@{
        schema_version = 'deepdeck-parser-runner-fingerprint/v1'
        generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        profile = 'debug'
        rustc_identity = Get-RustCompilerIdentity
        target_directory = $targetPath
        executable = $runnerPath
        executable_hash = (Get-FileHash -LiteralPath $runnerPath -Algorithm SHA256).Hash
        input_fingerprint = $Fingerprint
    }
    [IO.File]::WriteAllText(
        $runnerMetadataPath,
        ($metadata | ConvertTo-Json -Depth 6),
        [Text.UTF8Encoding]::new($false)
    )
}

function Get-CargoLaneName([string] $Path) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = [BitConverter]::ToString(
            $algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($Path.ToLowerInvariant()))
        ).Replace('-', '').Substring(0, 32)
        return "Local\DeepDeckCardMigrationCargoLane-$hash"
    } finally {
        $algorithm.Dispose()
    }
}

Push-Location -LiteralPath $repoRoot
try {
    if ($Build) {
        $fingerprintBeforeBuild = Get-ParserRunnerFingerprint
        $lane = [Threading.Mutex]::new($false, (Get-CargoLaneName $targetPath))
        $laneTaken = $false
        try {
            try { $laneTaken = $lane.WaitOne([TimeSpan]::FromSeconds(60)) }
            catch [Threading.AbandonedMutexException] { $laneTaken = $true }
            if (-not $laneTaken) { throw 'Timed out waiting for the card-migration Cargo lane.' }
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $buildOccurred = $true
            $priorErrorActionPreference = $ErrorActionPreference
            try {
                # Windows PowerShell wraps redirected native stderr in error
                # records. Cargo writes ordinary build progress there; only
                # its exit code determines whether compilation succeeded.
                $ErrorActionPreference = 'Continue'
                & cargo build --locked -p deepdeck-parser-fixture-runner
                $cargoExitCode = $LASTEXITCODE
            }
            finally {
                $ErrorActionPreference = $priorErrorActionPreference
                $timer.Stop()
                $buildSeconds = $timer.Elapsed.TotalSeconds
            }
            if ($cargoExitCode -ne 0) {
                throw "Parser fixture runner build failed with exit code $cargoExitCode."
            }
            $runnerFingerprint = Get-ParserRunnerFingerprint
            if ($runnerFingerprint -ne $fingerprintBeforeBuild) {
                throw 'Parser inputs changed during the runner build; rebuild from a stable source snapshot.'
            }
            Write-ParserRunnerMetadata $runnerFingerprint
            $fingerprintVerified = Test-ParserRunnerMetadata $runnerFingerprint
            if (-not $fingerprintVerified) {
                throw 'Parser runner fingerprint metadata failed post-build verification.'
            }
        } finally {
            if ($laneTaken) { $lane.ReleaseMutex() }
            $lane.Dispose()
        }
    }

    if (-not $Build) {
        $runnerFingerprint = Get-ParserRunnerFingerprint
        $fingerprintVerified = Test-ParserRunnerMetadata $runnerFingerprint
        if (-not $fingerprintVerified) {
            throw "Parser fixture runner is absent or stale for the current source fingerprint. Run this command once with -Build after the parser snapshot stabilizes. Metadata: $runnerMetadataPath"
        }
    }
    $resolvedFixtures = @($Fixture | ForEach-Object {
        $candidate = if ([IO.Path]::IsPathRooted($_)) { $_ } else { Join-Path $repoRoot $_ }
        [IO.Path]::GetFullPath($candidate)
    })
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $runnerArguments = @()
    if ($NativeOnly) { $runnerArguments += '--native-only' }
    $runnerArguments += $resolvedFixtures
    & $runnerPath @runnerArguments
    $exitCode = $LASTEXITCODE
    $timer.Stop()
    $executionSeconds = $timer.Elapsed.TotalSeconds
}
catch { $failure = $_ }
finally {
    Pop-Location
}

$parserMode = if ($NativeOnly) { 'nativeOnly' } else { 'full' }
Write-Host ('Parser fixture timing: mode={0} build={1:N3}s execute={2:N3}s buildOccurred={3}' -f `
    $parserMode, $buildSeconds, $executionSeconds, $buildOccurred)

if (-not [string]::IsNullOrWhiteSpace($ReportJson)) {
    $reportPath = if ([IO.Path]::IsPathRooted($ReportJson)) {
        $ReportJson
    } else {
        Join-Path $repoRoot $ReportJson
    }
    $reportDirectory = Split-Path -Parent $reportPath
    if (-not (Test-Path -LiteralPath $reportDirectory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $reportDirectory -Force)
    }
    $report = [ordered]@{
        schema_version = 'deepdeck-parser-fixture-timing/v1'
        generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        parser_mode = $parserMode
        target_directory = $targetPath
        runner_path = $runnerPath
        runner_metadata_path = $runnerMetadataPath
        input_fingerprint = $runnerFingerprint
        fingerprint_verified = $fingerprintVerified
        fixtures = $resolvedFixtures
        build_occurred = $buildOccurred
        build_seconds = [Math]::Round($buildSeconds, 3)
        execution_seconds = [Math]::Round($executionSeconds, 3)
        exit_code = $exitCode
        passed = $null -eq $failure -and $exitCode -eq 0
        error = if ($null -eq $failure) { $null } else { $failure.Exception.Message }
    }
    [IO.File]::WriteAllText(
        [IO.Path]::GetFullPath($reportPath),
        ($report | ConvertTo-Json -Depth 8),
        [Text.UTF8Encoding]::new($false)
    )
}

if ($null -ne $failure) {
    Write-Error -ErrorAction Continue $failure.Exception.Message
    exit 1
}
exit $exitCode
