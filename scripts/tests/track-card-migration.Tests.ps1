$ErrorActionPreference = 'Stop'
$scriptPath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\track-card-migration.ps1'))
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-migration-timing-{0}" -f [Guid]::NewGuid().ToString('N'))

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw $Message }
}

try {
    & $scriptPath -Action begin -TrackingId test-card -Card 'Test Card' `
        -QueuePosition 7 -Directory $temporaryDirectory
    & $scriptPath -Action start-stage -TrackingId test-card -Stage analysis `
        -Directory $temporaryDirectory
    & $scriptPath -Action loop -TrackingId test-card -Details 'expected-token mismatch' `
        -Directory $temporaryDirectory
    & $scriptPath -Action record-command -TrackingId test-card -Stage focused-tests `
        -Command 'cargo test --no-run' -WallSeconds 1.5 -CompileSeconds 1.2 `
        -ExecutionSeconds 0.2 -Build yes -ExitCode 0 -Outcome passed `
        -Directory $temporaryDirectory
    & $scriptPath -Action end-stage -TrackingId test-card -Stage analysis `
        -Outcome passed -Directory $temporaryDirectory
    & $scriptPath -Action complete -TrackingId test-card -Outcome native-primitive-executable `
        -Directory $temporaryDirectory

    $state = Get-Content -LiteralPath (Join-Path $temporaryDirectory 'test-card.json') -Raw | ConvertFrom-Json
    Assert-True ($state.schema_version -eq 'deepdeck-card-migration-timing/v1') 'Unexpected timing schema.'
    Assert-True ($state.card -eq 'Test Card' -and $state.queue_position -eq 7) 'Card identity was not preserved.'
    Assert-True ($state.loop_count -eq 1) 'Corrective loop count was not persisted.'
    Assert-True ($state.outcome -eq 'native-primitive-executable') 'Completion outcome was not persisted.'
    $command = @($state.events | Where-Object { $_.kind -eq 'command' })[0]
    Assert-True ($command.compile_seconds -eq 1.2 -and $command.execution_seconds -eq 0.2) `
        'Compile and execution durations must remain separate.'
    Assert-True ($command.build_occurred -eq 'yes' -and $command.exit_code -eq 0) `
        'Build and command result metadata were not persisted.'
    Assert-True (@($state.events | Where-Object { $_.kind -eq 'stage-ended' }).Count -eq 1) `
        'The stage must have one terminal timing event.'

    $reportOutput = (& $scriptPath -Action report -TrackingId test-card `
        -Directory $temporaryDirectory 6>&1) -join "`n"
    Assert-True ($reportOutput -match 'Breakdown: stages=.*between-stages=.*non-command-stage-work=') `
        'The report must expose stage, coordination, and non-command work separately.'
    Assert-True ($reportOutput -match 'Command wall:.*other-command-work=.*failed-commands=0') `
        'The report must expose command overhead and failed command count.'

    Write-Host 'track-card-migration lifecycle tests passed.' -ForegroundColor Green
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory -PathType Container) {
        $trackingFile = Join-Path $temporaryDirectory 'test-card.json'
        if (Test-Path -LiteralPath $trackingFile -PathType Leaf) {
            Remove-Item -LiteralPath $trackingFile -Force
        }
        Remove-Item -LiteralPath $temporaryDirectory -Force
    }
}

