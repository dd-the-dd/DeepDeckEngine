$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$wrapperPath = Join-Path $repoRoot 'scripts\run-runtime-fixture.ps1'
$fingerprintModule = Join-Path $repoRoot 'scripts\runner-fingerprint.ps1'
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) `
    "deepdeck-runtime-wrapper-$PID-$([Guid]::NewGuid().ToString('N'))"
$targetPath = Join-Path $temporaryDirectory 'target-alias'
$runnerPath = Join-Path $temporaryDirectory 'fake-runtime-runner.ps1'
$metadataPath = Join-Path $temporaryDirectory 'fake-runtime-runner.fingerprint.json'
$fixtureRoot = Join-Path $temporaryDirectory 'fixtures'
$reportPath = Join-Path $temporaryDirectory 'report.json'

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw $Message }
}

function Invoke-IsolatedWrapper([string[]] $Arguments) {
    $powershell = (Get-Process -Id $PID).Path
    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $powershell -NoLogo -NoProfile -File $wrapperPath @Arguments 2>$null | Out-Null
        return $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $priorErrorActionPreference }
}

# Import the production enumerator so test metadata cannot omit a newly required dependency.
$parseTokens = $null; $parseErrors = $null
$wrapperAst = [Management.Automation.Language.Parser]::ParseFile($wrapperPath, [ref]$parseTokens, [ref]$parseErrors)
Assert-True ($parseErrors.Count -eq 0) 'Runtime wrapper must parse.'
$inputFunction = $wrapperAst.Find({param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-RuntimeRunnerInputFiles'
}, $false)
Assert-True ($null -ne $inputFunction) 'Runtime input enumerator must exist.'
. ([scriptblock]::Create($inputFunction.Extent.Text))
function Get-RuntimeInputFiles { return @(Get-RuntimeRunnerInputFiles) }

try {
    [void](New-Item -ItemType Directory -Path $targetPath, $fixtureRoot -Force)
    [IO.File]::WriteAllText($runnerPath, @'
param([Parameter(ValueFromRemainingArguments = $true)][string[]] $Fixtures)
foreach ($fixture in $Fixtures) {
    $id = [IO.Path]::GetFileNameWithoutExtension($fixture)
    [pscustomobject]@{
        schemaVersion = 'deepdeck-runtime-fixture-result/v1'
        fixtureId = $id
        status = 'passed'
        assertions = @([pscustomobject]@{ index = 0; passed = $true })
    } | ConvertTo-Json -Depth 5 -Compress
}
exit 0
'@, [Text.UTF8Encoding]::new($false))
    foreach ($groupIndex in 0..3) {
        [void](New-Item -ItemType Directory -Path (Join-Path $fixtureRoot "group-$groupIndex") -Force)
    }
    foreach ($index in 0..346) {
        $group = Join-Path $fixtureRoot ("group-{0}" -f ($index % 4))
        $path = Join-Path $group ("fixture-{0:D3}.json" -f $index)
        [IO.File]::WriteAllText($path, '{"fixture":true}', [Text.UTF8Encoding]::new($false))
    }
    $knownGapRoot = Join-Path $fixtureRoot 'known-gaps'
    [void](New-Item -ItemType Directory -Path $knownGapRoot -Force)
    $knownGapFixture = Join-Path $knownGapRoot 'expected-red.json'
    [IO.File]::WriteAllText($knownGapFixture, '{"fixture":"known-gap"}', [Text.UTF8Encoding]::new($false))

    . $fingerprintModule
    $buildArguments = @('build', '--locked', '-p', 'deepdeck-runtime-fixture-runner')
    $inputFiles = @(Get-RuntimeInputFiles)
    $fingerprint = Get-DeepDeckRunnerFingerprint `
        -Schema 'deepdeck-runtime-runner-inputs/v1' -Profile 'debug' `
        -RepoRoot $repoRoot -InputFiles $inputFiles -BuildArguments $buildArguments
    Write-DeepDeckRunnerMetadata -MetadataPath $metadataPath `
        -SchemaVersion 'deepdeck-runtime-runner-fingerprint/v1' -Profile 'debug' `
        -TargetPath ([IO.Path]::GetFullPath($targetPath)) -RunnerPath ([IO.Path]::GetFullPath($runnerPath)) `
        -Fingerprint $fingerprint -InputFileCount $inputFiles.Count

    $commonArguments = @(
        '-Fixture', $fixtureRoot,
        '-TargetDirectory', $targetPath,
        '-RunnerPath', $runnerPath,
        '-RunnerMetadataPath', $metadataPath,
        '-ReportJson', $reportPath,
        '-MaxBatchArgumentCharacters', '4096'
    )
    Assert-True ((Invoke-IsolatedWrapper $commonArguments) -eq 0) `
        'Fresh fingerprint metadata should allow fixture-only execution.'
    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-True (-not $report.build_occurred -and $report.fingerprint_verified) `
        'Fixture-only execution must not build and must verify provenance.'
    Assert-True ($report.result_count -eq 347 -and $report.passed_results -eq 347 -and $report.assertion_count -eq 347) `
        'Directory expansion should execute all 347 nested JSON fixtures.'
    Assert-True (@($report.execution_batches).Count -gt 4) `
        'The 347-fixture directory should be split into bounded batches.'
    Assert-True (-not $report.include_known_gaps) `
        'Known-gap directories must be excluded from the ordinary runtime gate.'

    $includeKnownGapsArguments = @($commonArguments) + '-IncludeKnownGaps'
    Assert-True ((Invoke-IsolatedWrapper $includeKnownGapsArguments) -eq 0) `
        'The diagnostic switch should include known-gap directories.'
    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-True ($report.result_count -eq 348 -and $report.include_known_gaps) `
        'IncludeKnownGaps should add the expected-red fixture to the diagnostic run.'

    $explicitKnownGapArguments = @(
        '-Fixture', $knownGapFixture,
        '-TargetDirectory', $targetPath,
        '-RunnerPath', $runnerPath,
        '-RunnerMetadataPath', $metadataPath,
        '-ReportJson', $reportPath
    )
    Assert-True ((Invoke-IsolatedWrapper $explicitKnownGapArguments) -eq 0) `
        'An explicitly requested known-gap file should remain runnable without the directory switch.'
    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-True ($report.result_count -eq 1 -and -not $report.include_known_gaps) `
        'Explicit known-gap execution should run exactly the requested file.'

    [IO.File]::WriteAllText(
        (Join-Path $fixtureRoot 'three.json'),
        '{"fixture":"added-without-rebuild"}',
        [Text.UTF8Encoding]::new($false)
    )
    Assert-True ((Invoke-IsolatedWrapper $commonArguments) -eq 0) `
        'Adding fixture JSON must not stale the runner fingerprint.'
    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-True ($report.result_count -eq 348 -and -not $report.build_occurred) `
        'Fixture-only addition should execute without a build.'

    Add-Content -LiteralPath $runnerPath -Value '# binary mutation' -Encoding UTF8
    Assert-True ((Invoke-IsolatedWrapper $commonArguments) -ne 0) `
        'A changed runner binary must be rejected by NoBuild provenance checks.'
    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-True (-not $report.passed -and -not $report.fingerprint_verified) `
        'Binary mismatch report should identify failed fingerprint verification.'

    $metadata = Get-Content -LiteralPath $metadataPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $metadata.executable_hash = (Get-FileHash -LiteralPath $runnerPath -Algorithm SHA256).Hash
    $metadata.input_fingerprint = 'STALE-SOURCE-FINGERPRINT'
    [IO.File]::WriteAllText($metadataPath, ($metadata | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    Assert-True ((Invoke-IsolatedWrapper $commonArguments) -ne 0) `
        'A source fingerprint mismatch must be rejected by NoBuild provenance checks.'

    Write-Host 'run-runtime-fixture provenance tests passed.' -ForegroundColor Green
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory -PathType Container) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
}
