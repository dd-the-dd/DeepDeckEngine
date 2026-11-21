<#
.SYNOPSIS
Evicts old regenerable incremental groups when a repository target exceeds 40 GiB.
.DESCRIPTION
Only direct children of target/debug/incremental may be removed. Dependencies,
executables and fingerprints remain intact. Recent groups are preferred;
working-session groups are pinned and reparse points are never traversed.
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)][string] $TargetDirectory,
    [ValidateRange(1, [long]::MaxValue)][long] $BudgetBytes = 40GB,
    [ValidateRange(1, 64)][int] $KeepRecentPerCrate = 2,
    [switch] $PassThru,
    [switch] $CargoLaneOwned
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'cargo-incremental-cache.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$lane = $null
$taken = $false
try {
    if (-not $CargoLaneOwned) {
        $lane = [Threading.Mutex]::new($false, (Get-DeepDeckIncrementalCacheLaneName $TargetDirectory))
        try { $taken = $lane.WaitOne([TimeSpan]::FromSeconds(60)) }
        catch [Threading.AbandonedMutexException] { $taken = $true }
        if (-not $taken) { throw 'Timed out waiting for Cargo cache maintenance lane.' }
    }
    $report = Invoke-DeepDeckIncrementalCacheMaintenance -RepositoryRoot $repo -TargetDirectory $TargetDirectory `
        -BudgetBytes $BudgetBytes -KeepRecentPerCrate $KeepRecentPerCrate -WhatIf:$WhatIfPreference
    Write-Host ("Incremental cache: {0}; planned {1}, removed {2} groups ({3:N2} GiB logical)." -f `
        $report.status, @($report.planned_paths).Count, @($report.removed_paths).Count, ($report.removed_logical_bytes / 1GB))
    if ($PassThru) { $report }
}
finally {
    if ($taken) { $lane.ReleaseMutex() }
    if ($null -ne $lane) { $lane.Dispose() }
}
