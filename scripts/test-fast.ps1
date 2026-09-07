param(
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

& cargo test @CargoArgs
$testExitCode = $LASTEXITCODE

$target = Join-Path $PSScriptRoot '..\target'
if (Test-Path -LiteralPath $target) {
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
