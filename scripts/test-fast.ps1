param(
    [switch] $SkipCacheTrim,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CargoArgs
)

$ErrorActionPreference = 'Stop'
$TargetBudgetBytes = 12GB
if (-not $env:CARGO_BUILD_JOBS) {
    $env:CARGO_BUILD_JOBS = '1'
}
if (-not $env:RUST_MIN_STACK) {
    $env:RUST_MIN_STACK = '33554432'
}
if (-not $env:MTG_CARD_CATALOG_PATH) {
    $fullCatalog = Join-Path $PSScriptRoot '..\data\cards-minimized.json'
    $fixtureCatalog = Join-Path $PSScriptRoot '..\tests\fixtures\local-app-cards.json'
    if (-not (Test-Path -LiteralPath $fullCatalog) -and (Test-Path -LiteralPath $fixtureCatalog)) {
        $env:MTG_CARD_CATALOG_PATH = (Resolve-Path -LiteralPath $fixtureCatalog).Path
    }
}

& cargo test @CargoArgs
$testExitCode = $LASTEXITCODE

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot '..\target' }
if (-not $SkipCacheTrim -and (Test-Path -LiteralPath $target)) {
    $targetBytes = (Get-ChildItem -LiteralPath $target -File -Recurse |
        Measure-Object -Property Length -Sum).Sum
    if ($targetBytes -gt $TargetBudgetBytes) {
        $incremental = Join-Path $target 'debug\incremental'
        if (Test-Path -LiteralPath $incremental) {
            $resolvedTarget = (Resolve-Path -LiteralPath $target).Path.TrimEnd('\')
            $resolvedIncremental = (Resolve-Path -LiteralPath $incremental).Path
            if (-not $resolvedIncremental.StartsWith("$resolvedTarget\", [StringComparison]::OrdinalIgnoreCase)) {
                throw "Refusing to trim a cache outside the Cargo target directory: $resolvedIncremental"
            }
            Remove-Item -LiteralPath $resolvedIncremental -Recurse -Force
            Write-Host "Cargo target exceeded $TargetBudgetBytes bytes; old incremental state was trimmed."
        }
    }
}

exit $testExitCode
