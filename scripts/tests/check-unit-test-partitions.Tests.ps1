$ErrorActionPreference = 'Stop'

$checkerPath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\check-unit-test-partitions.ps1'))
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile(
    $checkerPath,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors.Count) {
    throw "check-unit-test-partitions.ps1 parse errors: $($parseErrors.Message -join '; ')"
}

function Get-FunctionText([string]$Name) {
    $function = $ast.Find({
        param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq $Name
    }, $true)
    if ($null -eq $function) { throw "Missing function $Name." }
    return $function.Extent.Text
}

foreach ($name in @(
    'Get-RelativePath',
    'Test-IsDedicatedEngineTest',
    'Get-AttributeContext',
    'Get-PartitionFeatures',
    'Add-FeatureSet',
    'Get-EngineDedicatedTestInventory'
)) {
    Invoke-Expression (Get-FunctionText $name)
}

function Write-Fixture([string]$RelativePath,[string]$Content) {
    $path = Join-Path $fixtureRoot $RelativePath
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $path))
    [IO.File]::WriteAllText($path, $Content)
}

function Assert-Fails([scriptblock]$Action,[string]$Pattern,[string]$Label) {
    try {
        [void](& $Action)
    }
    catch {
        if ($_.Exception.Message -match $Pattern) { return }
        throw "$Label failed for the wrong reason: $($_.Exception.Message)"
    }
    throw "$Label unexpectedly passed."
}

$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) `
    "deepdeck-partition-check-$PID-$([Guid]::NewGuid().ToString('N'))"
try {
    Write-Fixture 'src/engine/mod.rs' @'
#[cfg(all(
    test,
    any(
        not(feature = "ci-test-partition"),
        feature = "ci-unit-tests-1",
        feature = "ci-unit-tests-2",
        feature = "ci-unit-tests-3",
        feature = "ci-unit-tests-4"
    )
))]
#[path = "tests.rs"]
mod tests;
'@
    Write-Fixture 'src/engine/tests.rs' @'
#[path = "tests/good.rs"]
mod good;
#[path = "tests/missing_cfg.rs"]
mod missing_cfg;
'@
    Write-Fixture 'src/engine/tests/good.rs' @'
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn good_partitioned_test() {}
'@
    Write-Fixture 'src/engine/tests/missing_cfg.rs' @'
#[test]
fn missing_partition_cfg() {}
'@

    Assert-Fails { Get-EngineDedicatedTestInventory $fixtureRoot } `
        'must resolve to exactly one CI partition' 'Dedicated test without cfg'

    Write-Fixture 'src/engine/tests/missing_cfg.rs' @'
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn now_partitioned() {}
'@
    Write-Fixture 'src/engine/tests/orphan.rs' @'
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn orphan_test() {}
'@
    Assert-Fails { Get-EngineDedicatedTestInventory $fixtureRoot } `
        'Orphan engine test files' 'Orphan dedicated module'

    Write-Fixture 'src/engine/tests.rs' @'
#[path = "tests/good.rs"]
mod good;
#[path = "tests/missing_cfg.rs"]
mod missing_cfg;
#[path = "tests/orphan.rs"]
mod orphan;
'@
    $inventory = Get-EngineDedicatedTestInventory $fixtureRoot
    if ($inventory.Files.Count -ne 4 -or
        $inventory.Counts['ci-unit-tests-2'] -ne 1 -or
        $inventory.Counts['ci-unit-tests-3'] -ne 1 -or
        $inventory.Counts['ci-unit-tests-4'] -ne 1) {
        throw 'Valid recursive fixture produced incorrect files or partition counts.'
    }

    Write-Fixture 'src/engine/tests.rs' @'
#[path = "tests/good.rs"]
mod good;
#[path = "tests/missing_cfg.rs"]
mod missing_cfg;
#[path = "tests/orphan.rs"]
mod orphan;
#[path = "tests/does_not_exist.rs"]
mod missing_file;
'@
    Assert-Fails { Get-EngineDedicatedTestInventory $fixtureRoot } `
        'declared test module is missing' 'Missing declared module'
}
finally {
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
}

Write-Host 'Unit-test partition inventory harness passed.'
