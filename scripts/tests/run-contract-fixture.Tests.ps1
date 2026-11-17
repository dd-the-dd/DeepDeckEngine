[CmdletBinding()]
param([string] $RunnerBinary, [string] $SmokeRoot, [string] $FingerprintModule)
$ErrorActionPreference = 'Stop'
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$wrapper = Join-Path $sourceRoot 'scripts/run-contract-fixture.ps1'
if (-not $FingerprintModule) { $FingerprintModule = Join-Path $sourceRoot 'scripts/runner-fingerprint.ps1' }
if (-not $RunnerBinary) { $RunnerBinary = Join-Path $sourceRoot '.tmp/targets/card-migration/debug/deepdeck-contract-fixture-runner.exe' }
if (-not $SmokeRoot) { $SmokeRoot = Join-Path $sourceRoot '.tmp/contract-wrapper-smoke' }
$SmokeRoot = [IO.Path]::GetFullPath($SmokeRoot)
$repoRoot = Join-Path $SmokeRoot ('workspace-' + [Guid]::NewGuid().ToString('N'))
$target = Join-Path $repoRoot 'target'
$fixtureRoot = Join-Path $repoRoot 'fixtures'
$report = Join-Path $SmokeRoot 'last-wrapper-report.json'
$metadata = Join-Path $target 'debug/deepdeck-contract-fixture-runner.fingerprint.json'
$binary = Join-Path $target 'debug/deepdeck-contract-fixture-runner.exe'
$utf8 = [Text.UTF8Encoding]::new($false)
function Assert-True([bool] $Condition, [string] $Message) { if (-not $Condition) { throw $Message } }
function Write-File([string] $Path, [string] $Text) {
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $Path))
    [IO.File]::WriteAllText($Path, $Text, $utf8)
}
foreach ($relative in @('Cargo.toml','Cargo.lock','crates/deepdeck-oracle/Cargo.toml',
    'crates/deepdeck-card-domain/Cargo.toml',
    'crates/deepdeck-engine-runtime/Cargo.toml','crates/deepdeck-oracle-engine-contracts/Cargo.toml',
    'crates/deepdeck-contract-fixture-runner/Cargo.toml','src/card_catalog.rs','src/game_rules.rs',
    'src/model.rs','src/model_runtime.rs','src/oracle.rs','data/builtin-game-pieces.json')) {
    Write-File (Join-Path $repoRoot $relative) 'synthetic fingerprint input'
}
foreach ($relative in @('crates/deepdeck-oracle/src','crates/deepdeck-engine-runtime/src',
    'crates/deepdeck-card-domain/src',
    'crates/deepdeck-oracle-engine-contracts/src','crates/deepdeck-contract-fixture-runner/src','src/oracle','src/engine')) {
    Write-File (Join-Path $repoRoot "$relative/synthetic.rs") 'synthetic source'
}
foreach ($relative in @('crates/deepdeck-oracle/generated','crates/deepdeck-engine-runtime/generated',
    'data/ability-catalog','data/oracle-vocabulary')) { Write-File (Join-Path $repoRoot "$relative/snapshot.json") '{}' }
[void][IO.Directory]::CreateDirectory((Join-Path $repoRoot 'scripts'))
Copy-Item -LiteralPath $wrapper -Destination (Join-Path $repoRoot 'scripts/run-contract-fixture.ps1')
Copy-Item -LiteralPath $FingerprintModule -Destination (Join-Path $repoRoot 'scripts/runner-fingerprint.ps1')
[void][IO.Directory]::CreateDirectory((Split-Path -Parent $binary))
Copy-Item -LiteralPath $RunnerBinary -Destination $binary
$wrapper = Join-Path $repoRoot 'scripts/run-contract-fixture.ps1'
. (Join-Path $repoRoot 'scripts/runner-fingerprint.ps1')
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($wrapper, [ref]$tokens, [ref]$parseErrors)
Assert-True ($parseErrors.Count -eq 0) 'Wrapper must parse before smoke execution.'
$function = $ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-ContractRunnerInputFiles'}, $false)
. ([scriptblock]::Create($function.Extent.Text))
$files = @(Get-ContractRunnerInputFiles)
$fingerprint = Get-DeepDeckRunnerFingerprint -Schema 'deepdeck-contract-runner-inputs/v1' -Profile debug -RepoRoot $repoRoot `
    -InputFiles $files -BuildArguments @('build','--locked','-p','deepdeck-contract-fixture-runner','--message-format=json') `
    -Context @{ MTG_CARD_CATALOG_PATH = [Environment]::GetEnvironmentVariable('MTG_CARD_CATALOG_PATH') }
Write-DeepDeckRunnerMetadata -MetadataPath $metadata -SchemaVersion 'deepdeck-contract-runner-fingerprint/v1' -Profile debug `
    -TargetPath $target -RunnerPath $binary -Fingerprint $fingerprint -InputFileCount $files.Count
function Write-Fixture([string] $Name) {
    Write-File (Join-Path $fixtureRoot "$Name.json") (@{
        schemaVersion='deepdeck-contract-fixture/v1'; id="smoke/$Name"; expectedExecutable=$true
        rule=@{kind='spellAbility'; source=@{kind='self'}; effects=@(@{kind='gainLife';player=@{kind='controllerOf';object=@{kind='self'}};amount=@{kind='integer';value=1}})}
    } | ConvertTo-Json -Depth 10)
}
function Invoke-Wrapper {
    $powershell = (Get-Process -Id $PID).Path
    $prior = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $powershell -NoLogo -NoProfile -File $wrapper -Fixture $fixtureRoot -TargetDirectory $target -ReportJson $report 2>$null | Out-Null
        return $LASTEXITCODE
    } finally { $ErrorActionPreference = $prior }
}
Write-Fixture 'one'
Assert-True ((Invoke-Wrapper) -eq 0) 'Fresh metadata must permit no-build execution.'
$result = Get-Content -LiteralPath $report -Raw -Encoding UTF8 | ConvertFrom-Json
Assert-True (-not $result.build_occurred -and $result.compile_seconds -eq 0 -and $result.fingerprint_verified) 'No-build must never invoke Cargo.'
Assert-True ($result.fixture_arguments.Count -eq 1 -and $result.result_count -eq 1) 'A directory must be passed intact to one runner invocation.'
Write-Fixture 'two'
Assert-True ((Invoke-Wrapper) -eq 0) 'A DATA-only addition must not stale the binary.'
$result = Get-Content -LiteralPath $report -Raw -Encoding UTF8 | ConvertFrom-Json
Assert-True ($result.result_count -eq 2 -and -not $result.build_occurred) 'The existing runner must discover both fixtures without a build.'
Copy-Item -LiteralPath (Join-Path $fixtureRoot 'one.json') -Destination (Join-Path $fixtureRoot 'duplicate-id.json')
Assert-True ((Invoke-Wrapper) -ne 0) 'The runner must retain its duplicate-ID failure semantics.'
$result = Get-Content -LiteralPath $report -Raw -Encoding UTF8 | ConvertFrom-Json
Assert-True ($result.summary.invalidFixtures -eq 1) 'Duplicate IDs must remain runner fixtureError, not wrapper deduplication.'
Remove-Item -LiteralPath (Join-Path $fixtureRoot 'duplicate-id.json')
# Exact domain/data mutations must reject metadata before any fixture executes.
foreach ($relative in @('crates/deepdeck-card-domain/Cargo.toml', 'crates/deepdeck-card-domain/src/synthetic.rs', 'data/oracle-vocabulary/snapshot.json')) {
    $changedPath = Join-Path $repoRoot $relative
    $originalBytes = [IO.File]::ReadAllBytes($changedPath)
    try {
        Write-File $changedPath 'changed domain dependency'
        Assert-True ((Invoke-Wrapper) -ne 0) "Changed domain dependency must stale old metadata: $relative"
        $stale = Get-Content -LiteralPath $report -Raw -Encoding UTF8 | ConvertFrom-Json
        Assert-True (-not $stale.fingerprint_verified -and $stale.result_count -eq 0) 'Stale domain metadata must reject before fixture execution.'
    } finally { [IO.File]::WriteAllBytes($changedPath, $originalBytes) }
    Assert-True ((Invoke-Wrapper) -eq 0) 'Restoring exact domain bytes must restore the matching metadata.'
}
$catalogBefore = [Environment]::GetEnvironmentVariable('MTG_CARD_CATALOG_PATH')
try {
    $externalCatalog = Join-Path $SmokeRoot 'external-catalog.json'
    Write-File $externalCatalog '{"catalog":"first"}'
    $env:MTG_CARD_CATALOG_PATH = $externalCatalog
    Assert-True ((Invoke-Wrapper) -ne 0) 'A changed catalog override must stale the old metadata.'
    $externalFiles = @(Get-ContractRunnerInputFiles)
    $externalFingerprint = Get-DeepDeckRunnerFingerprint -Schema 'deepdeck-contract-runner-inputs/v1' -Profile debug -RepoRoot $repoRoot `
        -InputFiles $externalFiles -BuildArguments @('build','--locked','-p','deepdeck-contract-fixture-runner','--message-format=json') `
        -Context @{ MTG_CARD_CATALOG_PATH = $env:MTG_CARD_CATALOG_PATH }
    Write-DeepDeckRunnerMetadata -MetadataPath $metadata -SchemaVersion 'deepdeck-contract-runner-fingerprint/v1' -Profile debug `
        -TargetPath $target -RunnerPath $binary -Fingerprint $externalFingerprint -InputFileCount $externalFiles.Count
    Assert-True ((Invoke-Wrapper) -eq 0) 'An authenticated external local catalog must be supported.'
    Write-File $externalCatalog '{"catalog":"changed"}'
    Assert-True ((Invoke-Wrapper) -ne 0) 'Changing external catalog content must reject the artifact.'
} finally {
    [Environment]::SetEnvironmentVariable('MTG_CARD_CATALOG_PATH', $catalogBefore)
}
Write-DeepDeckRunnerMetadata -MetadataPath $metadata -SchemaVersion 'deepdeck-contract-runner-fingerprint/v1' -Profile debug `
    -TargetPath $target -RunnerPath $binary -Fingerprint $fingerprint -InputFileCount $files.Count
Write-File (Join-Path $repoRoot 'src/model.rs') 'changed synthetic source'
Assert-True ((Invoke-Wrapper) -ne 0) 'A source change must reject the old fingerprint.'
Write-File (Join-Path $repoRoot 'src/model.rs') 'synthetic fingerprint input'
$stream=[IO.File]::Open($binary,[IO.FileMode]::Append,[IO.FileAccess]::Write)
try { $stream.WriteByte(0) } finally { $stream.Dispose() }
Assert-True ((Invoke-Wrapper) -ne 0) 'An executable hash change must reject the artifact before execution.'
Remove-Item -LiteralPath $metadata
Assert-True ((Invoke-Wrapper) -ne 0) 'Missing metadata must reject no-build execution.'
Write-DeepDeckRunnerMetadata -MetadataPath $metadata -SchemaVersion 'deepdeck-contract-runner-fingerprint/v1' -Profile debug `
    -TargetPath $target -RunnerPath $binary -Fingerprint $fingerprint -InputFileCount $files.Count
Remove-Item -LiteralPath $binary
Assert-True ((Invoke-Wrapper) -ne 0) 'Missing executable must reject no-build execution even when metadata exists.'
Write-File (Join-Path $SmokeRoot 'smoke-results.json') (@{
    status='passed'; scope='Synthetic wrapper mechanics only; not current-source runtime certification'
    checks=@('fresh no-build','directory argument preserved','DATA-only addition','duplicate IDs retain failure','catalog override change rejected','external local catalog authenticated','external catalog content change rejected','stale source rejected','tampered binary rejected','missing metadata rejected','missing executable rejected')
    cargoInvoked=$false; workspace=$repoRoot
} | ConvertTo-Json -Depth 5)
Write-Host 'Contract wrapper smoke checks passed; no Cargo invoked.'
