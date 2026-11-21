# Shared focused-runner cache policy. No work is performed by dot-sourcing.
function Get-DeepDeckIncrementalCacheLaneName([string] $TargetDirectory) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes(([IO.Path]::GetFullPath($TargetDirectory)).ToLowerInvariant())
        $hash = [BitConverter]::ToString($algorithm.ComputeHash($bytes)).Replace('-', '').Substring(0, 32)
        return "Local\DeepDeckCardMigrationCargoLane-$hash"
    }
    finally { $algorithm.Dispose() }
}

function Assert-DeepDeckIncrementalRoot([string] $RepositoryRoot, [string] $TargetDirectory) {
    $repo = [IO.Path]::GetFullPath($RepositoryRoot).TrimEnd([char[]]@('\', '/'))
    $target = [IO.Path]::GetFullPath($TargetDirectory).TrimEnd([char[]]@('\', '/'))
    if (-not $target.StartsWith("$repo\", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing cache maintenance outside the repository: $target"
    }
    # Reject junctions/symlinks on every existing ancestor, before any recursion.
    $cacheRoot = [IO.Path]::GetFullPath((Join-Path $target 'debug\incremental'))
    $cursor = $cacheRoot
    while ($cursor.Length -ge $repo.Length) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Refusing a reparse-point cache ancestor: $cursor"
            }
        }
        if ($cursor.Equals($repo, [StringComparison]::OrdinalIgnoreCase)) { break }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
    return $cacheRoot
}

function Assert-DeepDeckIncrementalGroup([string] $CacheRoot, [string] $GroupPath) {
    $cache = [IO.Path]::GetFullPath($CacheRoot).TrimEnd([char[]]@('\', '/'))
    $group = [IO.Path]::GetFullPath($GroupPath).TrimEnd([char[]]@('\', '/'))
    if (-not ([IO.Path]::GetDirectoryName($group)).Equals($cache, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Only a direct incremental cache group can be evicted: $group"
    }
    if (-not $group.StartsWith("$cache\", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Incremental group escaped the cache: $group"
    }
    if (Test-Path -LiteralPath $group) {
        $item = Get-Item -LiteralPath $group -Force
        if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Refusing a non-directory or reparse-point group: $group"
        }
    }
    return $group
}

function Get-DeepDeckCacheTreeStats([string] $Directory) {
    [long] $bytes = 0
    $lastWrite = [DateTime]::MinValue
    $working = $false
    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
        return [pscustomobject]@{ Bytes = $bytes; LastWriteUtc = $lastWrite; Working = $working }
    }
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push([IO.Path]::GetFullPath($Directory))
    while ($pending.Count -gt 0) {
        $path = $pending.Pop()
        $item = Get-Item -LiteralPath $path -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing cache traversal through $path" }
        if ($item.LastWriteTimeUtc -gt $lastWrite) { $lastWrite = $item.LastWriteTimeUtc }
        if ($item.Name.EndsWith('-working', [StringComparison]::OrdinalIgnoreCase)) { $working = $true }
        foreach ($child in Get-ChildItem -LiteralPath $path -Force -ErrorAction Stop) {
            if ($child.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing cache traversal through $($child.FullName)" }
            if ($child.LastWriteTimeUtc -gt $lastWrite) { $lastWrite = $child.LastWriteTimeUtc }
            if ($child.PSIsContainer) { $pending.Push($child.FullName) }
            else { $bytes += [long]$child.Length }
        }
    }
    return [pscustomobject]@{ Bytes = $bytes; LastWriteUtc = $lastWrite; Working = $working }
}

function Get-DeepDeckIncrementalTrimPlan {
    param([long] $TargetBytes, [long] $BudgetBytes, [object[]] $Groups, [int] $KeepRecentPerCrate = 2)
    [long] $incremental = 0
    foreach ($group in $Groups) { $incremental += [long]$group.Bytes }
    [long] $outsideCache = [Math]::Max([long]0, [long]($TargetBytes - $incremental))
    $unreachable = $outsideCache -gt $BudgetBytes
    $protected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($family in @($Groups | Group-Object Family)) {
        foreach ($group in @($family.Group | Sort-Object LastWriteUtc -Descending | Select-Object -First $KeepRecentPerCrate)) {
            [void]$protected.Add([string]$group.Path)
        }
    }
    $candidates = @($Groups | Where-Object { -not $_.Working } | Sort-Object `
        @{ Expression = { if ($protected.Contains([string]$_.Path)) { 1 } else { 0 } } }, LastWriteUtc, Path)
    $selected = [Collections.Generic.List[object]]::new()
    [long] $projected = $TargetBytes
    foreach ($group in $candidates) {
        if ($projected -le $BudgetBytes) { break }
        # If deps alone exceed the budget, deleting hot groups cannot fix it.
        # Keep them instead of repeatedly destroying the only useful warm cache.
        if ($unreachable -and $protected.Contains([string]$group.Path)) { continue }
        $selected.Add($group)
        $projected -= [long]$group.Bytes
    }
    return [pscustomobject]@{
        Selected = @($selected.ToArray()); ProjectedBytes = $projected; IncrementalBytes = $incremental
        OutsideManagedGroupsBytes = $outsideCache; BudgetUnreachable = $unreachable
        ProtectedPaths = @($protected); PinnedWorkingGroups = @($Groups | Where-Object Working).Count
    }
}

function Invoke-DeepDeckIncrementalCacheMaintenance {
    [CmdletBinding(SupportsShouldProcess = $true)]
    param([string] $RepositoryRoot, [string] $TargetDirectory,
        [ValidateRange(1, [long]::MaxValue)][long] $BudgetBytes = 40GB,
        [ValidateRange(1, 64)][int] $KeepRecentPerCrate = 2)
    $cache = Assert-DeepDeckIncrementalRoot $RepositoryRoot $TargetDirectory
    $scan = [Diagnostics.Stopwatch]::StartNew()
    $target = [IO.Path]::GetFullPath($TargetDirectory)
    $targetStats = Get-DeepDeckCacheTreeStats $target
    $groups = @()
    if ($targetStats.Bytes -gt $BudgetBytes -and (Test-Path -LiteralPath $cache -PathType Container)) {
        foreach ($directory in Get-ChildItem -LiteralPath $cache -Directory -Force) {
            $path = Assert-DeepDeckIncrementalGroup $cache $directory.FullName
            $stats = Get-DeepDeckCacheTreeStats $path
            $family = $directory.Name -replace '-[a-z0-9]+$', ''
            $groups += [pscustomobject]@{ Path = $path; Family = $family; Bytes = $stats.Bytes; LastWriteUtc = $stats.LastWriteUtc; Working = $stats.Working }
        }
    }
    $scan.Stop()
    $plan = Get-DeepDeckIncrementalTrimPlan $targetStats.Bytes $BudgetBytes $groups $KeepRecentPerCrate
    $eviction = [Diagnostics.Stopwatch]::StartNew()
    $removed = @()
    $skippedChanged = @()
    [long] $removedBytes = 0
    foreach ($group in $plan.Selected) {
        # Check containment, ancestors and active-session status again at commit.
        [void](Assert-DeepDeckIncrementalRoot $RepositoryRoot $TargetDirectory)
        $path = Assert-DeepDeckIncrementalGroup $cache $group.Path
        if (-not (Test-Path -LiteralPath $path -PathType Container)) {
            $skippedChanged += $path
            continue
        }
        $current = Get-DeepDeckCacheTreeStats $path
        if ($current.Working -or $current.LastWriteUtc -gt $group.LastWriteUtc -or $current.Bytes -ne $group.Bytes) {
            $skippedChanged += $path
            continue
        }
        if ($PSCmdlet.ShouldProcess($path, 'Evict regenerable Cargo incremental group')) {
            Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop
            $removed += $path
            $removedBytes += [long]$current.Bytes
        }
    }
    $eviction.Stop()
    [long] $estimatedAfter = [Math]::Max([long]0, [long]($targetStats.Bytes - $removedBytes))
    $status = if ($targetStats.Bytes -le $BudgetBytes) { 'within_budget' }
        elseif ($WhatIfPreference) { 'dry_run' }
        elseif ($plan.BudgetUnreachable) { 'unreachable_with_incremental_groups_only' }
        elseif ($estimatedAfter -gt $BudgetBytes) { 'budget_not_reached_pinned_or_changed_groups' }
        else { 'trimmed' }
    return [pscustomobject][ordered]@{
        status = $status; budget_bytes = $BudgetBytes; target_before_bytes = $targetStats.Bytes
        outside_managed_groups_bytes = if ($targetStats.Bytes -gt $BudgetBytes) { $plan.OutsideManagedGroupsBytes } else { $null }
        incremental_group_bytes = if ($targetStats.Bytes -gt $BudgetBytes) { $plan.IncrementalBytes } else { $null }
        projected_target_bytes = $plan.ProjectedBytes; planned_paths = @($plan.Selected | ForEach-Object { $_.Path })
        removed_paths = $removed; removed_logical_bytes = $removedBytes
        estimated_target_after_bytes = $estimatedAfter; skipped_changed_paths = $skippedChanged
        budget_unreachable_with_incremental_groups_only = $plan.BudgetUnreachable
        protected_paths = $plan.ProtectedPaths; pinned_working_groups = $plan.PinnedWorkingGroups
        scan_seconds = $scan.Elapsed.TotalSeconds; eviction_seconds = $eviction.Elapsed.TotalSeconds
        keep_recent_per_crate = $KeepRecentPerCrate
    }
}
