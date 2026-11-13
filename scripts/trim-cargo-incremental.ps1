<#
.SYNOPSIS
Trims only Cargo's regenerable incremental cache when a workspace target exceeds a budget.

.DESCRIPTION
The target must live inside this repository. The script resolves and validates
the exact debug\incremental child before removing it; compiled dependencies,
test executables, fingerprints, and build-script outputs are preserved.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $TargetDirectory,

    [ValidateRange(1, [long]::MaxValue)]
    [long] $BudgetBytes = 40GB
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\')
$resolvedTarget = [IO.Path]::GetFullPath($TargetDirectory).TrimEnd('\')
if (-not $resolvedTarget.StartsWith("$repositoryRoot\", [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to trim a Cargo target outside the repository: $resolvedTarget"
}
if (-not (Test-Path -LiteralPath $resolvedTarget -PathType Container)) {
    return
}

$targetBytes = (Get-ChildItem -LiteralPath $resolvedTarget -File -Recurse -ErrorAction Stop |
    Measure-Object -Property Length -Sum).Sum
if ([long]$targetBytes -le $BudgetBytes) {
    return
}

$incrementalCache = [IO.Path]::GetFullPath((Join-Path $resolvedTarget 'debug\incremental'))
$expectedCache = "$resolvedTarget\debug\incremental"
if (-not $incrementalCache.Equals($expectedCache, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Unexpected incremental-cache target: $incrementalCache"
}
if (-not $incrementalCache.StartsWith("$resolvedTarget\", [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to trim a cache outside the Cargo target: $incrementalCache"
}
if (-not (Test-Path -LiteralPath $incrementalCache -PathType Container)) {
    return
}

$cacheBytes = (Get-ChildItem -LiteralPath $incrementalCache -File -Recurse -ErrorAction Stop |
    Measure-Object -Property Length -Sum).Sum
Remove-Item -LiteralPath $incrementalCache -Recurse -Force -ErrorAction Stop
Write-Host ("Cargo target exceeded {0:N2} GiB; trimmed {1:N2} GiB from {2}" -f ($BudgetBytes / 1GB), ([long]$cacheBytes / 1GB), $incrementalCache)
