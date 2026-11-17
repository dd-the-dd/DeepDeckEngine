[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('SemanticPins','RunnerFingerprint')][string]$Mode,
    [Parameter(Mandatory=$true)][string]$RepoRoot,
    [Parameter(Mandatory=$true)][string]$InputJson,
    [Parameter(Mandatory=$true)][string]$OutputJson
)
$ErrorActionPreference='Stop'
$repoRoot=(Resolve-Path -LiteralPath $RepoRoot).Path
$outputPath=[IO.Path]::GetFullPath($OutputJson)
$temporaryRoot=(Resolve-Path -LiteralPath (Join-Path $repoRoot '.tmp')).Path.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar
if(-not $outputPath.StartsWith($temporaryRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Helper output must stay within repository/.tmp'}
if(Test-Path -LiteralPath $outputPath){throw 'Never overwrite a helper result'}
$request=Get-Content -LiteralPath $InputJson -Raw -Encoding UTF8 | ConvertFrom-Json
function Import-SelectedFunctions([string]$Path,[string[]]$Names){
    $parseTokens=$null;$parseErrors=$null
    $parsed=[Management.Automation.Language.Parser]::ParseFile($Path,[ref]$parseTokens,[ref]$parseErrors)
    if($parseErrors.Count){throw 'Official helper source does not parse'}
    $definitions=@($parsed.FindAll({param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $Names
    },$false))
    if($definitions.Count-ne$Names.Count){throw 'Official helper function set changed; review adapter'}
    return ($definitions | ForEach-Object {$_.Extent.Text}) -join "`n"
}
if($Mode-eq'SemanticPins'){
    $audit=Join-Path $repoRoot 'scripts/audit-card-set.ps1'
    . ([scriptblock]::Create((Import-SelectedFunctions $audit @('Get-Hash','Get-SemanticIrDigest'))))
    $pins=@($request.entries | ForEach-Object {
        [ordered]@{oracleId=$_.oracleId;phase=$_.phase;digest=(Get-SemanticIrDigest $_.rulesJson)}
    })
    $result=[ordered]@{mode=$Mode;implementation='scripts/audit-card-set.ps1::Get-SemanticIrDigest';pins=$pins}
}else{
    if($request.profile-notin@('runtime','parser','contract')){throw 'Unsupported runner profile'}
    $scriptName=switch($request.profile){runtime{'run-runtime-fixture.ps1'} parser{'run-parser-fixture.ps1'} contract{'run-contract-fixture.ps1'}}
    $wrapper=Join-Path $repoRoot ('scripts/'+$scriptName)
    . (Join-Path $repoRoot 'scripts/runner-fingerprint.ps1')
    if($request.profile-eq'parser'){
        . ([scriptblock]::Create((Import-SelectedFunctions $wrapper @('Get-ParserRunnerInputFiles','Get-RustCompilerIdentity','Get-ParserRunnerFingerprint'))))
        $inputs=@(Get-ParserRunnerInputFiles)
        $fingerprint=Get-ParserRunnerFingerprint
    }else{
        $inputFunction=if($request.profile-eq'runtime'){'Get-RuntimeRunnerInputFiles'}else{'Get-ContractRunnerInputFiles'}
        . ([scriptblock]::Create((Import-SelectedFunctions $wrapper @($inputFunction))))
        $parseTokens=$null;$parseErrors=$null
        $parsed=[Management.Automation.Language.Parser]::ParseFile($wrapper,[ref]$parseTokens,[ref]$parseErrors)
        $assignments=@($parsed.FindAll({param($node)
            $node-is[Management.Automation.Language.AssignmentStatementAst] -and
            $node.Left-is[Management.Automation.Language.VariableExpressionAst] -and
            $node.Left.VariablePath.UserPath-in@('fingerprintSchema','profile','buildArguments')
        },$false))
        if($assignments.Count-ne3){throw 'Official build/fingerprint argument assignments changed'}
        . ([scriptblock]::Create(($assignments | ForEach-Object {$_.Extent.Text}) -join "`n"))
        $inputs=@(& $inputFunction)
        $context=@{}
        if($request.profile-eq'contract'){$context=@{MTG_CARD_CATALOG_PATH=[Environment]::GetEnvironmentVariable('MTG_CARD_CATALOG_PATH')}}
        $fingerprint=Get-DeepDeckRunnerFingerprint -Schema $fingerprintSchema -Profile $profile -RepoRoot $repoRoot -InputFiles $inputs -BuildArguments $buildArguments -Context $context
    }
    $map=[ordered]@{}
    foreach($input in $inputs){
        $relative=$input.FullName.Substring($repoRoot.Length).TrimStart('\','/').Replace('\','/')
        $map[$relative]=(Get-FileHash -LiteralPath $input.FullName -Algorithm SHA256).Hash
    }
    $result=[ordered]@{mode=$Mode;profile=$request.profile;fingerprint=$fingerprint;inputCount=$inputs.Count;inputs=$map}
}
[IO.File]::WriteAllText($outputPath,($result|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
$result | Select-Object mode,profile,fingerprint,inputCount | ConvertTo-Json -Compress
