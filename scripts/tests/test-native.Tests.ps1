$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$scriptPath = Join-Path $repoRoot 'scripts\test-native.ps1'
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) `
    "deepdeck-native-test-inventory-$PID-$([Guid]::NewGuid().ToString('N'))"
$reportPath = Join-Path $temporaryDirectory 'inventory.json'

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw $Message }
}

function Get-TestCount([IO.FileInfo[]] $Files) {
    $count = 0
    foreach ($file in $Files) {
        $count += [regex]::Matches(
            [IO.File]::ReadAllText($file.FullName),
            '(?m)^\s*#\[test\]\s*$'
        ).Count
    }
    return $count
}

try {
    [void](New-Item -ItemType Directory -Path $temporaryDirectory -Force)
    $report = $null
    $historicalFiles = @()
    $fixtureFiles = @()
    $caseCount = 0
    foreach ($attempt in 1..3) {
        & $scriptPath -InventoryOnly -ReportJson $reportPath
        Assert-True ($LASTEXITCODE -eq 0) 'Inventory-only native profile failed.'
        Assert-True (Test-Path -LiteralPath $reportPath -PathType Leaf) `
            'Inventory-only native profile did not write its report.'
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        $historicalFiles = @(Get-ChildItem `
            (Join-Path $repoRoot 'crates\deepdeck-oracle\tests') -File -Filter '*.rs')
        $fixtureFiles = @(Get-ChildItem `
            (Join-Path $repoRoot 'crates\deepdeck-parser-fixture-runner\fixtures') `
            -File -Filter '*.json')
        $caseCount = 0
        foreach ($file in $fixtureFiles) {
            $fixture = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
            $caseCount += @($fixture.cases).Count
        }
        if ($report.inventory.historical_rust_harnesses.parser.target_count -eq $historicalFiles.Count -and
            $report.inventory.native_profile.fixtures.fixture_count -eq $fixtureFiles.Count -and
            $report.inventory.native_profile.fixtures.case_count -eq $caseCount) {
            break
        }
        if ($attempt -eq 3) {
            throw 'Test inventory did not stabilize after three read-only snapshots.'
        }
    }
    Assert-True ($report.schema_version -eq 'deepdeck-native-test-profile/v1') `
        'Unexpected native test profile report schema.'
    Assert-True ($report.mode -eq 'inventory-only' -and -not $report.cargo_invoked) `
        'Inventory-only mode must never invoke Cargo.'

    $historical = $report.inventory.historical_rust_harnesses.parser
    Assert-True ($historical.target_count -eq $historicalFiles.Count) `
        'Historical parser harness count differs from the filesystem.'
    Assert-True ($historical.test_count -eq (Get-TestCount $historicalFiles)) `
        'Historical parser test count differs from the filesystem.'
    Assert-True (@($historical.targets).Count -eq $historicalFiles.Count) `
        'Historical target-name inventory is incomplete.'

    $native = $report.inventory.native_profile
    Assert-True ($native.cargo_test_harnesses -eq 0) `
        'Native profile must not compile Rust test harnesses.'
    Assert-True ($native.stable_runner_binaries -eq 1) `
        'Native profile must use exactly one stable runner binary.'
    Assert-True ($native.fixtures.fixture_count -eq $fixtureFiles.Count -and
        $native.fixtures.case_count -eq $caseCount) `
        'Native fixture inventory differs from the filesystem.'

    foreach ($relative in @($report.inventory.preserved_edge_proofs)) {
        Assert-True (Test-Path -LiteralPath (Join-Path $repoRoot $relative) -PathType Leaf) `
            "Preserved edge proof is missing: $relative"
    }

    Write-Host 'Native/legacy static test-boundary inventory passed.' -ForegroundColor Green
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory -PathType Container) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
}
