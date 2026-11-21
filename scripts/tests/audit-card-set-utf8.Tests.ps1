$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot '../audit-card-set.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Audit script must parse.' }
foreach ($name in @('Get-Property', 'Get-QueueOracle', 'Get-QueueSet', 'Get-QueueCollector',
    'Get-QueueKey', 'Read-Cards', 'Read-Queue', 'Read-CacheMetadata')) {
    $definition = $ast.Find({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
    }, $true)
    if ($null -eq $definition) { throw "Missing function: $name" }
    . ([scriptblock]::Create($definition.Extent.Text))
}
$cardPath = [IO.Path]::GetTempFileName()
$queuePath = [IO.Path]::GetTempFileName()
try {
    $typeLine = 'Instant ' + [char]0x2014 + ' Adventure'
    $cardName = [char]0x00C9 + 'cho ' + [char]0x2019 + ' TEST'
    $oracle = 'Target creature' + [char]0x2019 + 's controller draws a card.'
    $card = [ordered]@{ oracle_id = 'utf8-proof'; name = $cardName; layout = 'adventure'; type_line = $typeLine; oracle_text = $oracle }
    $utf8 = [Text.UTF8Encoding]::new($false)
    [IO.File]::WriteAllText($cardPath, (ConvertTo-Json -InputObject @($card) -Depth 10), $utf8)
    $cards = @(Read-Cards $cardPath)
    if ($cards.Count -ne 1 -or $cards[0].type_line -cne $typeLine -or
        $cards[0].name -cne $cardName -or $cards[0].oracle_text -cne $oracle) {
        throw 'BOM-less UTF-8 card text, case, and Adventure subtype separator must survive.'
    }
    [IO.File]::WriteAllText($queuePath, (@{ queue = @($card) } | ConvertTo-Json -Depth 10), $utf8)
    $QueueFile = $queuePath
    $queue = @(Read-Queue)
    if ($queue.Count -ne 1 -or $queue[0].name -cne $cardName) {
        throw 'BOM-less UTF-8 queue identity names must survive.'
    }
    $cached = Read-CacheMetadata $queuePath
    if ($cached.queue[0].name -cne $cardName) { throw 'Cache JSON must preserve UTF-8 too.' }
    Write-Host 'Audit UTF-8 proofs passed: cards, queue and cache; no Cargo.'
}
finally {
    Remove-Item -LiteralPath $cardPath
    Remove-Item -LiteralPath $queuePath
}
