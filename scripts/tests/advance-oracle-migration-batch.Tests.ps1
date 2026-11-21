$ErrorActionPreference = 'Stop'
$scriptPath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\advance-oracle-migration-batch.ps1'))
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("deepdeck-batch-advance-{0}" -f [Guid]::NewGuid().ToString('N'))
$utf8NoBom = [Text.UTF8Encoding]::new($false)
$unicodeTextPrefix = "{0}clair {1} co{2}t d{3}{4}t{4}" -f `
    [char]0x00C9, [char]0x2014, [char]0x00FB, [char]0x2019, [char]0x00E9

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw $Message }
}

function Write-Json($Value, [string] $Path) {
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 30), $utf8NoBom)
}

function Write-JsonLines($Values, [string] $Path) {
    $lines = @($Values | ForEach-Object { $_ | ConvertTo-Json -Depth 20 -Compress })
    [IO.File]::WriteAllText($Path, ($lines -join "`n") + "`n", $utf8NoBom)
}

function New-Audit([string] $Id, [string] $Name, [string] $Recognizer, [string] $Status = 'canonical') {
    return [pscustomobject]@{
        oracle_id = $Id; name = $Name; set = 'tst'; collector_number = $Id
        status = $Status
        abilities = @([pscustomobject]@{
            text = "$unicodeTextPrefix $Id"; type = 'staticAbility'; status = $Status
            recognizer = $Recognizer; rule = $null; diagnostics = @(); unknown = @()
        })
    }
}

try {
    [void][IO.Directory]::CreateDirectory($temporaryDirectory)
    $ids = @('a', 'b', 'c', 'd', 'e', 'f', 'g')
    $sources = @('recent', 'priority', 'league', 'recent', 'priority', 'league', 'recent')
    $queueRows = for ($index = 0; $index -lt $ids.Count; $index++) {
        [pscustomobject]@{
            global_rank = $index + 1; oracle_id = $ids[$index]; name = "Carte $($ids[$index])"
            selected_source = $sources[$index]; sources = @($sources[$index])
        }
    }
    $queuePath = Join-Path $temporaryDirectory 'queue.json'
    $auditPath = Join-Path $temporaryDirectory 'audit.jsonl'
    $oraclePath = Join-Path $temporaryDirectory 'oracle.json'
    $previousPath = Join-Path $temporaryDirectory 'previous.json'
    $previousAuditPath = Join-Path $temporaryDirectory 'previous-audit.jsonl'
    $outputPath = Join-Path $temporaryDirectory 'next.json'
    $inputOutputPath = Join-Path $temporaryDirectory 'next-input.json'

    Write-Json ([ordered]@{ queue = @($queueRows) }) $queuePath
    Write-JsonLines @($ids | ForEach-Object { New-Audit $_ "Carte $_" 'legacy' }) $auditPath
    Write-Json @($ids | ForEach-Object {
        [pscustomobject]@{ oracle_id = $_; name = "Carte $_"; oracle_text = "$unicodeTextPrefix $_" }
    }) $oraclePath
    Write-Json ([ordered]@{ cards = @($queueRows[0..3]) }) $previousPath
    Write-JsonLines @(
        (New-Audit 'a' 'Carte a' 'native'),
        (New-Audit 'b' 'Carte b' 'legacy'),
        (New-Audit 'c' 'Carte c' 'native' 'unsupported'),
        (New-Audit 'd' 'Carte d' 'native' 'unsupported')
    ) $previousAuditPath

    & $scriptPath -QueuePath $queuePath -QueueAuditPath $auditPath -OracleInputPath $oraclePath `
        -PreviousBatchPath $previousPath -PreviousBatchAuditPath $previousAuditPath `
        -OutputPath $outputPath -CardInputOutputPath $inputOutputPath -BatchSize 4 `
        -SourceTargets @{ recent = 2; priority = 1; league = 1 }

    $result = [IO.File]::ReadAllText($outputPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
    $selectedIds = @($result.cards | ForEach-Object { [string]$_.oracle_id })
    Assert-True (($selectedIds -join ',') -eq 'b,c,d,g') 'Unresolved cards must be retained and only the vacancy filled.'
    Assert-True ($result.retained_count -eq 3 -and $result.added_count -eq 1) 'Retention counts are incorrect.'
    Assert-True ($result.source_counts.recent -eq 2 -and $result.source_counts.priority -eq 1 -and $result.source_counts.league -eq 1) `
        'Provenance quotas were not preserved.'
    Assert-True ($result.baseline_hashes.queue_sha256 -eq (Get-FileHash $queuePath -Algorithm SHA256).Hash) `
        'Queue baseline hash was not preserved.'
    Assert-True ($result.baseline_hashes.previous_batch_audit_sha256 -eq (Get-FileHash $previousAuditPath -Algorithm SHA256).Hash) `
        'Fresh previous-audit hash was not preserved.'
    $selectedOracle = [IO.File]::ReadAllText($inputOutputPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
    Assert-True ($selectedOracle[0].oracle_text -eq "$unicodeTextPrefix b") 'UTF-8 Oracle text was corrupted.'

    $misalignedOraclePath = Join-Path $temporaryDirectory 'oracle-misaligned.json'
    $misaligned = @($ids | ForEach-Object { [pscustomobject]@{ oracle_id = $_; name = "Carte $_" } })
    $misaligned[2].oracle_id = 'wrong-id'
    Write-Json $misaligned $misalignedOraclePath
    $alignmentRejected = $false
    try {
        & $scriptPath -QueuePath $queuePath -QueueAuditPath $auditPath -OracleInputPath $misalignedOraclePath `
            -OutputPath (Join-Path $temporaryDirectory 'bad.json') `
            -CardInputOutputPath (Join-Path $temporaryDirectory 'bad-input.json') -BatchSize 4 `
            -SourceTargets @{ recent = 2; priority = 1; league = 1 }
    } catch { $alignmentRejected = $_.Exception.Message -match 'alignment mismatch' }
    Assert-True $alignmentRejected 'Index-level oracle_id misalignment must be rejected.'

    $duplicateQueuePath = Join-Path $temporaryDirectory 'queue-duplicate.json'
    $duplicateRows = @($queueRows | ForEach-Object { $_ | Select-Object * })
    $duplicateRows[6].oracle_id = 'a'
    Write-Json ([ordered]@{ queue = $duplicateRows }) $duplicateQueuePath
    $duplicateRejected = $false
    try {
        & $scriptPath -QueuePath $duplicateQueuePath -QueueAuditPath $auditPath -OracleInputPath $oraclePath `
            -OutputPath (Join-Path $temporaryDirectory 'duplicate.json') `
            -CardInputOutputPath (Join-Path $temporaryDirectory 'duplicate-input.json') -BatchSize 4 `
            -SourceTargets @{ recent = 2; priority = 1; league = 1 }
    } catch { $duplicateRejected = $_.Exception.Message -match 'duplicate oracle_id' }
    Assert-True $duplicateRejected 'Duplicate oracle_id values must be rejected.'

    Write-Host 'advance-oracle-migration-batch tests passed.' -ForegroundColor Green
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory -PathType Container) {
        $resolvedTemporary = [IO.Path]::GetFullPath($temporaryDirectory)
        $systemTemporary = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if (-not $resolvedTemporary.StartsWith($systemTemporary, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove unexpected test directory: $resolvedTemporary"
        }
        Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force
    }
}
