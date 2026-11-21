[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Plan,
    [Parameter(Mandatory)][string] $RepositoryRoot,
    [switch] $PlanOnly,
    [switch] $CargoLaneAuthorized
)
$ErrorActionPreference = 'Stop'
$clock = [Diagnostics.Stopwatch]::StartNew()
$repo = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$planPath = (Resolve-Path -LiteralPath $Plan).Path
function FullPath([string] $path) {
    if ([IO.Path]::IsPathRooted($path)) { return [IO.Path]::GetFullPath($path) }
    return [IO.Path]::GetFullPath((Join-Path $repo $path))
}
function Keys($value, [string[]] $allowed, [string[]] $required) {
    if ($value -isnot [pscustomobject]) { throw 'Expected JSON object' }
    $names = @($value.PSObject.Properties.Name)
    foreach ($key in $names) { if ($key -cnotin $allowed) { throw "Unknown field: $key" } }
    foreach ($key in $required) { if ($key -cnotin $names) { throw "Missing field: $key" } }
}
function Save([string] $path, $value) {
    [IO.File]::WriteAllText($path, ($value | ConvertTo-Json -Depth 60), [Text.UTF8Encoding]::new($false))
}
$p = Get-Content -LiteralPath $planPath -Raw -Encoding utf8 | ConvertFrom-Json
Keys $p @('schema','capability','run_id','output_directory','stages','evidence') @('schema','capability','run_id','output_directory','stages')
if ($p.schema -cne 'deepdeck-capability-proof-plan/v1') { throw 'Unsupported schema' }
if ($p.run_id -cnotmatch '^[a-z0-9][a-z0-9._-]*$' -or [string]::IsNullOrWhiteSpace($p.capability)) { throw 'Invalid run_id/capability' }
if ($p.stages -isnot [array] -or $p.stages.Count -eq 0) { throw 'stages must be a nonempty array' }
$seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$validated = @()
foreach ($s in $p.stages) {
    Keys $s @('id','kind','fixtures','build','mode') @('id','kind','fixtures','build')
    if ($s.id -cnotmatch '^[a-z0-9][a-z0-9._-]*$' -or -not $seen.Add($s.id)) { throw 'Invalid/duplicate stage id' }
    if ($s.kind -cnotin @('parser','contract','runtime')) { throw 'Unsupported wrapper kind' }
    if ($s.build -isnot [bool]) { throw 'build must be boolean' }
    if ($s.kind -ceq 'parser') {
        if ($s.mode -cnotin @('Native','Full')) { throw 'Parser mode must be Native or Full' }
    } elseif ($null -ne $s.mode) { throw 'mode is only valid for parser' }
    if ($s.fixtures -isnot [array] -or $s.fixtures.Count -eq 0) { throw 'fixtures must be a nonempty array' }
    $paths = @(); $pins = @()
    foreach ($f in $s.fixtures) {
        if ($f -isnot [string]) { throw 'Fixture path must be a string' }
        $path = FullPath $f
        if (-not (Test-Path -LiteralPath $path)) { throw "Missing fixture: $path" }
        $paths += $path
        $files = if (Test-Path -LiteralPath $path -PathType Container) { @(Get-ChildItem -LiteralPath $path -Recurse -File -Filter '*.json') } else { @(Get-Item -LiteralPath $path) }
        if (-not $files.Count) { throw "No fixture files: $path" }
        foreach ($file in $files) { $pins += @{path=$file.FullName;sha256=(Get-FileHash -LiteralPath $file.FullName).Hash} }
    }
    $wrapper = Join-Path $repo "scripts/run-$($s.kind)-fixture.ps1"
    $validated += @{id=$s.id;kind=$s.kind;mode=$s.mode;build=$s.build;fixtures=$paths;fixture_pins=$pins;wrapper=$wrapper;wrapper_sha256=(Get-FileHash -LiteralPath $wrapper).Hash}
}
$evidence = @()
foreach ($e in $p.evidence) {
    Keys $e @('path','sha256') @('path','sha256')
    $path = FullPath $e.path; $sha = (Get-FileHash -LiteralPath $path).Hash
    if ($sha -cne $e.sha256) { throw "Evidence hash mismatch: $path" }
    $evidence += @{path=$path;sha256=$sha}
}
$output = FullPath $p.output_directory
if (Test-Path -LiteralPath $output) { throw 'Output exists; use a new output directory' }
if (-not $PlanOnly -and @($validated | Where-Object build).Count -and -not $CargoLaneAuthorized) { throw 'Build requires exclusive Cargo lane scheduling (-CargoLaneAuthorized)' }
[void][IO.Directory]::CreateDirectory($output)
$tracker = Join-Path $repo 'scripts/track-card-migration.ps1'
$report = [ordered]@{schema='deepdeck-capability-proof-run/v1';capability=$p.capability;plan_sha256=(Get-FileHash -LiteralPath $planPath).Hash;plan_only=[bool]$PlanOnly;validated_stages=$validated;evidence_pins=$evidence;tracker_sha256=(Get-FileHash -LiteralPath $tracker).Hash;stages=@();passed=$false;failed_stage=$null;error=$null;wall_seconds=$null}
Save (Join-Path $output 'validated-plan.json') $report
if ($PlanOnly) { $report.passed=$true;$report.wall_seconds=$clock.Elapsed.TotalSeconds;Save (Join-Path $output 'report.json') $report;exit 0 }
$tracking = Join-Path $output 'tracking'
& $tracker -Action begin -TrackingId $p.run_id -Card $p.capability -Directory $tracking -Details 'Shared capability proof execution; not card development or coverage' | Out-Null
$exitCode = 0
foreach ($s in $validated) {
    $wrapperReport = Join-Path $output "$($s.id).wrapper.json"
    $log = Join-Path $output "$($s.id).log"
    $arguments = @{Fixture=[string[]]$s.fixtures;ReportJson=$wrapperReport;TargetDirectory=(Join-Path $repo '.tmp/targets/card-migration')}
    if ($s.build) { $arguments.Build=$true }
    if ($s.kind -ceq 'parser' -and $s.mode -ceq 'Native') { $arguments.NativeOnly=$true }
    & $tracker -Action start-stage -TrackingId $p.run_id -Directory $tracking -Stage $s.id | Out-Null
    $stageClock = [Diagnostics.Stopwatch]::StartNew()
    $invocationError = $null
    $code = 1
    try {
        & $s.wrapper @arguments *> $log
        $code = $LASTEXITCODE
    } catch {
        $invocationError = $_.Exception.Message
        [IO.File]::WriteAllText(($log + '.launch-error'), $invocationError)
    }
    $stageClock.Stop()
    $original = $null
    $reportError = $null
    if (Test-Path -LiteralPath $wrapperReport) {
        try { $original = Get-Content -LiteralPath $wrapperReport -Raw -Encoding utf8 | ConvertFrom-Json }
        catch { $reportError = $_.Exception.Message }
    }
    $compile = if ($null -ne $original.compile_seconds) { $original.compile_seconds } elseif ($null -ne $original.build_seconds -and $original.build_seconds -eq 0) { 0.0 } else { $null }
    $execute = $original.execution_seconds; $queue = $original.queue_wait_seconds
    $fixtureErrors = @()
    if (Test-Path -LiteralPath $log) {
        foreach ($line in Get-Content -LiteralPath $log) {
            if ($line.TrimStart().StartsWith('{')) {
                try { $row = $line | ConvertFrom-Json; if ($row.type -ceq 'fixtureError') { $fixtureErrors += $row } } catch { }
            }
        }
    }
    $changedInputs = @()
    foreach ($pin in $s.fixture_pins) {
        try {
            $actualSHA = (Get-FileHash -LiteralPath $pin.path).Hash
            if ($actualSHA -cne $pin.sha256) { $changedInputs += @{path=$pin.path;expected_sha256=$pin.sha256;actual_sha256=$actualSHA;error=$null} }
        } catch { $changedInputs += @{path=$pin.path;expected_sha256=$pin.sha256;actual_sha256=$null;error=$_.Exception.Message} }
    }
    $ok = $null -eq $reportError -and $changedInputs.Count -eq 0 -and $fixtureErrors.Count -eq 0 -and $code -eq 0 -and $null -ne $original -and $original.passed
    $outcome = if ($ok) { 'passed' } else { 'failed' }
    $report.stages += @{id=$s.id;passed=$ok;exit_code=$code;wall_seconds=$stageClock.Elapsed.TotalSeconds;compile_seconds=$compile;aggregate_build_seconds=$original.build_seconds;execution_seconds=$execute;queue_wait_seconds=$queue;original_report=$wrapperReport;original_log=$log;invocation_error=$invocationError;wrapper_report_error=$reportError;wrapper_result=$original;fixture_errors=$fixtureErrors;changed_fixture_inputs=$changedInputs}
    $record = @{Action='record-command';TrackingId=$p.run_id;Directory=$tracking;Stage=$s.id;Command=($s.wrapper+' (structured arguments in validated-plan.json)');WallSeconds=$stageClock.Elapsed.TotalSeconds;Build=$(if($null -eq $original){'unknown'}elseif($original.build_occurred){'yes'}else{'no'});ExitCode=$code;Outcome=$outcome;Details="Original report/log: $wrapperReport / $log"}
    if ($null -ne $compile) { $record.CompileSeconds=[double]$compile }
    if ($null -ne $execute) { $record.ExecutionSeconds=[double]$execute }
    & $tracker @record | Out-Null
    & $tracker -Action end-stage -TrackingId $p.run_id -Directory $tracking -Stage $s.id -Outcome $outcome | Out-Null
    if (-not $ok) { $report.failed_stage=$s.id;$report.error=if($fixtureErrors.Count){$fixtureErrors}else{if($original.error){$original.error}else{if($invocationError){$invocationError}else{if($reportError){$reportError}else{if($changedInputs.Count){'Fixture inputs changed during execution'}else{'Wrapper reported failure; inspect original report/log'}}}}};$exitCode=1;break }
}
$report.passed=$exitCode -eq 0;$report.wall_seconds=$clock.Elapsed.TotalSeconds
Save (Join-Path $output 'report.json') $report
& $tracker -Action complete -TrackingId $p.run_id -Directory $tracking -Outcome $(if($report.passed){'proof-execution-passed'}else{'proof-execution-failed'}) -Details 'Runner execution only; integration and semantic linkage remain separate' | Out-Null
exit $exitCode
