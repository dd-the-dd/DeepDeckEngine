param(
    [string]$AnalyzerPath,
    [string]$AnalyzerInputPath,
    [string]$OutputDirectory = '.tmp/captured-process-tests/reusable'
)

$ErrorActionPreference = 'Stop'
$utf8NoBom = [Text.UTF8Encoding]::new($false)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$out = (Resolve-Path -LiteralPath $OutputDirectory).Path
$timer = [Diagnostics.Stopwatch]::StartNew()

function Invoke-CaptureProbe(
    [string]$Name,
    [double]$TimeoutSeconds,
    [string[]]$Command
) {
    $stdout = Join-Path $out "$Name.stdout.log"
    $stderr = Join-Path $out "$Name.stderr.log"
    $result = Join-Path $out "$Name.process.json"
    $arguments = @(
        'scripts/run-captured-process.py',
        '--timeout-seconds', [string]$TimeoutSeconds,
        '--stdout', $stdout,
        '--stderr', $stderr,
        '--result', $result,
        '--'
    ) + $Command
    $null = & python @arguments
    if ($LASTEXITCODE -ne 0) { throw "Captured-process helper failed for ${Name}: $LASTEXITCODE" }
    return Get-Content -Raw -Encoding UTF8 $result | ConvertFrom-Json
}

$exit0 = Invoke-CaptureProbe 'exit0' 5 @('powershell', '-NoProfile', '-Command', 'exit 0')
$exit7 = Invoke-CaptureProbe 'exit7' 5 @('powershell', '-NoProfile', '-Command', 'exit 7')
$timeout = Invoke-CaptureProbe 'timeout' 0.15 @('powershell', '-NoProfile', '-Command', 'Start-Sleep -Seconds 2')

if ($exit0.status -cne 'completed' -or $exit0.exitCode -ne 0) { throw 'exit0 contract failed' }
if ($exit7.status -cne 'completed' -or $exit7.exitCode -ne 7) { throw 'exit7 contract failed' }
if ($timeout.status -cne 'timeout' -or -not $timeout.timedOut -or $null -eq $timeout.exitCode) {
    throw 'timeout contract failed'
}

$analyzer = $null
if ($AnalyzerPath -or $AnalyzerInputPath) {
    if (-not $AnalyzerPath -or -not $AnalyzerInputPath) {
        throw 'AnalyzerPath and AnalyzerInputPath must be provided together.'
    }
    $analyzerOutput = Join-Path $out 'analyzer-output.jsonl'
    $oldMode = $env:DEEPDECK_ORACLE_NATIVE_ONLY
    try {
        $env:DEEPDECK_ORACLE_NATIVE_ONLY = '1'
        $analyzer = Invoke-CaptureProbe 'analyzer' 30 @(
            (Resolve-Path -LiteralPath $AnalyzerPath).Path,
            (Resolve-Path -LiteralPath $AnalyzerInputPath).Path,
            $analyzerOutput
        )
    } finally {
        if ($null -eq $oldMode) { Remove-Item Env:DEEPDECK_ORACLE_NATIVE_ONLY -ErrorAction SilentlyContinue }
        else { $env:DEEPDECK_ORACLE_NATIVE_ONLY = $oldMode }
    }
    if ($analyzer.status -cne 'completed' -or $analyzer.exitCode -ne 0) {
        throw 'analyzer contract failed'
    }
    $inputCards = Get-Content -Raw -Encoding UTF8 $AnalyzerInputPath | ConvertFrom-Json
    $expectedRows = @($inputCards).Count
    $actualRows = @([IO.File]::ReadLines($analyzerOutput) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($actualRows -ne $expectedRows) {
        throw "analyzer row-count mismatch: expected $expectedRows, got $actualRows"
    }
}

$timer.Stop()
$report = [ordered]@{
    schemaVersion = 'deepdeck-captured-process-tests/v1'
    status = 'passed'
    wallSeconds = [Math]::Round($timer.Elapsed.TotalSeconds, 3)
    exit0 = $exit0
    exit7 = $exit7
    timeout = $timeout
    analyzer = $analyzer
}
$reportPath = Join-Path $out 'report.json'
[IO.File]::WriteAllText($reportPath, (ConvertTo-Json $report -Depth 10), $utf8NoBom)
$report | ConvertTo-Json -Depth 10
