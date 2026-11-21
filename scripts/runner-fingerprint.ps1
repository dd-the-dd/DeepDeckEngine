Set-StrictMode -Version Latest

function Get-DeepDeckRustCompilerIdentity {
    $compiler = if ([string]::IsNullOrWhiteSpace($env:RUSTC)) { 'rustc' } else { $env:RUSTC }
    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $identity = @(& $compiler -Vv 2>&1)
        $compilerExitCode = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $priorErrorActionPreference }
    if ($compilerExitCode -ne 0 -or $identity.Count -eq 0) {
        throw "Cannot fingerprint the Rust compiler through '$compiler -Vv'."
    }
    return ($identity | ForEach-Object { $_.ToString().TrimEnd() }) -join "`n"
}

function Get-DeepDeckRunnerFingerprint {
    param(
        [Parameter(Mandatory = $true)][string] $Schema,
        [Parameter(Mandatory = $true)][string] $Profile,
        [Parameter(Mandatory = $true)][string] $RepoRoot,
        [Parameter(Mandatory = $true)][IO.FileInfo[]] $InputFiles,
        [string[]] $BuildArguments = @(),
        [hashtable] $Context = @{}
    )

    $root = [IO.Path]::GetFullPath($RepoRoot).TrimEnd([char[]]@('\', '/'))
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add("schema=$Schema")
    $lines.Add("profile=$Profile")
    $lines.Add("build-arguments=$($BuildArguments -join [char]0x1f)")
    $lines.Add("rustc=$env:RUSTC")
    $lines.Add("rustc-identity=$(Get-DeepDeckRustCompilerIdentity)")
    foreach ($name in @(
        'RUSTC_WRAPPER',
        'RUSTC_WORKSPACE_WRAPPER',
        'RUSTFLAGS',
        'CARGO_ENCODED_RUSTFLAGS',
        'CARGO_BUILD_RUSTFLAGS',
        'CARGO_BUILD_TARGET',
        'CARGO_PROFILE_DEV_OPT_LEVEL',
        'CARGO_PROFILE_DEV_DEBUG',
        'CARGO_PROFILE_DEV_LTO',
        'CARGO_PROFILE_DEV_CODEGEN_UNITS'
    )) {
        $lines.Add("env:$name=$([Environment]::GetEnvironmentVariable($name))")
    }
    foreach ($key in @($Context.Keys | Sort-Object)) {
        $lines.Add("context:$key=$($Context[$key])")
    }
    foreach ($file in @($InputFiles | Sort-Object FullName -Unique)) {
        if (-not $file.Exists) { throw "Runner fingerprint input is missing: $($file.FullName)" }
        $fullName = [IO.Path]::GetFullPath($file.FullName)
        $prefix = $root + [IO.Path]::DirectorySeparatorChar
        $comparison = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
            [StringComparison]::OrdinalIgnoreCase
        } else { [StringComparison]::Ordinal }
        $relative = if ($fullName.StartsWith($prefix, $comparison)) {
            $file.FullName.Substring($root.Length).TrimStart([char[]]@('\', '/')).Replace('\', '/')
        } else { 'external:' + $fullName.Replace('\', '/') }
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

function Get-DeepDeckCargoLaneName {
    param([Parameter(Mandatory = $true)][string] $TargetPath)

    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = [BitConverter]::ToString(
            $algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($TargetPath.ToLowerInvariant()))
        ).Replace('-', '').Substring(0, 32)
        return "Local\DeepDeckCardMigrationCargoLane-$hash"
    }
    finally { $algorithm.Dispose() }
}

function Test-DeepDeckRunnerMetadata {
    param(
        [Parameter(Mandatory = $true)][string] $MetadataPath,
        [Parameter(Mandatory = $true)][string] $SchemaVersion,
        [Parameter(Mandatory = $true)][string] $Profile,
        [Parameter(Mandatory = $true)][string] $TargetPath,
        [Parameter(Mandatory = $true)][string] $RunnerPath,
        [Parameter(Mandatory = $true)][string] $ExpectedFingerprint
    )

    if (-not (Test-Path -LiteralPath $MetadataPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $RunnerPath -PathType Leaf)) {
        return $false
    }
    try { $metadata = Get-Content -LiteralPath $MetadataPath -Raw -Encoding UTF8 | ConvertFrom-Json }
    catch { return $false }
    if ([string]$metadata.schema_version -ne $SchemaVersion -or
        [string]$metadata.profile -ne $Profile -or
        [string]$metadata.target_directory -ne $TargetPath -or
        [string]$metadata.executable -ne $RunnerPath -or
        [string]$metadata.input_fingerprint -ne $ExpectedFingerprint) {
        return $false
    }
    $actualHash = (Get-FileHash -LiteralPath $RunnerPath -Algorithm SHA256).Hash
    return [string]$metadata.executable_hash -eq $actualHash
}

function Write-DeepDeckRunnerMetadata {
    param(
        [Parameter(Mandatory = $true)][string] $MetadataPath,
        [Parameter(Mandatory = $true)][string] $SchemaVersion,
        [Parameter(Mandatory = $true)][string] $Profile,
        [Parameter(Mandatory = $true)][string] $TargetPath,
        [Parameter(Mandatory = $true)][string] $RunnerPath,
        [Parameter(Mandatory = $true)][string] $Fingerprint,
        [Parameter(Mandatory = $true)][int] $InputFileCount
    )

    if (-not (Test-Path -LiteralPath $RunnerPath -PathType Leaf)) {
        throw "Fixture runner not found after build: $RunnerPath"
    }
    $metadata = [pscustomobject][ordered]@{
        schema_version = $SchemaVersion
        generated_at_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        profile = $Profile
        rustc_identity = Get-DeepDeckRustCompilerIdentity
        target_directory = $TargetPath
        executable = $RunnerPath
        executable_hash = (Get-FileHash -LiteralPath $RunnerPath -Algorithm SHA256).Hash
        input_fingerprint = $Fingerprint
        input_file_count = $InputFileCount
    }
    $directory = Split-Path -Parent $MetadataPath
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
        [void](New-Item -ItemType Directory -Path $directory -Force)
    }
    [IO.File]::WriteAllText(
        $MetadataPath,
        ($metadata | ConvertTo-Json -Depth 6),
        [Text.UTF8Encoding]::new($false)
    )
}
