$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$helper = Join-Path $repo 'scripts/cargo-incremental-cache.ps1'
$cases = @(
    @{ Target=40GB; Sizes=@(10GB); Expected=40GB; Outside=30GB; Count=0; Unreachable=$false },
    @{ Target=50GB; Sizes=@(10GB,10GB,10GB); Expected=40GB; Outside=20GB; Count=1; Unreachable=$false },
    @{ Target=70GB; Sizes=@(10GB,10GB,10GB); Expected=40GB; Outside=40GB; Count=3; Unreachable=$false },
    @{ Target=70GB; Sizes=@(5GB,5GB,5GB); Expected=65GB; Outside=55GB; Count=1; Unreachable=$true }
)
& {
    . $helper
    foreach ($case in $cases) {
        $groups = @(for ($i=0; $i -lt $case.Sizes.Count; $i++) {
            [pscustomobject]@{ Path="D:\synthetic-only\engine-$i"; Family='engine'; Bytes=[long]$case.Sizes[$i]
                LastWriteUtc=([DateTime]'2026-01-01T00:00:00Z').AddDays($i); Working=$false }
        })
        $plan = Get-DeepDeckIncrementalTrimPlan -TargetBytes $case.Target -BudgetBytes 40GB -Groups $groups
        if ($plan.ProjectedBytes -ne $case.Expected -or $plan.OutsideManagedGroupsBytes -ne $case.Outside -or
            @($plan.Selected).Count -ne $case.Count -or $plan.BudgetUnreachable -ne $case.Unreachable) {
            throw "Wrong Int64 eviction plan for $($case.Target) bytes"
        }
    }
}
# Exercise the maintenance estimate without allocating large files or touching
# the actual Cargo target. Only the metadata reader is replaced in this scope.
$scratch = Join-Path $repo ('.tmp/cache-regression-' + [Guid]::NewGuid().ToString('N'))
if (-not ([IO.Path]::GetFullPath($scratch)).StartsWith("$repo\.tmp\", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Synthetic cache path escaped the test workspace.'
}
$target = Join-Path $scratch 'target'
[void][IO.Directory]::CreateDirectory($target)
$report = & {
    . $helper
    function Get-DeepDeckCacheTreeStats([string] $Directory) {
        [pscustomobject]@{ Bytes=[long]38GB; LastWriteUtc=([DateTime]'2026-01-01T00:00:00Z'); Working=$false }
    }
    Invoke-DeepDeckIncrementalCacheMaintenance -RepositoryRoot $scratch -TargetDirectory $target -WhatIf
}
if ($report.estimated_target_after_bytes -ne 38GB -or @($report.removed_paths).Count) {
    throw 'Int64 maintenance estimate must preserve all bytes.'
}
Write-Host '5 cache Int64 regressions passed; no Cargo, large allocation or deletion.'
