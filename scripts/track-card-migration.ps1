<#
.SYNOPSIS
Records the development loop for one native card migration.

.DESCRIPTION
Keeps an append-only timeline per card so analysis, implementation, corrective
loops, Cargo compilation, test execution, and regression audits can be reported
separately. The file lives under .tmp by default and is not a coverage proof;
it complements the proof manifest and audit-card-set.ps1.

.EXAMPLE
.\scripts\track-card-migration.ps1 -Action begin -TrackingId priority-129-opt `
    -Card Opt -QueuePosition 129

.EXAMPLE
.\scripts\track-card-migration.ps1 -Action start-stage `
    -TrackingId priority-129-opt -Stage analysis

.EXAMPLE
.\scripts\track-card-migration.ps1 -Action record-command `
    -TrackingId priority-129-opt -Stage focused-tests `
    -Command 'cargo test ...' -WallSeconds 1.42 -CompileSeconds 1.10 `
    -ExecutionSeconds 0.03 -Build yes -ExitCode 0

.EXAMPLE
.\scripts\track-card-migration.ps1 -Action report -TrackingId priority-129-opt
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('begin', 'start-stage', 'end-stage', 'loop', 'record-command', 'complete', 'report')]
    [string] $Action,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[a-z0-9][a-z0-9._-]*$')]
    [string] $TrackingId,

    [string] $Card,
    [string] $OracleId,
    [int] $QueuePosition,
    [string] $Stage,
    [string] $Outcome = 'pending',
    [string] $Details,
    [string] $Command,
    [Nullable[double]] $WallSeconds,
    [Nullable[double]] $CompileSeconds,
    [Nullable[double]] $ExecutionSeconds,
    [ValidateSet('yes', 'no', 'unknown')]
    [string] $Build = 'unknown',
    [Nullable[int]] $ExitCode,
    [string] $Directory = '.tmp/card-migration-timings'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (-not [System.IO.Path]::IsPathRooted($Directory)) {
    $Directory = Join-Path $repoRoot $Directory
}
$Directory = [System.IO.Path]::GetFullPath($Directory)
if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
    [void](New-Item -ItemType Directory -Path $Directory -Force)
}
$trackingPath = Join-Path $Directory "$TrackingId.json"

function Get-UtcTimestamp {
    return [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
}

function Read-TrackingState {
    if (-not (Test-Path -LiteralPath $trackingPath -PathType Leaf)) {
        throw "Tracking file does not exist: $trackingPath. Run -Action begin first."
    }
    return Get-Content -LiteralPath $trackingPath -Raw | ConvertFrom-Json
}

function Write-TrackingState($State) {
    $json = $State | ConvertTo-Json -Depth 20
    $temporaryPath = "$trackingPath.tmp-$PID"
    [System.IO.File]::WriteAllText($temporaryPath, $json, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporaryPath -Destination $trackingPath -Force
}

function Add-Event($State, [hashtable] $Properties) {
    $event = [ordered]@{ timestamp_utc = Get-UtcTimestamp }
    foreach ($key in $Properties.Keys) {
        $event[$key] = $Properties[$key]
    }
    $State.events = @($State.events) + [pscustomobject]$event
}

function Require-Value([string] $Value, [string] $Name) {
    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "$Name is required for action '$Action'."
    }
}

$hashAlgorithm = [Security.Cryptography.SHA256]::Create()
try {
    $pathHash = [BitConverter]::ToString(
        $hashAlgorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($trackingPath.ToLowerInvariant()))
    ).Replace('-', '').Substring(0, 32)
}
finally { $hashAlgorithm.Dispose() }
$mutex = [Threading.Mutex]::new($false, "Local\DeepDeckCardMigrationTiming-$pathHash")
$lockTaken = $false
try {
    try { $lockTaken = $mutex.WaitOne([TimeSpan]::FromSeconds(30)) }
    catch [Threading.AbandonedMutexException] { $lockTaken = $true }
    if (-not $lockTaken) { throw "Timed out waiting for migration timing lock: $trackingPath" }

switch ($Action) {
    'begin' {
        Require-Value $Card '-Card'
        if (Test-Path -LiteralPath $trackingPath -PathType Leaf) {
            throw "Tracking file already exists: $trackingPath"
        }
        $now = Get-UtcTimestamp
        $state = [pscustomobject][ordered]@{
            schema_version = 'deepdeck-card-migration-timing/v1'
            tracking_id = $TrackingId
            card = $Card
            oracle_id = $OracleId
            queue_position = if ($QueuePosition -gt 0) { $QueuePosition } else { $null }
            started_at_utc = $now
            completed_at_utc = $null
            outcome = 'in_progress'
            loop_count = 0
            open_stage = $null
            events = @([pscustomobject][ordered]@{
                timestamp_utc = $now
                kind = 'migration-began'
                details = $Details
            })
        }
        Write-TrackingState $state
        Write-Host "Started migration timing for $Card at $now"
    }
    'start-stage' {
        Require-Value $Stage '-Stage'
        $state = Read-TrackingState
        if ($null -ne $state.completed_at_utc) {
            throw 'The migration is already complete.'
        }
        if ($null -ne $state.open_stage) {
            throw "Stage '$($state.open_stage.name)' is already open."
        }
        $now = Get-UtcTimestamp
        $state.open_stage = [pscustomobject][ordered]@{ name = $Stage; started_at_utc = $now }
        Add-Event $state ([ordered]@{ kind = 'stage-started'; stage = $Stage; details = $Details })
        Write-TrackingState $state
        Write-Host "Started stage '$Stage' at $now"
    }
    'end-stage' {
        $state = Read-TrackingState
        if ($null -eq $state.open_stage) {
            throw 'No stage is open.'
        }
        if (-not [string]::IsNullOrWhiteSpace($Stage) -and $Stage -ne $state.open_stage.name) {
            throw "Open stage is '$($state.open_stage.name)', not '$Stage'."
        }
        $ended = [DateTime]::UtcNow
        $started = [DateTime]::Parse(
            [string]$state.open_stage.started_at_utc,
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind
        )
        $elapsed = ($ended - $started).TotalSeconds
        $closedStage = [string]$state.open_stage.name
        Add-Event $state ([ordered]@{
            kind = 'stage-ended'
            stage = $closedStage
            outcome = $Outcome
            elapsed_seconds = [Math]::Round($elapsed, 3)
            details = $Details
        })
        $state.open_stage = $null
        Write-TrackingState $state
        Write-Host ("Ended stage '{0}' after {1:N3} s ({2})" -f $closedStage, $elapsed, $Outcome)
    }
    'loop' {
        $state = Read-TrackingState
        if ($null -ne $state.completed_at_utc) { throw 'The migration is already complete.' }
        $state.loop_count = [int]$state.loop_count + 1
        $activeStage = if ($null -ne $state.open_stage) { [string]$state.open_stage.name } else { $Stage }
        Add-Event $state ([ordered]@{
            kind = 'corrective-loop'
            loop = [int]$state.loop_count
            stage = $activeStage
            reason = $Details
        })
        Write-TrackingState $state
        Write-Host "Recorded corrective loop $($state.loop_count): $Details"
    }
    'record-command' {
        Require-Value $Stage '-Stage'
        Require-Value $Command '-Command'
        if ($null -eq $WallSeconds) {
            throw '-WallSeconds is required for action record-command.'
        }
        $state = Read-TrackingState
        if ($null -ne $state.completed_at_utc) { throw 'The migration is already complete.' }
        Add-Event $state ([ordered]@{
            kind = 'command'
            stage = $Stage
            command = $Command
            wall_seconds = [Math]::Round([double]$WallSeconds, 3)
            compile_seconds = if ($null -ne $CompileSeconds) { [Math]::Round([double]$CompileSeconds, 3) } else { $null }
            execution_seconds = if ($null -ne $ExecutionSeconds) { [Math]::Round([double]$ExecutionSeconds, 3) } else { $null }
            build_occurred = $Build
            exit_code = if ($null -ne $ExitCode) { [int]$ExitCode } else { $null }
            outcome = $Outcome
            details = $Details
        })
        Write-TrackingState $state
        Write-Host "Recorded command for stage '$Stage' ($(([double]$WallSeconds).ToString('N3')) s)"
    }
    'complete' {
        $state = Read-TrackingState
        if ($null -ne $state.completed_at_utc) { throw 'The migration is already complete.' }
        if ($null -ne $state.open_stage) {
            throw "Close stage '$($state.open_stage.name)' before completing the migration."
        }
        $state.completed_at_utc = Get-UtcTimestamp
        $state.outcome = $Outcome
        Add-Event $state ([ordered]@{ kind = 'migration-completed'; outcome = $Outcome; details = $Details })
        Write-TrackingState $state
        Write-Host "Completed migration timing for $($state.card): $Outcome"
    }
    'report' {
        $state = Read-TrackingState
        $end = if ($null -ne $state.completed_at_utc) {
            [DateTime]::Parse([string]$state.completed_at_utc, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::RoundtripKind)
        }
        else { [DateTime]::UtcNow }
        $start = [DateTime]::Parse([string]$state.started_at_utc, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::RoundtripKind)
        $stageRows = @($state.events | Where-Object { $_.kind -eq 'stage-ended' } | Group-Object stage | ForEach-Object {
            [pscustomobject]@{
                Stage = $_.Name
                Runs = $_.Count
                Seconds = [Math]::Round((($_.Group | Measure-Object elapsed_seconds -Sum).Sum), 3)
            }
        })
        $commands = @($state.events | Where-Object { $_.kind -eq 'command' })
        $wallSeconds = ($end - $start).TotalSeconds
        $stageTotal = ($stageRows | Measure-Object Seconds -Sum).Sum
        if ($null -eq $stageTotal) { $stageTotal = 0.0 }
        $commandWallTotal = ($commands | Measure-Object wall_seconds -Sum).Sum
        if ($null -eq $commandWallTotal) { $commandWallTotal = 0.0 }
        $compileTotal = ($commands | Where-Object { $null -ne $_.compile_seconds } | Measure-Object compile_seconds -Sum).Sum
        $executionTotal = ($commands | Where-Object { $null -ne $_.execution_seconds } | Measure-Object execution_seconds -Sum).Sum
        if ($null -eq $compileTotal) { $compileTotal = 0.0 }
        if ($null -eq $executionTotal) { $executionTotal = 0.0 }
        $betweenStageSeconds = [Math]::Max(0.0, $wallSeconds - [double]$stageTotal)
        $nonCommandStageSeconds = [Math]::Max(0.0, [double]$stageTotal - [double]$commandWallTotal)
        $commandOtherSeconds = [Math]::Max(
            0.0,
            [double]$commandWallTotal - [double]$compileTotal - [double]$executionTotal
        )
        $failedCommands = @($commands | Where-Object {
            $null -ne $_.exit_code -and [int]$_.exit_code -ne 0 -and $_.outcome -ne 'expected-stop'
        })
        Write-Host "Card: $($state.card)  queue: $($state.queue_position)  outcome: $($state.outcome)"
        Write-Host ("Wall clock: {0:N3} s  corrective loops: {1}" -f $wallSeconds, $state.loop_count)
        if ($stageRows.Count -gt 0) { $stageRows | Format-Table -AutoSize }
        Write-Host ("Commands: {0}  compile: {1:N3} s  execution: {2:N3} s" -f $commands.Count, [double]$compileTotal, [double]$executionTotal)
        Write-Host ("Breakdown: stages={0:N3} s  between-stages={1:N3} s  non-command-stage-work={2:N3} s" -f `
            [double]$stageTotal, $betweenStageSeconds, $nonCommandStageSeconds)
        Write-Host ("Command wall: {0:N3} s  other-command-work={1:N3} s  failed-commands={2}" -f `
            [double]$commandWallTotal, $commandOtherSeconds, $failedCommands.Count)
        Write-Host "Tracking JSON: $trackingPath"
    }
}
}
finally {
    if ($lockTaken) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}

